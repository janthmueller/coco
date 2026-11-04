use super::*;

#[tokio::test]
async fn dispatch_lost_with_a_generation_stays_uncertain_despite_a_native_completion() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let (fixture, workspace) = setup(FakeWorker::paused_turn_start(entered.clone(), release)).await;
    {
        let start = fixture.coordinator.start_turn(TurnStartParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id.clone(),
            operation_id: "lost-native-ack".into(),
            message: "Maybe executed".into(),
        });
        tokio::pin!(start);
        tokio::select! { _ = entered.notified() => {}, result = &mut start => panic!("start did not pause: {result:?}") }
        fixture
            .coordinator
            .record_codex_event(CodexEvent::Notification {
                method: "turn/completed".into(),
                params: json!({"threadId": workspace.codex_thread_id,
                "turn": {"id": "turn-1", "status": "completed"}}),
            })
            .unwrap();
        // Drop the pending response, then use the same reconciliation that
        // connection loss/startup applies; never infer the key from proximity.
    }
    fixture.coordinator.record_codex_disconnected().unwrap();
    let recovered = fixture.recovery_coordinator(fixture.worker.clone(), "after-lost-ack");
    let observed = recovered
        .get_turn(get_params(&fixture, &workspace, "lost-native-ack"))
        .await
        .unwrap();
    assert_eq!(observed.dispatch, TurnDispatchState::Uncertain);
    assert_eq!(
        observed.unavailable_reason,
        Some(TurnUnavailableReason::AcceptanceUnconfirmed)
    );
    assert!(observed.codex_thread_id.is_some());
    assert!(observed.codex_turn_id.is_none());
    assert!(observed.outcome.is_none());
    let interrupted = recovered
        .interrupt_turn(interrupt_params(&fixture, &workspace, "lost-native-ack"))
        .await
        .unwrap();
    assert_eq!(
        interrupted.interruption,
        TurnInterruptionState::NotRequested
    );
    assert!(
        fixture
            .worker
            .turn_control
            .interrupts
            .lock()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn receipt_survives_generation_change_and_response_cache_loss_without_native_reads() {
    let (fixture, workspace) = setup(FakeWorker::default()).await;
    let sent = send(&fixture, &workspace, "durable").await;
    complete(&fixture, &sent, TurnTerminalStatus::Completed);
    *fixture.coordinator.completed_turn_results.lock().unwrap() = Default::default();
    *fixture.worker.turn_control.lookup_error.lock().unwrap() = true;
    let recovered = fixture.recovery_coordinator(fixture.worker.clone(), "next-generation");
    let result = recovered
        .get_turn(get_params(&fixture, &workspace, "durable"))
        .await
        .unwrap();
    assert_eq!(result.state, TurnObservationState::Finished);
    assert_eq!(result.outcome, Some(TurnTerminalStatus::Completed));
    assert!(result.outcome_observed_at_ms.is_some());
    assert!(
        fixture
            .worker
            .turn_control
            .lookups
            .lock()
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        recovered
            .turn_result(TurnResultParams {
                operation_id: "durable".to_owned()
            })
            .unwrap(),
        TurnResult::Unavailable { .. }
    ));
}

