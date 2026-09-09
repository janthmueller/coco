use super::*;
use crate::protocol::{
    WorkspaceAttachAdoptParams, WorkspaceAttachAdoptResult, WorkspaceAttachReleaseParams,
    WorkspaceAttachRenewParams,
};

#[tokio::test]
async fn adopts_only_the_exact_materialized_thread_from_a_fresh_jump() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let prepared = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let attached = fixture
        .coordinator
        .attach_workspace(WorkspaceAttachParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: prepared.id.clone(),
        })
        .await
        .unwrap();
    let WorkspaceAttachLaunch::Start { lease_id } = attached.launch else {
        panic!("fresh jump did not return a start lease");
    };
    assert!(matches!(
        fixture
            .coordinator
            .attach_workspace(WorkspaceAttachParams {
                scope: RepositoryScope::repository(fixture.source.clone()),
                workspace: prepared.id.clone(),
            })
            .await,
        Err(CoordinatorError::WorkspaceAttachInProgress)
    ));

    let worktree = prepared.worktree_path.clone().unwrap();
    fixture.worker.remember_native_thread(NativeThread {
        id: "tui-thread".to_owned(),
        cwd: worktree.clone(),
        name: None,
        status: CodexThreadStatus::Idle,
        forked_from_id: None,
    });
    let params = WorkspaceAttachAdoptParams {
        workspace_id: prepared.id.clone(),
        lease_id: lease_id.clone(),
        thread_id: "tui-thread".to_owned(),
    };
    assert_eq!(
        fixture
            .coordinator
            .adopt_workspace_thread(params.clone())
            .await
            .unwrap(),
        WorkspaceAttachAdoptResult::Pending
    );
    assert!(
        fixture
            .store
            .workspace_by_id(&prepared.id)
            .unwrap()
            .unwrap()
            .codex_thread_id
            .is_none()
    );

    fixture.worker.remember_materialized_thread(NativeThread {
        id: "tui-thread".to_owned(),
        cwd: worktree,
        name: None,
        status: CodexThreadStatus::Idle,
        forked_from_id: None,
    });
    let WorkspaceAttachAdoptResult::Bound { workspace } = fixture
        .coordinator
        .adopt_workspace_thread(params)
        .await
        .unwrap()
    else {
        panic!("materialized TUI thread was not adopted");
    };
    assert_eq!(workspace.codex_thread_id.as_deref(), Some("tui-thread"));
    assert_eq!(workspace.phase, WorkspacePhase::Idle);
    assert_adoption_subscribed_to_exact_thread(&fixture.worker);

    fixture
        .coordinator
        .start_turn(TurnStartParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: prepared.id.clone(),
            message: "continue through the CLI while the TUI stays open".to_owned(),
            operation_id: "send-after-adoption".to_owned(),
        })
        .await
        .unwrap();

    fixture
        .coordinator
        .release_workspace_attach(WorkspaceAttachReleaseParams {
            workspace_id: prepared.id,
            lease_id,
        })
        .unwrap();
}

fn assert_adoption_subscribed_to_exact_thread(worker: &FakeWorker) {
    assert!(matches!(
        worker.calls().as_slice(),
        [
            WorkerCall::FindMaterialized { thread_id: first, .. },
            WorkerCall::FindMaterialized { thread_id: second, .. },
            WorkerCall::Name { thread_id: named, name },
            WorkerCall::Read { thread_id: read },
            WorkerCall::Resume { thread_id: resumed, .. },
        ] if first == "tui-thread"
            && second == "tui-thread"
            && named == "tui-thread"
            && name == "first-workspace"
            && read == "tui-thread"
            && resumed == "tui-thread"
    ));
}

