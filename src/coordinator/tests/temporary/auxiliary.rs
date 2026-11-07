use super::*;
use crate::protocol::WorkspaceAttachDisarmCleanupParams;

fn disarm(finish: &WorkspaceAttachFinishParams) -> WorkspaceAttachDisarmCleanupParams {
    WorkspaceAttachDisarmCleanupParams {
        workspace_id: finish.workspace_id.clone(),
        lease_id: finish.lease_id.clone(),
    }
}

#[tokio::test]
async fn auxiliary_conversation_disarms_cleanup_for_all_clients_and_after_database_restart() {
    let fixture = Fixture::new_persistent(FakeWorker::default());
    let workspace = temporary(&fixture, true, false).await;
    let first = attach(&fixture, &workspace).await;
    let last = attach(&fixture, &workspace).await;
    let original = workspace.codex_thread_id.as_ref().unwrap();
    fixture
        .coordinator
        .disarm_workspace_cleanup(disarm(&first))
        .unwrap();
    // Repeating a confirmed disarm is safe and does not release the lease.
    fixture
        .coordinator
        .disarm_workspace_cleanup(disarm(&first))
        .unwrap();
    let mut auxiliary = fixture.worker.native_threads.lock().unwrap()[original].clone();
    auxiliary.id = "independent-tui-thread".into();
    auxiliary.forked_from_id = None;
    auxiliary.status = CodexThreadStatus::Active {
        active_flags: vec![],
    };
    fixture
        .worker
        .native_threads
        .lock()
        .unwrap()
        .insert(auxiliary.id.clone(), auxiliary);
    for finish in [first, last] {
        assert_eq!(
            fixture
                .coordinator
                .finish_workspace_attach(finish)
                .await
                .unwrap()
                .cleanup,
            None
        );
    }
    assert!(workspace.worktree_path.as_ref().unwrap().is_dir());
    assert_eq!(
        fixture
            .store
            .workspace_by_id(&workspace.id)
            .unwrap()
            .unwrap()
            .codex_thread_id
            .as_ref(),
        Some(original)
    );
    assert!(
        !fixture
            .worker
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|call| matches!(
                call,
                WorkerCall::StopExecution { .. } | WorkerCall::DeleteThread { .. }
            ))
    );

    assert_retained_after_restart(fixture, workspace).await;
}

async fn assert_retained_after_restart(fixture: Fixture, workspace: Workspace) {
    // Close the old SQLite connection and actually open a new database handle
    // and coordinator generation, not just a reconstructed in-memory fixture.
    drop(fixture.coordinator);
    drop(fixture.store);
    let reopened = Arc::new(Store::open(fixture._temp.path().join("coordinator.db")).unwrap());
    assert!(
        reopened
            .workspace_by_id(&workspace.id)
            .unwrap()
            .unwrap()
            .temporary
            .is_none()
    );
    let coordinator = Coordinator::new(
        reopened,
        Git::default(),
        fixture.worker.clone(),
        fixture.worktrees.clone(),
        fixture.codex_home.clone(),
        Arc::new(HookRegistry::empty()),
        "new-generation".into(),
    );
    let launch = coordinator
        .attach_workspace(WorkspaceAttachParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id.clone(),
            client: None,
        })
        .await
        .unwrap()
        .launch;
    let lease_id = match launch {
        WorkspaceAttachLaunch::Start { lease_id }
        | WorkspaceAttachLaunch::Resume { lease_id, .. } => lease_id,
    };
    assert_eq!(
        coordinator
            .finish_workspace_attach(WorkspaceAttachFinishParams {
                workspace_id: workspace.id.clone(),
                lease_id,
            })
            .await
            .unwrap()
            .cleanup,
        None
    );
    assert!(workspace.worktree_path.as_ref().unwrap().is_dir());
}

#[tokio::test]
async fn disarm_requires_live_exact_authority_and_does_not_touch_another_workspace() {
    let fixture = Fixture::new(FakeWorker::default());
    let workspace = temporary(&fixture, false, false).await;
    let finish = attach(&fixture, &workspace).await;
    for (workspace_id, lease_id) in [
        ("another-workspace".to_owned(), finish.lease_id.clone()),
        (workspace.id.clone(), "unknown-lease".to_owned()),
    ] {
        assert!(
            fixture
                .coordinator
                .disarm_workspace_cleanup(WorkspaceAttachDisarmCleanupParams {
                    workspace_id,
                    lease_id
                })
                .is_err()
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
    }
    fixture
        .coordinator
        .release_workspace_attach(WorkspaceAttachReleaseParams {
            workspace_id: workspace.id.clone(),
            lease_id: finish.lease_id.clone(),
        })
        .unwrap();
    assert!(
        fixture
            .coordinator
            .disarm_workspace_cleanup(disarm(&finish))
            .is_err()
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
}

#[tokio::test]
async fn failed_durable_disarm_returns_an_error_without_claiming_success() {
    let fixture = Fixture::new_persistent(FakeWorker::default());
    let workspace = temporary(&fixture, false, false).await;
    let finish = attach(&fixture, &workspace).await;
    let database = rusqlite::Connection::open(fixture._temp.path().join("coordinator.db")).unwrap();
    database
        .execute_batch(
            "CREATE TRIGGER fail_disarm BEFORE UPDATE OF temporary_cleanup_json ON workspaces
         BEGIN SELECT RAISE(ABORT, 'injected failed durable disarm'); END;",
        )
        .unwrap();
    assert!(
        fixture
            .coordinator
            .disarm_workspace_cleanup(disarm(&finish))
            .is_err()
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
    assert!(workspace.worktree_path.as_ref().unwrap().exists());
}
