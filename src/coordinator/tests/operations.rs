use super::*;

#[tokio::test]
async fn ambiguous_turn_dispatch_is_recorded_once_and_never_retried() {
    let fixture = Fixture::new(FakeWorker::failing_turn_start());
    fixture.register().await;
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let before = fixture.store.events_after(Some(&workspace.id), 0).unwrap();
    let send = TurnStartParams {
        scope: RepositoryScope::repository(fixture.source.clone()),
        workspace: workspace.id.clone(),
        message: "dispatch exactly once".to_owned(),
        operation_id: "ambiguous-send".to_owned(),
    };

    let error = fixture
        .coordinator
        .start_turn(send.clone())
        .await
        .unwrap_err();
    assert_eq!(error.code(), "OPERATION_UNCERTAIN");
    assert_eq!(error.data(), Some(json!({"operationId": "ambiguous-send"})));
    let operation = fixture
        .store
        .operation_by_client_id("ambiguous-send")
        .unwrap()
        .unwrap();
    assert_eq!(operation.state, OperationState::Uncertain);
    assert!(operation.native_result_id.is_none());
    assert!(
        fixture
            .store
            .turn_by_operation_id("ambiguous-send")
            .unwrap()
            .is_none()
    );
    assert_eq!(
        fixture.store.events_after(Some(&workspace.id), 0).unwrap(),
        before
    );
    let turn_calls = || {
        fixture
            .worker
            .calls()
            .iter()
            .filter(|call| matches!(call, WorkerCall::Turn { .. }))
            .count()
    };
    assert_eq!(turn_calls(), 1);

    assert_eq!(
        fixture
            .coordinator
            .start_turn(send.clone())
            .await
            .unwrap_err()
            .code(),
        "OPERATION_UNCERTAIN"
    );
    assert_eq!(turn_calls(), 1);

    let mut conflicting = send.clone();
    conflicting.message = "different payload".to_owned();
    assert_eq!(
        fixture
            .coordinator
            .start_turn(conflicting)
            .await
            .unwrap_err()
            .code(),
        "IDEMPOTENCY_CONFLICT"
    );
}

#[tokio::test]
async fn an_ambiguous_operation_blocks_new_sends_until_the_generation_is_lost() {
    let fixture = Fixture::new(FakeWorker::failing_turn_start());
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
        message: "dispatch exactly once".to_owned(),
        operation_id: "ambiguous-send".to_owned(),
    };
    assert_eq!(
        fixture
            .coordinator
            .start_turn(send.clone())
            .await
            .unwrap_err()
            .code(),
        "OPERATION_UNCERTAIN"
    );
    let operation = fixture
        .store
        .operation_by_client_id("ambiguous-send")
        .unwrap()
        .unwrap();
    let turn_calls = || {
        fixture
            .worker
            .calls()
            .iter()
            .filter(|call| matches!(call, WorkerCall::Turn { .. }))
            .count()
    };

    let mut different_operation = send.clone();
    different_operation.operation_id = "another-send".to_owned();
    assert_eq!(
        fixture
            .coordinator
            .start_turn(different_operation)
            .await
            .unwrap_err()
            .code(),
        "OPERATION_UNCERTAIN"
    );
    assert_eq!(turn_calls(), 1);

    let status = read_workspace(&fixture, &workspace.id).await;
    assert_eq!(status.phase, WorkspacePhase::Unavailable);
    assert_eq!(
        status.active_turn_id.as_deref(),
        Some(operation.id.as_str())
    );

    fixture
        .worker
        .set_native_status("thread-1", CodexThreadStatus::NotLoaded);
    let unloaded = read_workspace(&fixture, &workspace.id).await;
    assert_eq!(unloaded.phase, WorkspacePhase::Unavailable);
    assert_eq!(
        unloaded.active_turn_id.as_deref(),
        Some(operation.id.as_str())
    );
    fixture
        .worker
        .set_native_status("thread-1", CodexThreadStatus::Idle);

    assert_eq!(fixture.coordinator.record_codex_disconnected().unwrap(), 0);
    let status = read_workspace(&fixture, &workspace.id).await;
    assert_eq!(status.phase, WorkspacePhase::Idle);
    assert!(status.active_turn_id.is_none());
    assert_eq!(
        fixture
            .coordinator
            .start_turn(send)
            .await
            .unwrap_err()
            .code(),
        "OPERATION_UNCERTAIN"
    );
    assert_eq!(turn_calls(), 1);
}

async fn read_workspace(fixture: &Fixture, workspace_id: &str) -> Workspace {
    fixture
        .coordinator
        .get_workspace(WorkspaceGetParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace_id.to_owned(),
        })
        .await
        .unwrap()
        .workspace
}

#[tokio::test]
async fn native_events_cannot_prove_an_ambiguous_dispatch_was_accepted() {
    let fixture = Fixture::new(FakeWorker::failing_turn_start());
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
        message: "do not infer from an event".to_owned(),
        operation_id: "event-unproven-send".to_owned(),
    };
    assert_eq!(
        fixture
            .coordinator
            .start_turn(send.clone())
            .await
            .unwrap_err()
            .code(),
        "OPERATION_UNCERTAIN"
    );

    fixture
        .coordinator
        .record_codex_event(CodexEvent::Notification {
            method: "turn/started".to_owned(),
            params: json!({
                "threadId": "thread-1",
                "turn": {"id": "event-turn-1", "status": "inProgress"},
            }),
        })
        .unwrap();
    let operation = fixture
        .store
        .operation_by_client_id("event-unproven-send")
        .unwrap()
        .unwrap();
    assert_eq!(operation.state, OperationState::Uncertain);
    assert!(operation.native_result_id.is_none());

    assert_eq!(
        fixture
            .coordinator
            .start_turn(send)
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

    fixture
        .coordinator
        .record_codex_event(CodexEvent::Notification {
            method: "turn/completed".to_owned(),
            params: json!({
                "threadId": "thread-1",
                "turn": {"id": "event-turn-1", "status": "completed"},
            }),
        })
        .unwrap();
    let status = fixture
        .coordinator
        .get_workspace(WorkspaceGetParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id,
        })
        .await
        .unwrap();
    assert_eq!(status.workspace.phase, WorkspacePhase::Unavailable);
    assert_eq!(
        status.workspace.active_turn_id.as_deref(),
        Some(operation.id.as_str())
    );
}

#[tokio::test]
async fn a_completion_notification_may_overtake_the_confirming_response() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let fixture = Fixture::new(FakeWorker::paused_turn_start(
        Arc::clone(&entered),
        Arc::clone(&release),
    ));
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
        message: "finish before the response".to_owned(),
        operation_id: "fast-send".to_owned(),
    };

    let start = fixture.coordinator.start_turn(send);
    let complete_before_response = async {
        entered.notified().await;
        fixture
            .coordinator
            .record_codex_event(CodexEvent::Notification {
                method: "turn/completed".to_owned(),
                params: json!({
                    "threadId": "thread-1",
                    "turn": {"id": "turn-1", "status": "completed"},
                }),
            })
            .unwrap();
        release.notify_one();
    };
    let (result, ()) = tokio::join!(start, complete_before_response);
    let result = result.unwrap();
    assert_eq!(result.codex_turn_id.as_deref(), Some("turn-1"));

    fixture
        .worker
        .set_native_status("thread-1", CodexThreadStatus::Idle);
    let status = fixture
        .coordinator
        .get_workspace(WorkspaceGetParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id,
        })
        .await
        .unwrap();
    assert_eq!(status.workspace.phase, WorkspacePhase::Idle);
    assert!(status.workspace.active_turn_id.is_none());
}
