use super::*;

#[tokio::test]
async fn a_proven_rejection_keeps_fingerprints_but_allows_same_or_new_send() {
    for reuse_id in [false, true] {
        let fixture = Fixture::new(FakeWorker::default());
        fixture.register().await;
        let workspace = fixture
            .coordinator
            .create_workspace(fixture.create_params())
            .await
            .unwrap()
            .workspace;
        let send = TurnStartParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id.clone(),
            message: "safe explicit retry".to_owned(),
            operation_id: "rejected-send".to_owned(),
        };
        *fixture.worker.reject_next_turn_start.lock().unwrap() = true;
        let error = fixture
            .coordinator
            .start_turn(send.clone())
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            CoordinatorError::Worker(WorkerError::TurnRejected(_))
        ));
        let retained = fixture
            .store
            .operation_by_client_id(&send.operation_id)
            .unwrap()
            .unwrap();
        assert_eq!(retained.state, OperationState::Prepared);
        assert!(retained.dispatch_started_at_ms.is_none());
        assert!(retained.native_result_id.is_none());
        let status = read_workspace(&fixture, &workspace.id).await;
        assert_ne!(status.phase, WorkspacePhase::Unavailable);
        assert!(status.active_turn_id.is_none());

        let mut conflicting = send.clone();
        conflicting.message = "changed payload".to_owned();
        assert_eq!(
            fixture
                .coordinator
                .start_turn(conflicting)
                .await
                .unwrap_err()
                .code(),
            "IDEMPOTENCY_CONFLICT"
        );
        let mut retry = send;
        if !reuse_id {
            retry.operation_id = "new-send".to_owned();
        }
        fixture.coordinator.start_turn(retry.clone()).await.unwrap();
        let accepted = fixture
            .store
            .operation_by_client_id(&retry.operation_id)
            .unwrap()
            .unwrap();
        assert_eq!(accepted.state, OperationState::Accepted);
        assert!(accepted.native_result_id.is_some());
        assert_eq!(
            fixture
                .worker
                .calls()
                .iter()
                .filter(|call| matches!(call, WorkerCall::Turn { .. }))
                .count(),
            2
        );
    }
}

#[tokio::test]
async fn rejection_preserves_an_already_materialized_fresh_binding_without_a_turn_guard() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    fixture
        .worker
        .materialized_threads
        .lock()
        .unwrap()
        .insert("thread-1".to_owned());
    *fixture.worker.reject_next_turn_start.lock().unwrap() = true;
    let send = TurnStartParams {
        scope: RepositoryScope::repository(fixture.source.clone()),
        workspace: workspace.id.clone(),
        message: "not accepted".to_owned(),
        operation_id: "rejected-materialized".to_owned(),
    };
    assert_eq!(
        fixture
            .coordinator
            .start_turn(send.clone())
            .await
            .unwrap_err()
            .code(),
        "CODEX_ERROR"
    );
    let status = read_workspace(&fixture, &workspace.id).await;
    assert_eq!(status.codex_thread_id.as_deref(), Some("thread-1"));
    assert!(status.active_turn_id.is_none());
    fixture.coordinator.start_turn(send).await.unwrap();
    assert_eq!(
        fixture
            .worker
            .calls()
            .iter()
            .filter(|call| matches!(call, WorkerCall::Thread { .. }))
            .count(),
        1
    );
}

#[tokio::test]
async fn a_rejection_cannot_reopen_an_operation_already_reconciled_as_uncertain() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let fixture = Fixture::new(FakeWorker::paused_turn_start(
        entered.clone(),
        release.clone(),
    ));
    fixture.register().await;
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    *fixture.worker.reject_next_turn_start.lock().unwrap() = true;
    let send = TurnStartParams {
        scope: RepositoryScope::repository(fixture.source.clone()),
        workspace: workspace.id.clone(),
        message: "fail closed if the generation changes".to_owned(),
        operation_id: "reconciled-rejection".to_owned(),
    };
    let reconcile = async {
        entered.notified().await;
        assert_eq!(
            fixture
                .store
                .mark_unconfirmed_operations_uncertain()
                .unwrap(),
            1
        );
        release.notify_one();
    };
    let (result, ()) = tokio::join!(fixture.coordinator.start_turn(send.clone()), reconcile);
    assert_eq!(result.unwrap_err().code(), "OPERATION_UNCERTAIN");
    let mut next = send;
    next.operation_id = "not-safe-to-retry".to_owned();
    assert_eq!(
        fixture
            .coordinator
            .start_turn(next)
            .await
            .unwrap_err()
            .code(),
        "OPERATION_UNCERTAIN"
    );
    assert_eq!(
        fixture
            .worker
            .calls()
            .iter()
            .filter(|call| matches!(call, WorkerCall::Turn { .. }))
            .count(),
        1
    );
    assert_eq!(
        read_workspace(&fixture, &workspace.id).await.phase,
        WorkspacePhase::Unavailable
    );
}
