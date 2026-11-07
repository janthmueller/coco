use super::*;
use crate::domain::TemporaryWorkspacePolicy;
use crate::protocol::{WorkspaceAttachFinishParams, WorkspaceCleanupResult};

mod auxiliary;
mod safety;

async fn temporary(fixture: &Fixture, bound: bool, keep_thread: bool) -> Workspace {
    fixture.register().await;
    let mut params = fixture.create_params();
    params.temporary = Some(TemporaryWorkspacePolicy { keep_thread });
    if bound {
        fixture.create_and_materialize(params).await
    } else {
        fixture
            .coordinator
            .create_workspace(params)
            .await
            .unwrap()
            .workspace
    }
}

async fn attach(fixture: &Fixture, workspace: &Workspace) -> WorkspaceAttachFinishParams {
    let attached = fixture
        .coordinator
        .attach_workspace(WorkspaceAttachParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id.clone(),
            client: None,
        })
        .await
        .unwrap();
    let lease_id = match attached.launch {
        WorkspaceAttachLaunch::Start { lease_id }
        | WorkspaceAttachLaunch::Resume { lease_id, .. } => lease_id,
    };
    WorkspaceAttachFinishParams {
        workspace_id: workspace.id.clone(),
        lease_id,
    }
}

#[tokio::test]
async fn normal_last_session_removes_clean_resources_with_explicit_thread_policy() {
    for (bound, keep_thread) in [(false, false), (true, false), (true, true)] {
        let fixture = Fixture::new(FakeWorker::default());
        let workspace = temporary(&fixture, bound, keep_thread).await;
        let finish = attach(&fixture, &workspace).await;
        let result = fixture
            .coordinator
            .finish_workspace_attach(finish.clone())
            .await
            .unwrap();
        assert_eq!(result.cleanup, Some(WorkspaceCleanupResult::Deleted));
        assert!(!workspace.worktree_path.as_ref().unwrap().exists());
        assert!(
            fixture
                .store
                .workspace_by_id(&workspace.id)
                .unwrap()
                .is_none()
        );
        if let Some(thread) = workspace.codex_thread_id.as_ref() {
            assert_eq!(
                fixture
                    .worker
                    .native_threads
                    .lock()
                    .unwrap()
                    .contains_key(thread),
                keep_thread
            );
        }
        let repo = fixture.coordinator.git.discover(&fixture.source).unwrap();
        assert!(
            fixture
                .coordinator
                .git
                .resolve_local_branch(&repo, workspace.branch_name.as_deref().unwrap())
                .is_err()
        );
        assert!(
            fixture
                .coordinator
                .finish_workspace_attach(finish)
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn multiple_tuis_cleanup_only_after_the_last_regular_exit() {
    let fixture = Fixture::new(FakeWorker::default());
    let workspace = temporary(&fixture, true, false).await;
    let first = attach(&fixture, &workspace).await;
    let last = attach(&fixture, &workspace).await;
    assert_eq!(
        fixture
            .coordinator
            .finish_workspace_attach(first)
            .await
            .unwrap()
            .cleanup,
        None
    );
    assert!(
        fixture
            .store
            .workspace_by_id(&workspace.id)
            .unwrap()
            .unwrap()
            .temporary
            .is_some()
    );
    assert_eq!(
        fixture.coordinator.workspace_clients(&workspace.id).len(),
        1
    );
    assert_eq!(
        fixture
            .coordinator
            .finish_workspace_attach(last)
            .await
            .unwrap()
            .cleanup,
        Some(WorkspaceCleanupResult::Deleted)
    );
}

#[tokio::test]
async fn ordinary_workspaces_are_not_deleted_even_by_successful_completion() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let finish = attach(&fixture, &workspace).await;
    assert_eq!(
        fixture
            .coordinator
            .finish_workspace_attach(finish)
            .await
            .unwrap()
            .cleanup,
        None
    );
    assert!(workspace.worktree_path.unwrap().is_dir());
    assert!(
        fixture
            .store
            .workspace_by_id(&workspace.id)
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn ordinary_release_lost_lease_and_restart_do_not_trigger_cleanup() {
    for mode in ["release", "expiry", "restart"] {
        let fixture = Fixture::new_persistent(FakeWorker::default());
        let workspace = temporary(&fixture, true, false).await;
        if mode == "expiry" {
            fixture
                .coordinator
                .set_jump_lease_ttl(std::time::Duration::ZERO);
        }
        let finish = attach(&fixture, &workspace).await;
        match mode {
            "release" => {
                fixture
                    .coordinator
                    .release_workspace_attach(WorkspaceAttachReleaseParams {
                        workspace_id: finish.workspace_id.clone(),
                        lease_id: finish.lease_id.clone(),
                    })
                    .unwrap();
            }
            "expiry" => {}
            _ => fixture.coordinator.clear_jump_leases(),
        }
        assert!(
            fixture
                .coordinator
                .finish_workspace_attach(finish)
                .await
                .is_err()
        );
        let recovered = fixture.recovery_coordinator(fixture.worker.clone(), "next-generation");
        assert!(
            fixture
                .store
                .workspace_by_id(&workspace.id)
                .unwrap()
                .unwrap()
                .temporary
                .is_some()
        );
        assert!(workspace.worktree_path.unwrap().exists());
        assert!(recovered.workspace_clients(&workspace.id).is_empty());
    }
}

#[tokio::test]
async fn send_without_jump_never_consumes_the_cleanup_policy() {
    let fixture = Fixture::new(FakeWorker::default());
    let workspace = temporary(&fixture, true, false).await;
    fixture
        .coordinator
        .start_turn(TurnStartParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id.clone(),
            message: "Inspect the repository".into(),
            operation_id: "headless-turn".into(),
        })
        .await
        .unwrap();
    assert!(
        fixture
            .store
            .workspace_by_id(&workspace.id)
            .unwrap()
            .unwrap()
            .temporary
            .is_some()
    );
    assert!(workspace.worktree_path.unwrap().exists());
}

#[tokio::test]
async fn creation_retry_preserves_the_original_temporary_policy_and_rejects_changes() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let mut request = fixture.create_params();
    request.temporary = Some(TemporaryWorkspacePolicy { keep_thread: false });
    let workspace = fixture
        .coordinator
        .create_workspace(request.clone())
        .await
        .unwrap()
        .workspace;
    assert_eq!(
        fixture
            .coordinator
            .create_workspace(request.clone())
            .await
            .unwrap()
            .workspace
            .id,
        workspace.id
    );
    fixture
        .store
        .claim_temporary_cleanup(&workspace.id)
        .unwrap();
    assert!(
        fixture
            .coordinator
            .create_workspace(request.clone())
            .await
            .unwrap()
            .workspace
            .temporary
            .is_none()
    );
    request.temporary.as_mut().unwrap().keep_thread = true;
    assert!(matches!(
        fixture.coordinator.create_workspace(request).await,
        Err(CoordinatorError::IdempotencyConflict)
    ));
}