#[tokio::test]
async fn native_metadata_can_repair_missing_completion_but_no_evidence_stays_unavailable() {
    let (fixture, workspace) = setup(FakeWorker::default()).await;
    let sent = send(&fixture, &workspace, "native-repair").await;
    let recovered = fixture.recovery_coordinator(fixture.worker.clone(), "next-generation");
    let params = get_params(&fixture, &workspace, "native-repair");
    let missing = recovered.get_turn(params.clone()).await.unwrap();
    assert_eq!(missing.state, TurnObservationState::Unavailable);
    assert_eq!(
        missing.unavailable_reason,
        Some(TurnUnavailableReason::CompletionNotObserved)
    );
    *fixture.worker.turn_control.in_progress.lock().unwrap() = Some((
        sent.workspace.codex_thread_id.clone().unwrap(),
        sent.codex_turn_id.clone().unwrap(),
    ));
    let running = recovered.get_turn(params.clone()).await.unwrap();
    assert_eq!(running.state, TurnObservationState::Running);
    assert!(running.unavailable_reason.is_none());
    *fixture.worker.turn_control.in_progress.lock().unwrap() = None;
    *fixture.worker.turn_control.lookup_error.lock().unwrap() = true;
    assert_eq!(
        recovered
            .get_turn(params.clone())
            .await
            .unwrap()
            .unavailable_reason,
        Some(TurnUnavailableReason::NativeLookupFailed)
    );
    *fixture.worker.turn_control.lookup_error.lock().unwrap() = false;
    fixture.worker.turn_control.outcomes.lock().unwrap().insert(
        (
            sent.workspace.codex_thread_id.unwrap(),
            sent.codex_turn_id.unwrap(),
        ),
        TurnTerminalStatus::Interrupted,
    );
    let repaired = recovered.get_turn(params).await.unwrap();
    assert_eq!(repaired.outcome, Some(TurnTerminalStatus::Interrupted));
    assert_eq!(
        fixture
            .store
            .operation_by_client_id("native-repair")
            .unwrap()
            .unwrap()
            .terminal_status,
        Some(TurnTerminalStatus::Interrupted)
    );
}

#[tokio::test]
async fn uncertain_start_is_not_not_found_and_is_never_interrupted_or_redispatched() {
    let (fixture, workspace) = setup(FakeWorker::failing_turn_start()).await;
    let params = TurnStartParams {
        scope: RepositoryScope::repository(fixture.source.clone()),
        workspace: workspace.id.clone(),
        message: "May have started".to_owned(),
        operation_id: "uncertain".to_owned(),
    };
    assert_eq!(
        fixture
            .coordinator
            .start_turn(params.clone())
            .await
            .unwrap_err()
            .code(),
        "OPERATION_UNCERTAIN"
    );
    let found = fixture
        .coordinator
        .get_turn(get_params(&fixture, &workspace, "uncertain"))
        .await
        .unwrap();
    assert_eq!(found.dispatch, TurnDispatchState::Uncertain);
    assert_eq!(
        found.unavailable_reason,
        Some(TurnUnavailableReason::AcceptanceUnconfirmed)
    );
    let cancel = fixture
        .coordinator
        .interrupt_turn(interrupt_params(&fixture, &workspace, "uncertain"))
        .await
        .unwrap();
    assert_eq!(cancel.interruption, TurnInterruptionState::NotRequested);
    assert!(
        fixture
            .worker
            .turn_control
            .interrupts
            .lock()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        fixture
            .coordinator
            .start_turn(params)
            .await
            .unwrap_err()
            .code(),
        "OPERATION_UNCERTAIN"
    );
}

#[tokio::test]
async fn prepared_intent_is_not_started_and_not_interrupted() {
    let (fixture, workspace) = setup(FakeWorker::default()).await;
    let (op, _) = fixture
        .store
        .prepare_operation(crate::store::NewOperation {
            operation_id: "prepared".to_owned(),
            workspace_id: workspace.id.clone(),
            kind: crate::store::OperationKind::TurnStart,
            request_fingerprint: "fingerprint".to_owned(),
        })
        .unwrap();
    let get = get_params(&fixture, &workspace, "prepared");
    assert_eq!(
        fixture
            .coordinator
            .get_turn(get.clone())
            .await
            .unwrap()
            .state,
        TurnObservationState::NotStarted
    );
    assert_eq!(
        fixture
            .coordinator
            .interrupt_turn(interrupt_params(&fixture, &workspace, "prepared"))
            .await
            .unwrap()
            .interruption,
        TurnInterruptionState::NotRequested
    );
    assert_eq!(
        fixture
            .store
            .operation_by_client_id("prepared")
            .unwrap()
            .unwrap()
            .id,
        op.id
    );
    assert!(
        fixture
            .worker
            .turn_control
            .lookups
            .lock()
            .unwrap()
            .is_empty()
    );
}
