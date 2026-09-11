use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::watch;

use super::*;
use crate::protocol::{WorkspaceDeleteResult, WorkspaceRetirementPlan, WorkspaceThreadDisposition};
use crate::rpc::{RpcErrorPayload, RpcHandler, RpcServer};

struct CaptureDelete {
    plan: WorkspaceRetirementPlan,
    requests: Mutex<Vec<WorkspaceDeleteParams>>,
}

#[async_trait]
impl RpcHandler for CaptureDelete {
    async fn handle(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> std::result::Result<serde_json::Value, RpcErrorPayload> {
        assert_eq!(method, "workspace.delete");
        let params: WorkspaceDeleteParams = serde_json::from_value(params).unwrap();
        let applied = !params.dry_run;
        let mut plan = self.plan.clone();
        if plan.has_local_changes() && !params.discard_changes {
            plan.blockers.push("requires --discard-changes".into());
        }
        if plan.unretained_commit_count > 0 && !params.discard_unretained_commits {
            plan.blockers
                .push("requires --discard-unretained-commits".into());
        }
        self.requests.lock().unwrap().push(params);
        Ok(serde_json::to_value(WorkspaceDeleteResult { plan, applied }).unwrap())
    }
}

struct ConfirmOnce {
    questions: Vec<String>,
    answer: bool,
}

impl Interaction for ConfirmOnce {
    fn is_interactive(&self) -> bool {
        true
    }

    fn select(&mut self, _title: &str, _choices: &[Choice]) -> Result<usize> {
        panic!("yes/no confirmation must not open the picker")
    }

    fn confirm(&mut self, title: &str) -> Result<bool> {
        self.questions.push(title.to_owned());
        Ok(self.answer)
    }

    fn text(&mut self, _label: &str) -> Result<String> {
        panic!("unexpected text prompt")
    }
}

async fn exercise_deletion(
    changes: bool,
    answer: bool,
    yes: bool,
) -> (
    Result<()>,
    Vec<WorkspaceDeleteParams>,
    Vec<String>,
    WorkspaceRetirementPlan,
) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let paths = CocoPaths {
        data_dir: root.to_owned(),
        database_path: root.join("state.db"),
        socket_path: root.join("cocod.sock"),
        codex_endpoint_path: root.join("endpoint.json"),
        codex_token_path: root.join("token"),
        worktrees_dir: root.join("worktrees"),
        codex_home: root.join("codex"),
        hooks_path: root.join("hooks.json"),
    };
    let plan = WorkspaceRetirementPlan {
        workspace_id: "exact-previewed-id".to_owned(),
        workspace_name: "reused-name".to_owned(),
        worktree_path: root.join("worktrees/reused-name"),
        remove_worktree: false,
        head_sha: Some("a".repeat(40)),
        branch_name: Some("coco/reused-name".to_owned()),
        thread_id: Some("exact-thread-id".to_owned()),
        thread_disposition: WorkspaceThreadDisposition::Delete,
        delete_branch: true,
        tracked_changes: changes,
        untracked_file_count: 0,
        ignored_file_count: 0,
        detached_commits: false,
        unretained_commit_count: usize::from(changes),
        descendant_thread_count: 0,
        blockers: Vec::new(),
    };
    let handler = Arc::new(CaptureDelete {
        plan: plan.clone(),
        requests: Mutex::new(Vec::new()),
    });
    let server = RpcServer::bind(&paths.socket_path, handler.clone())
        .await
        .unwrap();
    let (shutdown, receiver) = watch::channel(false);
    let server = tokio::spawn(server.run(receiver));
    let mut interaction = ConfirmOnce {
        questions: Vec::new(),
        answer,
    };
    let result = run_delete(
        &paths,
        WorkspaceSelection {
            repository_scope: RepositoryScope::repository(root),
            has_explicit_path: false,
            workspace: Some("reused-name".to_owned()),
            global: false,
        },
        DeleteArgs {
            workspace: Some("reused-name".to_owned()),
            global: false,
            keep_thread: false,
            keep_branch: false,
            discard_changes: false,
            discard_unretained_commits: false,
            dry_run: false,
            yes,
        },
        &mut interaction,
    )
    .await;
    shutdown.send(true).unwrap();
    server.await.unwrap().unwrap();
    let requests = handler.requests.lock().unwrap().clone();
    (result, requests, interaction.questions, plan)
}

#[tokio::test]
async fn deletion_confirmation_sends_the_previewed_id_and_resource_plan() {
    let (result, requests, questions, plan) = exercise_deletion(false, true, false).await;
    result.unwrap();
    assert_eq!(questions, ["Permanently apply this deletion plan?"]);
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].workspace, "reused-name");
    assert!(requests[0].dry_run);
    assert_eq!(requests[1].workspace, plan.workspace_id);
    assert!(!requests[1].dry_run);
    assert_eq!(requests[1].expected_plan.as_ref(), Some(&plan));
}

#[tokio::test]
async fn deletion_requires_one_explicit_confirmation_for_both_kinds_of_loss() {
    let (result, requests, questions, plan) = exercise_deletion(true, true, false).await;
    result.unwrap();
    assert_eq!(requests.len(), 3);
    assert!(!requests[0].discard_changes && !requests[0].discard_unretained_commits);
    assert!(
        requests[1].dry_run
            && requests[1].discard_changes
            && requests[1].discard_unretained_commits
    );
    assert!(
        !requests[2].dry_run
            && requests[2].discard_changes
            && requests[2].discard_unretained_commits
    );
    assert_eq!(requests[2].expected_plan.as_ref(), Some(&plan));
    assert_eq!(questions.len(), 1);
    assert!(questions[0].contains("local changes and unretained commits"));
}

#[tokio::test]
async fn declining_loss_never_submits_an_apply_request() {
    let (result, requests, questions, _) = exercise_deletion(true, false, false).await;
    assert!(result.unwrap_err().to_string().contains("cancelled"));
    assert_eq!(questions.len(), 1);
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|r| r.dry_run));
}

#[tokio::test]
async fn yes_alone_never_grants_file_or_commit_discard() {
    let (result, requests, questions, _) = exercise_deletion(true, true, true).await;
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("--yes does not authorize")
    );
    assert!(questions.is_empty());
    assert_eq!(requests.len(), 1);
    assert!(requests[0].dry_run);
    assert!(!requests[0].discard_changes && !requests[0].discard_unretained_commits);
}
