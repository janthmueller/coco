use super::retirement::close_params;
use super::retirement_deletion::delete_params;
use super::*;
use crate::domain::WorktreeMode;
use crate::git::{GitError, WorktreeTarget};

#[tokio::test]
async fn new_default_branches_use_the_exact_workspace_name_and_replay_the_same_binding() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let params = fresh_create_params(fixture.source.clone(), "feat/login", "create-login");
    let created = fixture
        .coordinator
        .create_workspace(params.clone())
        .await
        .unwrap()
        .workspace;
    assert_eq!(created.branch_name.as_deref(), Some("feat/login"));
    assert_eq!(created.worktree_mode, WorktreeMode::NewBranch);
    assert_eq!(
        git_output(
            created.worktree_path.as_ref().unwrap(),
            &["symbolic-ref", "--short", "HEAD"]
        ),
        "feat/login"
    );
    assert!(git_output(&fixture.source, &["branch", "--list", "coco/feat/login"]).is_empty());
    let replay = fixture
        .coordinator
        .create_workspace(params)
        .await
        .unwrap()
        .workspace;
    assert_eq!(replay.id, created.id);
    assert_eq!(replay.branch_name, created.branch_name);
}

#[tokio::test]
async fn default_branch_collisions_do_not_adopt_an_existing_ref_or_create_a_fallback() {
    for (existing, requested) in [
        ("fix/login", "fix/login"),
        ("feat", "feat/login"),
        ("feat/login", "feat"),
        ("main", "main"),
    ] {
        let fixture = Fixture::new(FakeWorker::default());
        let repository = fixture.register().await;
        if existing != "main" {
            run_git(&fixture.source, &["branch", existing]);
        }
        let refs = git_output(
            &fixture.source,
            &[
                "for-each-ref",
                "--format=%(refname):%(objectname)",
                "refs/heads",
            ],
        );
        let error = fixture
            .coordinator
            .create_workspace(fresh_create_params(
                fixture.source.clone(),
                requested,
                "collision",
            ))
            .await
            .unwrap_err();
        assert!(
            matches!(error, CoordinatorError::Git(GitError::BranchCollision { requested: branch, existing: other }) if branch == requested && other == existing)
        );
        assert_eq!(
            git_output(
                &fixture.source,
                &[
                    "for-each-ref",
                    "--format=%(refname):%(objectname)",
                    "refs/heads"
                ]
            ),
            refs
        );
        assert!(
            fixture
                .store
                .list_workspaces(Some(&repository.id))
                .unwrap()
                .is_empty()
        );
        assert!(!fixture.worktrees.exists());
        assert!(fixture.worker.calls().is_empty());
    }
}

/// Reconstruct the old default's persisted binding without deriving its branch
/// from the current code. Its original request intentionally has branch: null.
async fn legacy_default(fixture: &Fixture, params: &WorkspaceCreateParams) -> Workspace {
    let template = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let repository = fixture.coordinator.git.discover(&fixture.source).unwrap();
    let plan = fixture
        .coordinator
        .git
        .plan_worktree(
            &repository,
            &fixture.worktrees,
            &params.name,
            WorktreeTarget::NewBranch {
                branch_name: format!("coco/{}", params.name),
            },
            template.base_sha.as_deref().unwrap(),
        )
        .unwrap();
    let (workspace, _) = fixture
        .store
        .create_workspace_with_event(
            crate::store::NewWorkspace {
                create_operation_id: Some(params.operation_id.clone()),
                temporary: None,
                repository_id: repository.id.clone(),
                name: params.name.clone(),
                context_mode: template.context_mode,
                context: template.context,
                profile: template.profile,
                worktree_mode: plan.mode,
                branch_name: plan.branch_name.clone(),
                base_sha: Some(plan.base_sha.clone()),
                worktree_path: Some(plan.path.clone()),
            },
            EventDraft::workspace(EventKind::WorkspaceCreated, EventSource::Coco, json!({})),
        )
        .unwrap();
    fixture
        .coordinator
        .git
        .create_worktree(&repository, &plan)
        .unwrap();
    fixture
        .store
        .transition_workspace_lifecycle_with_event(
            &workspace.id,
            WorkspaceLifecycle::Provisioning,
            WorkspaceLifecycle::Ready,
            None,
            EventDraft::workspace(EventKind::WorktreeCreated, EventSource::Git, json!({})),
        )
        .unwrap()
        .0
}

#[tokio::test]
async fn old_default_survives_restart_replay_and_retirement_without_touching_a_same_named_branch() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let params = fresh_create_params(fixture.source.clone(), "legacy/review", "old-create");
    let workspace = legacy_default(&fixture, &params).await;
    let branch = workspace.branch_name.clone().unwrap();
    assert_eq!(branch, "coco/legacy/review");
    run_git(&fixture.source, &["branch", &params.name]);
    let independent_head = git_output(&fixture.source, &["rev-parse", &params.name]);
    let recovered = fixture.recovery_coordinator(Arc::clone(&fixture.worker), "after-upgrade");
    let replay = recovered
        .create_workspace(params.clone())
        .await
        .unwrap()
        .workspace;
    assert_eq!(replay.id, workspace.id);
    assert_eq!(replay.branch_name.as_deref(), Some(branch.as_str()));
    let closed = recovered
        .close_workspace(close_params(&fixture, &replay))
        .await
        .unwrap();
    assert_eq!(
        closed.workspace.branch_name.as_deref(),
        Some(branch.as_str())
    );
    let reopened = recovered
        .reopen_workspace(WorkspaceReopenParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id.clone(),
        })
        .await
        .unwrap()
        .workspace;
    assert_eq!(
        git_output(
            reopened.worktree_path.as_ref().unwrap(),
            &["symbolic-ref", "--short", "HEAD"]
        ),
        branch
    );
    recovered
        .delete_workspace(delete_params(&fixture, &reopened))
        .await
        .unwrap();
    assert!(git_output(&fixture.source, &["branch", "--list", &branch]).is_empty());
    assert_eq!(
        git_output(&fixture.source, &["rev-parse", &params.name]),
        independent_head
    );
}
