use super::*;

mod uncertain;

fn reopen(fixture: Fixture) -> Fixture {
    let Fixture {
        _temp,
        source,
        worktrees,
        codex_home,
        store,
        worker,
        coordinator,
    } = fixture;
    drop(coordinator);
    drop(store);
    let store = Arc::new(Store::open(_temp.path().join("coordinator.db")).unwrap());
    let coordinator = Coordinator::new(
        store.clone(),
        Git::default(),
        worker.clone(),
        worktrees.clone(),
        codex_home.clone(),
        Arc::new(HookRegistry::empty()),
        "new-generation".into(),
    );
    Fixture {
        _temp,
        source,
        worktrees,
        codex_home,
        store,
        worker,
        coordinator,
    }
}

#[tokio::test]
async fn lost_receipt_cannot_adopt_a_workspace_or_remove_its_required_worker_binding() {
    let fixture = Fixture::new_persistent(FakeWorker::default());
    let mut request = params(&fixture);
    request.request.worker_mcp = Some(worker_file(&fixture, "run-a"));
    let created = fixture
        .coordinator
        .setup_workspace(request.clone())
        .await
        .unwrap();
    let original = fixture
        .coordinator
        .materialize_workspace_thread(workspace(&fixture, &created))
        .await
        .unwrap();
    let thread_id = original.codex_thread_id.clone().unwrap();
    let native_starts = fixture
        .worker
        .calls()
        .iter()
        .filter(|call| matches!(call, WorkerCall::Thread { .. }))
        .count();
    let connection =
        rusqlite::Connection::open(fixture._temp.path().join("coordinator.db")).unwrap();
    connection
        .execute("DELETE FROM workspace_creations", [])
        .unwrap();
    let observed = fixture
        .coordinator
        .get_workspace_creation(get(&fixture))
        .unwrap();
    assert_eq!(observed.state, WorkspaceCreationState::Unavailable);
    assert_eq!(
        observed.error_code.as_deref(),
        Some("CREATION_RECEIPT_MISSING")
    );
    let repeated = fixture
        .coordinator
        .setup_workspace(request)
        .await
        .unwrap_err();
    assert_eq!(repeated.code(), "CREATION_RECEIPT_MISSING");
    assert_eq!(
        fixture
            .coordinator
            .ensure_workspace_thread_loaded(original.clone())
            .await
            .unwrap_err()
            .code(),
        "WORKER_MCP_BINDING_UNAVAILABLE"
    );
    assert!(original.worktree_path.as_ref().unwrap().exists());
    assert_eq!(
        fixture
            .worker
            .calls()
            .iter()
            .filter(|call| matches!(call, WorkerCall::Thread { .. }))
            .count(),
        native_starts
    );
    assert!(
        fixture
            .store
            .thread_has_worker_mcp_binding(&thread_id)
            .unwrap()
    );
    let mut cleanup = super::super::retirement_deletion::delete_params(&fixture, &original);
    fixture.coordinator.clear_jump_leases();
    cleanup.delete_thread = false;
    fixture.coordinator.delete_workspace(cleanup).await.unwrap();
    assert!(
        fixture
            .store
            .thread_has_worker_mcp_binding(&thread_id)
            .unwrap(),
        "retained conversation must not become a fork source after evidence loss"
    );
}

#[tokio::test]
async fn a_missing_target_record_is_unavailable_not_a_new_create_permission() {
    let fixture = Fixture::new_persistent(FakeWorker::default());
    let request = params(&fixture);
    let created = fixture
        .coordinator
        .setup_workspace(request.clone())
        .await
        .unwrap();
    let connection =
        rusqlite::Connection::open(fixture._temp.path().join("coordinator.db")).unwrap();
    connection
        .execute_batch("PRAGMA foreign_keys=OFF;")
        .unwrap();
    // Simulates incomplete restoration/corrupt local evidence, not CoCo delete.
    connection
        .execute(
            "DELETE FROM workspaces WHERE id = ?1",
            [created.workspace_id.as_deref().unwrap()],
        )
        .unwrap();
    let observed = fixture
        .coordinator
        .get_workspace_creation(get(&fixture))
        .unwrap();
    assert_eq!(observed.state, WorkspaceCreationState::Unavailable);
    assert_eq!(
        observed.error_code.as_deref(),
        Some("CREATION_TARGET_UNAVAILABLE")
    );
    assert_eq!(
        fixture.coordinator.setup_workspace(request).await.unwrap(),
        observed
    );
    assert!(
        created.worktree_path.unwrap().exists(),
        "no destructive compensation"
    );
}

#[tokio::test]
async fn configured_and_deleted_receipts_survive_actual_database_reopen_and_retire_keys() {
    let fixture = Fixture::new_persistent(FakeWorker::default());
    let request = params(&fixture);
    let created = fixture
        .coordinator
        .setup_workspace(request.clone())
        .await
        .unwrap();
    let fixture = reopen(fixture);
    assert_eq!(
        fixture
            .coordinator
            .setup_workspace(request.clone())
            .await
            .unwrap(),
        created
    );
    let owned = workspace(&fixture, &created);
    fixture
        .coordinator
        .delete_workspace(super::super::retirement_deletion::delete_params(
            &fixture, &owned,
        ))
        .await
        .unwrap();
    let fixture = reopen(fixture);
    let deleted = fixture
        .coordinator
        .get_workspace_creation(get(&fixture))
        .unwrap();
    assert_eq!(deleted.state, WorkspaceCreationState::Deleted);
    assert_eq!(deleted.workspace_id, created.workspace_id);
    assert_eq!(
        fixture.coordinator.setup_workspace(request).await.unwrap(),
        deleted
    );
    assert!(!owned.worktree_path.unwrap().exists());
}

#[tokio::test]
async fn cancelled_client_leaves_preparing_evidence_and_never_reissues_creation() {
    let fixture = Fixture::new_persistent(FakeWorker::default());
    let discovered = fixture.coordinator.git.discover(&fixture.source).unwrap();
    let gate = fixture.coordinator.repository_lock(&discovered.id).await;
    let held = gate.lock().await;
    let request = params(&fixture);
    let mut pending = Box::pin(fixture.coordinator.setup_workspace(request.clone()));
    tokio::select! {
        result = &mut pending => panic!("creation unexpectedly passed gate: {result:?}"),
        () = tokio::task::yield_now() => {}
    }
    let receipt = fixture
        .coordinator
        .get_workspace_creation(get(&fixture))
        .unwrap();
    assert_eq!(receipt.state, WorkspaceCreationState::Preparing);
    assert!(receipt.workspace_id.is_none());
    drop(pending);
    drop(held);
    let fixture = reopen(fixture);
    assert_eq!(
        fixture.coordinator.setup_workspace(request).await.unwrap(),
        receipt
    );
    assert!(!fixture.worktrees.exists());
    // No receipt after local evidence loss is UNKNOWN, never a retry permit.
    let mut missing = get(&fixture);
    missing.operation_id = "lost-local-receipt".into();
    assert_eq!(
        fixture
            .coordinator
            .get_workspace_creation(missing)
            .unwrap()
            .state,
        WorkspaceCreationState::Unknown
    );
}
