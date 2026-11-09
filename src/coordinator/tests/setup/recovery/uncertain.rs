use super::*;

async fn unconfirmed_send() -> (Fixture, WorkspaceCreationReceipt, Workspace, String) {
    let fixture = Fixture::new_persistent(FakeWorker::failing_turn_start());
    let mut request = params(&fixture);
    request.request.worker_mcp = Some(worker_file(&fixture, "run-a"));
    let receipt = fixture.coordinator.setup_workspace(request).await.unwrap();
    let source = workspace(&fixture, &receipt);
    let error = fixture
        .coordinator
        .start_turn(TurnStartParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: source.id.clone(),
            operation_id: "unconfirmed-worker-send".into(),
            message: "fixture".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(error.code(), "OPERATION_UNCERTAIN");
    let operation = fixture
        .store
        .operation_by_client_id("unconfirmed-worker-send")
        .unwrap()
        .unwrap();
    assert_eq!(operation.state, crate::store::OperationState::Uncertain);
    let thread = operation.native_thread_id.unwrap();
    assert!(
        fixture
            .store
            .workspace_by_id(&source.id)
            .unwrap()
            .unwrap()
            .codex_thread_id
            .is_none()
    );
    assert!(
        fixture
            .coordinator
            .get_workspace_creation(get(&fixture))
            .unwrap()
            .codex_thread_id
            .is_none()
    );
    (fixture, receipt, source, thread)
}

fn finish_late(fixture: &Fixture, thread: &str) {
    let mut native = fixture
        .worker
        .native_threads
        .lock()
        .unwrap()
        .get(thread)
        .unwrap()
        .clone();
    native.status = CodexThreadStatus::Idle;
    fixture.worker.remember_materialized_thread(native);
    fixture
        .worker
        .context_boundaries
        .lock()
        .unwrap()
        .insert(thread.into(), Some("late-completed-turn".into()));
}

async fn assert_excluded(fixture: &Fixture, thread: &str) {
    assert!(fixture.store.thread_has_worker_mcp_binding(thread).unwrap());
    for compact in [false, true] {
        let mut fork = fixture.create_params();
        fork.name = "context-after-uncertain-send".into();
        fork.operation_id = "context-after-uncertain-send".into();
        fork.context = WorkspaceContextRequest::Fork {
            source: WorkspaceContextSource::Thread {
                thread_id: thread.into(),
            },
            compact,
        };
        let error = fixture
            .coordinator
            .create_workspace(fork)
            .await
            .unwrap_err();
        assert_eq!(error.code(), "INVALID_PARAMS");
        assert!(error.to_string().contains("worker-bound context"));
    }
    assert!(
        !fixture
            .worker
            .calls()
            .iter()
            .any(|call| matches!(call, WorkerCall::Fork { .. }))
    );
}

#[tokio::test]
async fn uncertain_first_send_is_excluded_before_and_after_late_materialization_and_real_reopen() {
    let (fixture, _, _, thread) = unconfirmed_send().await;
    assert_excluded(&fixture, &thread).await;
    finish_late(&fixture, &thread);
    let fixture = reopen(fixture);
    assert_excluded(&fixture, &thread).await;
    assert_eq!(fixture.store.list_workspaces(None).unwrap().len(), 1);
}

#[tokio::test]
async fn uncertain_worker_thread_stays_excluded_after_workspace_and_operation_cleanup() {
    let (fixture, receipt, source, thread) = unconfirmed_send().await;
    finish_late(&fixture, &thread);
    // A new coordinator has no in-memory send registry. The saved native
    // association must survive normal retirement of an unbound workspace.
    let fixture = reopen(fixture);
    let mut cleanup = super::super::super::retirement_deletion::delete_params(&fixture, &source);
    cleanup.delete_thread = false;
    fixture.coordinator.delete_workspace(cleanup).await.unwrap();
    assert!(
        fixture
            .store
            .operation_by_client_id("unconfirmed-worker-send")
            .unwrap()
            .is_none()
    );
    let fixture = reopen(fixture);
    let deleted = fixture
        .coordinator
        .get_workspace_creation(get(&fixture))
        .unwrap();
    assert_eq!(deleted.state, WorkspaceCreationState::Deleted);
    assert_eq!(deleted.workspace_id, receipt.workspace_id);
    assert!(
        deleted.codex_thread_id.is_none(),
        "attempt evidence must not invent a confirmed binding"
    );
    assert_excluded(&fixture, &thread).await;
}

#[tokio::test]
async fn known_empty_worker_thread_is_excluded_before_any_turn_dispatch() {
    let fixture = Fixture::new_persistent(FakeWorker::default());
    let mut request = params(&fixture);
    request.request.worker_mcp = Some(worker_file(&fixture, "run-a"));
    let receipt = fixture.coordinator.setup_workspace(request).await.unwrap();
    let source = workspace(&fixture, &receipt);
    let pending = fixture
        .coordinator
        .start_fresh_workspace_thread(&source)
        .await
        .unwrap();
    assert!(
        fixture
            .store
            .workspace_by_id(&source.id)
            .unwrap()
            .unwrap()
            .codex_thread_id
            .is_none()
    );
    assert!(
        fixture
            .store
            .operation_by_client_id("unconfirmed-worker-send")
            .unwrap()
            .is_none()
    );
    let fixture = reopen(fixture);
    assert_excluded(&fixture, &pending.started.id).await;
}