#[tokio::test]
async fn releasing_an_empty_jump_keeps_the_workspace_prepared() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let prepared = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let first = fixture
        .coordinator
        .attach_workspace(WorkspaceAttachParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: prepared.id.clone(),
        })
        .await
        .unwrap();
    let WorkspaceAttachLaunch::Start { lease_id } = first.launch else {
        panic!("fresh jump did not return a start lease");
    };
    fixture
        .coordinator
        .renew_workspace_attach(WorkspaceAttachRenewParams {
            workspace_id: prepared.id.clone(),
            lease_id: lease_id.clone(),
        })
        .unwrap();
    assert!(matches!(
        fixture
            .coordinator
            .renew_workspace_attach(WorkspaceAttachRenewParams {
                workspace_id: prepared.id.clone(),
                lease_id: "another-lease".to_owned(),
            }),
        Err(CoordinatorError::InvalidWorkspaceAttachLease)
    ));
    fixture
        .coordinator
        .release_workspace_attach(WorkspaceAttachReleaseParams {
            workspace_id: prepared.id.clone(),
            lease_id,
        })
        .unwrap();

    let stored = fixture
        .store
        .workspace_by_id(&prepared.id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.phase, WorkspacePhase::Prepared);
    assert!(stored.codex_thread_id.is_none());
    assert!(fixture.worker.calls().is_empty());

    let second = fixture
        .coordinator
        .attach_workspace(WorkspaceAttachParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: prepared.id.clone(),
        })
        .await
        .unwrap();
    let WorkspaceAttachLaunch::Start { lease_id } = second.launch else {
        panic!("released workspace did not acquire a new start lease");
    };
    fixture
        .coordinator
        .release_workspace_attach(WorkspaceAttachReleaseParams {
            workspace_id: prepared.id,
            lease_id,
        })
        .unwrap();
}

#[tokio::test]
async fn app_server_disconnect_expires_a_fresh_jump_lease() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let prepared = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let first = fixture
        .coordinator
        .attach_workspace(WorkspaceAttachParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: prepared.id.clone(),
        })
        .await
        .unwrap();
    let WorkspaceAttachLaunch::Start {
        lease_id: stale_lease,
    } = first.launch
    else {
        panic!("fresh jump did not return a start lease");
    };

    assert_eq!(fixture.coordinator.record_codex_disconnected().unwrap(), 0);
    let second = fixture
        .coordinator
        .attach_workspace(WorkspaceAttachParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: prepared.id.clone(),
        })
        .await
        .unwrap();
    let WorkspaceAttachLaunch::Start { lease_id } = second.launch else {
        panic!("workspace did not acquire a new lease after disconnect");
    };
    assert_ne!(lease_id, stale_lease);
    assert!(matches!(
        fixture
            .coordinator
            .release_workspace_attach(WorkspaceAttachReleaseParams {
                workspace_id: prepared.id.clone(),
                lease_id: stale_lease,
            }),
        Err(CoordinatorError::InvalidWorkspaceAttachLease)
    ));
    fixture
        .coordinator
        .release_workspace_attach(WorkspaceAttachReleaseParams {
            workspace_id: prepared.id,
            lease_id,
        })
        .unwrap();
}

#[tokio::test]
async fn a_pending_fresh_jump_blocks_a_competing_send_and_candidate_swap() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let prepared = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let attached = fixture
        .coordinator
        .attach_workspace(WorkspaceAttachParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: prepared.id.clone(),
        })
        .await
        .unwrap();
    let WorkspaceAttachLaunch::Start { lease_id } = attached.launch else {
        panic!("fresh jump did not return a start lease");
    };

    assert!(matches!(
        fixture
            .coordinator
            .start_turn(TurnStartParams {
                scope: RepositoryScope::repository(fixture.source.clone()),
                workspace: prepared.id.clone(),
                message: "race the TUI".to_owned(),
                operation_id: "competing-send".to_owned(),
            })
            .await,
        Err(CoordinatorError::WorkspaceAttachInProgress)
    ));
    assert_eq!(
        fixture
            .coordinator
            .adopt_workspace_thread(WorkspaceAttachAdoptParams {
                workspace_id: prepared.id.clone(),
                lease_id: lease_id.clone(),
                thread_id: "candidate-a".to_owned(),
            })
            .await
            .unwrap(),
        WorkspaceAttachAdoptResult::Pending
    );
    assert!(matches!(
        fixture
            .coordinator
            .adopt_workspace_thread(WorkspaceAttachAdoptParams {
                workspace_id: prepared.id.clone(),
                lease_id: lease_id.clone(),
                thread_id: "candidate-b".to_owned(),
            })
            .await,
        Err(CoordinatorError::InvalidWorkspaceAttachLease)
    ));
    fixture
        .coordinator
        .release_workspace_attach(WorkspaceAttachReleaseParams {
            workspace_id: prepared.id,
            lease_id,
        })
        .unwrap();
}
