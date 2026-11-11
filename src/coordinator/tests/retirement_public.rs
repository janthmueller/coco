use super::*;
use crate::domain::retirement::{WorkspaceDeletionReceipt, WorkspaceDeletionState};
use crate::protocol::{
    WorkspaceDeleteOperationParams, WorkspaceDeleteRequest, WorkspaceDeletionGetParams,
};

mod policies;
mod recovery;

fn request(fixture: &Fixture, workspace: &Workspace) -> WorkspaceDeleteOperationParams {
    WorkspaceDeleteOperationParams {
        repository_path: fixture.source.clone(),
        request: WorkspaceDeleteRequest {
            operation_id: "public-delete-1".into(),
            workspace: workspace.name.clone(),
            keep_thread: false,
            keep_branch: false,
            discard_changes: false,
            discard_unretained_commits: false,
        },
        dry_run: false,
        expected_plan_sha256: None,
    }
}

fn lookup(fixture: &Fixture, request: &WorkspaceDeleteOperationParams) -> WorkspaceDeletionReceipt {
    fixture
        .coordinator
        .get_workspace_deletion(WorkspaceDeletionGetParams {
            repository_path: request.repository_path.clone(),
            operation_id: request.request.operation_id.clone(),
        })
        .unwrap()
}

async fn preview(
    fixture: &Fixture,
    request: &WorkspaceDeleteOperationParams,
) -> WorkspaceDeletionReceipt {
    let mut request = request.clone();
    request.dry_run = true;
    fixture
        .coordinator
        .delete_workspace_operation(request)
        .await
        .unwrap()
}

#[tokio::test]
async fn preview_reserves_nothing_and_defaults_delete_all_owned_resources() {
    let fixture = Fixture::new(FakeWorker::default());
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let params = request(&fixture, &workspace);
    let observed = preview(&fixture, &params).await;
    assert_eq!(observed.state, WorkspaceDeletionState::Preview);
    assert_eq!(
        lookup(&fixture, &params).state,
        WorkspaceDeletionState::Unknown
    );
    let plan = observed.plan.unwrap();
    assert_eq!(
        observed.plan_sha256.as_deref(),
        Some(plan.sha256().unwrap().as_str())
    );
    assert!(plan.remove_worktree && plan.delete_branch && plan.blockers.is_empty());
    assert_eq!(plan.thread_disposition, WorkspaceThreadDisposition::Delete);
    assert!(workspace.worktree_path.as_ref().unwrap().is_dir());
    let deleted = fixture
        .coordinator
        .delete_workspace_operation(params.clone())
        .await
        .unwrap();
    assert_eq!(deleted.state, WorkspaceDeletionState::Deleted);
    assert!(deleted.created_at_ms.is_some() && deleted.error_code.is_none());
    assert!(
        fixture
            .store
            .workspace_by_id(&workspace.id)
            .unwrap()
            .is_none()
    );
    assert!(!workspace.worktree_path.as_ref().unwrap().exists());
    assert!(
        !fixture
            .worker
            .native_threads
            .lock()
            .unwrap()
            .contains_key(workspace.codex_thread_id.as_ref().unwrap())
    );
    assert_eq!(lookup(&fixture, &params), deleted);
}

#[tokio::test]
async fn lost_reply_replays_old_receipt_without_resolving_a_reused_name() {
    let fixture = Fixture::new(FakeWorker::default());
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let params = request(&fixture, &workspace);
    let deleted = fixture
        .coordinator
        .delete_workspace_operation(params.clone())
        .await
        .unwrap();
    let mut create = fixture.create_params();
    create.operation_id = "replacement-create".into();
    let replacement = fixture
        .coordinator
        .create_workspace(create)
        .await
        .unwrap()
        .workspace;
    assert_ne!(workspace.id, replacement.id);
    let calls = fixture.worker.calls();
    assert_eq!(
        fixture
            .coordinator
            .delete_workspace_operation(params.clone())
            .await
            .unwrap(),
        deleted
    );
    assert_eq!(preview(&fixture, &params).await, deleted);
    assert_eq!(lookup(&fixture, &params), deleted);
    assert_eq!(
        fixture.worker.calls(),
        calls,
        "replay/lookup must not consult native execution"
    );
    assert!(replacement.worktree_path.as_ref().unwrap().is_dir());
    let mut changed = params;
    changed.request.keep_thread = true;
    assert_eq!(
        fixture
            .coordinator
            .delete_workspace_operation(changed)
            .await
            .unwrap_err()
            .code(),
        "IDEMPOTENCY_CONFLICT"
    );
}

