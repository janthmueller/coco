use std::fs;
use std::path::Path;

use serde_json::json;

use super::*;
use crate::hooks::{GuardRejection, HookRegistry};

fn registry(root: &Path, guards: Value) -> HookRegistry {
    let path = root.join("hooks.json");
    fs::write(
        &path,
        serde_json::to_vec(&json!({
            "version": 1,
            "guards": guards,
        }))
        .unwrap(),
    )
    .unwrap();
    HookRegistry::load(&path).unwrap()
}

fn close_params(fixture: &Fixture, workspace: &Workspace, dry_run: bool) -> WorkspaceCloseParams {
    WorkspaceCloseParams {
        scope: RepositoryScope::repository(fixture.source.clone()),
        workspace: workspace.id.clone(),
        archive_thread: false,
        discard_changes: false,
        dry_run,
        expected_plan: None,
    }
}

fn delete_params(fixture: &Fixture, workspace: &Workspace) -> WorkspaceDeleteParams {
    WorkspaceDeleteParams {
        scope: RepositoryScope::repository(fixture.source.clone()),
        workspace: workspace.id.clone(),
        delete_thread: false,
        delete_branch: false,
        dry_run: false,
        expected_plan: None,
    }
}

#[tokio::test]
async fn close_guard_runs_after_preview_and_before_any_side_effect() {
    let temporary = tempfile::tempdir().unwrap();
    let capture = temporary.path().join("close-guard-ran");
    let hooks = registry(
        temporary.path(),
        json!([{
            "id": "protect-close",
            "action": "workspace.close",
            "command": [
                "/bin/sh",
                "-c",
                "IFS= read -r request || true; printf ran > \"$1\"; printf '%s' '{\"decision\":\"deny\",\"reason\":\"external work is open\"}'",
                "coco-guard",
                capture
            ],
            "onError": "deny"
        }]),
    );
    let fixture = Fixture::new_with_hooks(FakeWorker::default(), hooks);
    fixture.register().await;
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let worktree = workspace.worktree_path.clone().unwrap();

    let preview = fixture
        .coordinator
        .close_workspace(close_params(&fixture, &workspace, true))
        .await
        .unwrap();
    assert!(!preview.applied);
    assert!(!capture.exists(), "dry-run unexpectedly executed the guard");

    let error = fixture
        .coordinator
        .close_workspace(close_params(&fixture, &workspace, false))
        .await
        .unwrap_err();
    assert_eq!(error.code(), "GUARD_DENIED");
    assert_eq!(
        error.data(),
        Some(json!({
            "guardId": "protect-close",
            "action": "workspace.close",
            "reason": "external work is open",
        }))
    );
    assert!(matches!(
        error,
        CoordinatorError::Guard(GuardRejection::Denied { guard_id, reason, .. })
            if guard_id == "protect-close" && reason == "external work is open"
    ));
    assert!(capture.is_file());
    assert!(worktree.is_dir());
    assert_eq!(
        fixture
            .store
            .workspace_by_id(&workspace.id)
            .unwrap()
            .unwrap()
            .availability,
        WorkspaceAvailability::Open
    );
}

#[tokio::test]
async fn delete_guard_failure_closes_safely_without_deleting_the_record() {
    let temporary = tempfile::tempdir().unwrap();
    let hooks = registry(
        temporary.path(),
        json!([{
            "id": "protect-delete",
            "action": "workspace.delete",
            "command": ["/bin/sh", "-c", "printf not-json"],
            "onError": "deny"
        }]),
    );
    let fixture = Fixture::new_with_hooks(FakeWorker::default(), hooks);
    fixture.register().await;
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    fixture
        .coordinator
        .close_workspace(close_params(&fixture, &workspace, false))
        .await
        .unwrap();

    let error = fixture
        .coordinator
        .delete_workspace(delete_params(&fixture, &workspace))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        CoordinatorError::Guard(GuardRejection::FailedClosed { guard_id, .. })
            if guard_id == "protect-delete"
    ));
    assert_eq!(
        fixture
            .store
            .workspace_by_id(&workspace.id)
            .unwrap()
            .unwrap()
            .availability,
        WorkspaceAvailability::Closed
    );
}

#[tokio::test]
async fn explicit_fail_open_policy_allows_the_checked_action() {
    let temporary = tempfile::tempdir().unwrap();
    let hooks = registry(
        temporary.path(),
        json!([{
            "id": "advisory-close",
            "action": "workspace.close",
            "command": ["/bin/sh", "-c", "exit 9"],
            "onError": "allow"
        }]),
    );
    let fixture = Fixture::new_with_hooks(FakeWorker::default(), hooks);
    fixture.register().await;
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;

    let closed = fixture
        .coordinator
        .close_workspace(close_params(&fixture, &workspace, false))
        .await
        .unwrap();

    assert!(closed.applied);
    assert_eq!(closed.workspace.availability, WorkspaceAvailability::Closed);
}

#[tokio::test]
async fn recovery_finishes_an_already_authorized_close_without_rerunning_guards() {
    let temporary = tempfile::tempdir().unwrap();
    let capture = temporary.path().join("recovery-guard-ran");
    let hooks = registry(
        temporary.path(),
        json!([{
            "id": "protect-close",
            "action": "workspace.close",
            "command": [
                "/bin/sh",
                "-c",
                "printf ran > \"$1\"; printf '%s' '{\"decision\":\"deny\",\"reason\":\"do not rerun\"}'",
                "coco-guard",
                capture
            ],
            "onError": "deny"
        }]),
    );
    let fixture = Fixture::new_with_hooks(FakeWorker::default(), hooks);
    fixture.register().await;
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let path = workspace.worktree_path.as_ref().unwrap();
    let (_, repository) = fixture
        .coordinator
        .git_repository_for_workspace(&workspace)
        .unwrap();
    let observation = fixture
        .coordinator
        .git
        .observe_worktree_retirement(
            &repository,
            path,
            workspace.worktree_mode,
            workspace.branch_name.as_deref(),
        )
        .unwrap();
    fixture
        .store
        .begin_workspace_close(&workspace.id, &observation.binding.head_sha, false)
        .unwrap();
    fixture
        .coordinator
        .git
        .remove_worktree(&repository, &observation.binding, false)
        .unwrap();

    assert_eq!(fixture.coordinator.recover_workspace_retirements().await, 1);
    assert!(!capture.exists());
    assert_eq!(
        fixture
            .store
            .workspace_by_id(&workspace.id)
            .unwrap()
            .unwrap()
            .availability,
        WorkspaceAvailability::Closed
    );
}
