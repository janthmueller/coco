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
        self.requests.lock().unwrap().push(params);
        Ok(serde_json::to_value(WorkspaceDeleteResult {
            plan: self.plan.clone(),
            applied,
        })
        .unwrap())
    }
}

#[derive(Default)]
struct ConfirmOnce(usize);

impl Interaction for ConfirmOnce {
    fn is_interactive(&self) -> bool {
        true
    }

    fn select(&mut self, _title: &str, _choices: &[Choice]) -> Result<usize> {
        panic!("yes/no confirmation must not open the picker")
    }

    fn confirm(&mut self, title: &str) -> Result<bool> {
        assert!(title.contains("Permanently apply"));
        self.0 += 1;
        Ok(true)
    }

    fn text(&mut self, _label: &str) -> Result<String> {
        panic!("unexpected text prompt")
    }
}

#[tokio::test]
async fn deletion_confirmation_sends_the_previewed_id_and_resource_plan() {
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
    };
    let plan = WorkspaceRetirementPlan {
        workspace_id: "exact-previewed-id".to_owned(),
        workspace_name: "reused-name".to_owned(),
        worktree_path: root.join("worktrees/reused-name"),
        head_sha: Some("a".repeat(40)),
        branch_name: Some("coco/reused-name".to_owned()),
        thread_id: Some("exact-thread-id".to_owned()),
        thread_disposition: WorkspaceThreadDisposition::Delete,
        delete_branch: true,
        tracked_changes: false,
        untracked_file_count: 0,
        ignored_file_count: 0,
        detached_commits: false,
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
    let mut interaction = ConfirmOnce::default();
    run_delete(
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
            delete_thread: true,
            delete_branch: true,
            dry_run: false,
            yes: false,
        },
        &mut interaction,
    )
    .await
    .unwrap();
    shutdown.send(true).unwrap();
    server.await.unwrap().unwrap();
    assert_eq!(interaction.0, 1);
    let requests = handler.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].workspace, "reused-name");
    assert!(requests[0].dry_run);
    assert_eq!(requests[1].workspace, plan.workspace_id);
    assert!(!requests[1].dry_run);
    assert_eq!(requests[1].expected_plan.as_ref(), Some(&plan));
}