#[tokio::test]
async fn a_reviewed_plan_cannot_be_used_for_a_replacement_or_a_changed_head() {
    let fixture = Fixture::new(FakeWorker::default());
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let mut params = request(&fixture, &workspace);
    params.expected_plan_sha256 = preview(&fixture, &params).await.plan_sha256;
    fs::write(
        workspace.worktree_path.as_ref().unwrap().join("later"),
        "new file",
    )
    .unwrap();
    assert_eq!(
        fixture
            .coordinator
            .delete_workspace_operation(params.clone())
            .await
            .unwrap_err()
            .code(),
        "WORKSPACE_RETIREMENT_BLOCKED"
    );
    assert_eq!(
        lookup(&fixture, &params).state,
        WorkspaceDeletionState::Unknown
    );
    fixture
        .coordinator
        .delete_workspace(super::retirement_deletion::delete_params(
            &fixture, &workspace,
        ))
        .await
        .unwrap_err();
    fs::remove_file(workspace.worktree_path.as_ref().unwrap().join("later")).unwrap();
    fixture
        .coordinator
        .delete_workspace(super::retirement_deletion::delete_params(
            &fixture, &workspace,
        ))
        .await
        .unwrap();
    let mut create = fixture.create_params();
    create.operation_id = "replacement".into();
    let replacement = fixture
        .coordinator
        .create_workspace(create)
        .await
        .unwrap()
        .workspace;
    assert_eq!(
        fixture
            .coordinator
            .delete_workspace_operation(params)
            .await
            .unwrap_err()
            .code(),
        "WORKSPACE_RETIREMENT_BLOCKED"
    );
    assert!(replacement.worktree_path.as_ref().unwrap().is_dir());
}

#[tokio::test]
async fn repository_and_operation_ownership_are_enforced_before_dispatch() {
    let fixture = Fixture::new(FakeWorker::default());
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let params = request(&fixture, &workspace);
    let other = fixture._temp.path().join("other");
    git_output(
        fixture._temp.path(),
        &["init", "--initial-branch=main", other.to_str().unwrap()],
    );
    fixture
        .coordinator
        .register_repository(RepositoryRegisterParams {
            path: other.clone(),
        })
        .await
        .unwrap();
    git_output(
        &other,
        &[
            "-c",
            "user.name=CoCo Tests",
            "-c",
            "user.email=coco@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            "fixture",
        ],
    );
    let mut create_alias = fixture.create_params();
    create_alias.repository_path = other.clone();
    create_alias.name = workspace.id.clone();
    create_alias.operation_id = "foreign-uuid-name".into();
    let alias = fixture
        .coordinator
        .create_workspace(create_alias)
        .await
        .unwrap()
        .workspace;
    let mut foreign = params.clone();
    foreign.repository_path = other;
    foreign.request.workspace = workspace.id.clone();
    assert_eq!(
        fixture
            .coordinator
            .delete_workspace_operation(foreign.clone())
            .await
            .unwrap_err()
            .code(),
        "WORKSPACE_NOT_FOUND"
    );
    assert!(
        alias.worktree_path.as_ref().unwrap().is_dir(),
        "foreign UUID must not fall back to a local same-named workspace"
    );
    let deleted = fixture
        .coordinator
        .delete_workspace_operation(params)
        .await
        .unwrap();
    assert_eq!(deleted.state, WorkspaceDeletionState::Deleted);
    assert_eq!(
        lookup(&fixture, &foreign).state,
        WorkspaceDeletionState::Unknown
    );
    assert_eq!(
        fixture
            .coordinator
            .delete_workspace_operation(foreign)
            .await
            .unwrap_err()
            .code(),
        "IDEMPOTENCY_CONFLICT"
    );
}

#[tokio::test]
async fn deleting_the_original_scope_worktree_still_returns_its_confirmed_receipt() {
    let fixture = Fixture::new(FakeWorker::default());
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let mut params = request(&fixture, &workspace);
    params.repository_path = workspace.worktree_path.clone().unwrap();
    let result = fixture
        .coordinator
        .delete_workspace_operation(params.clone())
        .await
        .unwrap();
    assert_eq!(result.state, WorkspaceDeletionState::Deleted);
    assert!(!params.repository_path.exists());
    params.repository_path = fixture.source.clone();
    assert_eq!(lookup(&fixture, &params), result);
    assert_eq!(
        fixture
            .coordinator
            .delete_workspace_operation(params)
            .await
            .unwrap(),
        result
    );
}
