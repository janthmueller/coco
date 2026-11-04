use super::*;

fn notification(sent: &WorkspaceResult, status: TurnTerminalStatus) -> CodexEvent {
    CodexEvent::Notification {
        method: "turn/completed".into(),
        params: json!({"threadId": sent.workspace.codex_thread_id,
            "turn": {"id": sent.codex_turn_id, "status": status.as_str()}}),
    }
}

#[tokio::test]
async fn delayed_completions_after_real_reopen_preserve_outcomes_and_interrupt_attempts() {
    for outcome in [
        TurnTerminalStatus::Completed,
        TurnTerminalStatus::Failed,
        TurnTerminalStatus::Interrupted,
    ] {
        for lost_reply in [false, true] {
            let mut native = worker();
            native.turn_control.history_unavailable = true;
            let fixture = Fixture::new_persistent(native);
            fixture.register().await;
            let workspace = fixture
                .create_and_materialize(fixture.create_params())
                .await;
            let sent = send(&fixture, &workspace, "delayed-after-reopen").await;
            *fixture.worker.turn_control.interrupt_error.lock().unwrap() = lost_reply;
            let attempt = params(&fixture, &workspace, "delayed-after-reopen");
            let requested = fixture.coordinator.interrupt_turn(attempt.clone()).await;
            assert_eq!(requested.is_err(), lost_reply);
            let saved = fixture
                .store
                .operation_by_client_id("delayed-after-reopen")
                .unwrap()
                .unwrap();
            let lookups = fixture.worker.turn_control.lookups.lock().unwrap().len();
            let fixture = reopen(fixture);
            fixture
                .coordinator
                .record_codex_event(notification(&sent, outcome))
                .unwrap();
            let observed = fixture
                .coordinator
                .interrupt_turn(attempt.clone())
                .await
                .unwrap();
            assert_eq!(observed.turn.state, TurnObservationState::Finished);
            assert_eq!(observed.turn.outcome, Some(outcome));
            assert!(observed.turn.outcome_observed_at_ms.is_some());
            assert_eq!(observed.turn.interrupt_attempt, saved.interrupt_attempt);
            assert_eq!(
                observed.turn.interrupt_requested_at_ms,
                saved.interrupt_requested_at_ms
            );
            assert_eq!(
                observed.interruption,
                if outcome == TurnTerminalStatus::Interrupted {
                    TurnInterruptionState::Confirmed
                } else {
                    TurnInterruptionState::AlreadyFinished
                }
            );
            fixture
                .coordinator
                .record_codex_event(notification(&sent, outcome))
                .unwrap();
            let fixture = reopen(fixture);
            assert_eq!(
                fixture.coordinator.interrupt_turn(attempt).await.unwrap(),
                observed
            );
            assert_eq!(
                fixture.worker.turn_control.interrupts.lock().unwrap().len(),
                1
            );
            assert_eq!(
                fixture.worker.turn_control.lookups.lock().unwrap().len(),
                lookups
            );
            assert!(matches!(
                fixture
                    .coordinator
                    .turn_result(TurnResultParams {
                        operation_id: "delayed-after-reopen".into()
                    })
                    .unwrap(),
                TurnResult::Unavailable { .. }
            ));
        }
    }
}

#[tokio::test]
async fn delayed_old_completion_preserves_a_newer_guard_before_and_after_its_start_reply() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let fixture = Fixture::new_persistent(FakeWorker {
        turn_control: WorkerState {
            history_unavailable: true,
            ..Default::default()
        },
        ..FakeWorker::paused_turn_start(entered.clone(), release.clone())
    });
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    release.notify_one();
    let old = send(&fixture, &workspace, "old-after-reopen").await;
    entered.notified().await;
    let fixture = reopen(fixture);
    fixture.worker.set_native_status(
        workspace.codex_thread_id.as_deref().unwrap(),
        CodexThreadStatus::Idle,
    );
    let next = send(&fixture, &workspace, "newer-after-reopen");
    tokio::pin!(next);
    tokio::select! { _ = entered.notified() => {}, done = &mut next => panic!("start did not pause: {done:?}") }
    let guard = fixture
        .coordinator
        .runtime_turn_for_workspace(&workspace)
        .unwrap();
    assert!(guard.native_result_id.is_none());
    fixture
        .coordinator
        .record_codex_event(notification(&old, TurnTerminalStatus::Completed))
        .unwrap();
    assert_eq!(
        fixture.coordinator.runtime_turn_for_workspace(&workspace),
        Some(guard)
    );
    let observed = fixture
        .coordinator
        .get_turn(get_params(&fixture, &workspace, "old-after-reopen"))
        .await
        .unwrap();
    assert_eq!(observed.outcome, Some(TurnTerminalStatus::Completed));
    release.notify_one();
    let sent = next.await;
    let guard = fixture
        .coordinator
        .runtime_turn_for_workspace(&workspace)
        .unwrap();
    assert_eq!(guard.native_result_id, sent.codex_turn_id);
    *fixture.coordinator.completed_turn_results.lock().unwrap() = Default::default();
    fixture
        .coordinator
        .record_codex_event(notification(&old, TurnTerminalStatus::Completed))
        .unwrap();
    assert_eq!(
        fixture.coordinator.runtime_turn_for_workspace(&workspace),
        Some(guard)
    );
    let newer = fixture
        .coordinator
        .get_turn(get_params(&fixture, &workspace, "newer-after-reopen"))
        .await
        .unwrap();
    assert_eq!(newer.state, TurnObservationState::Running);
    assert!(newer.outcome.is_none());
    assert!(matches!(
        fixture
            .coordinator
            .turn_result(TurnResultParams {
                operation_id: "newer-after-reopen".into()
            })
            .unwrap(),
        TurnResult::Pending { .. }
    ));
}

#[tokio::test]
async fn delayed_conflicting_completions_cannot_replace_the_first_durable_outcome() {
    let fixture = Fixture::new_persistent(worker());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let sent = send(&fixture, &workspace, "conflicting-after-reopen").await;
    let fixture = reopen(fixture);
    fixture
        .coordinator
        .record_codex_event(notification(&sent, TurnTerminalStatus::Interrupted))
        .unwrap();
    let first = fixture
        .store
        .operation_by_client_id("conflicting-after-reopen")
        .unwrap()
        .unwrap();
    assert_eq!(first.terminal_status, Some(TurnTerminalStatus::Interrupted));
    assert!(matches!(
        fixture
            .coordinator
            .record_codex_event(notification(&sent, TurnTerminalStatus::Failed)),
        Err(crate::store::StoreError::OperationResultConflict { .. })
    ));
    assert_eq!(
        fixture
            .store
            .operation_by_client_id("conflicting-after-reopen")
            .unwrap(),
        Some(first)
    );
}

#[tokio::test]
async fn delayed_events_cannot_invent_acceptance_or_borrow_another_threads_turn_id() {
    let mut native = worker();
    native.turn_control.history_unavailable = true;
    let fixture = Fixture::new_persistent(native);
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let sent = send(&fixture, &workspace, "exact-pair-only").await;
    let other = fixture
        .create_and_materialize(fresh_create_params(
            fixture.source.clone(),
            "other-thread",
            "create-other-thread",
        ))
        .await;
    let (unconfirmed, _) = fixture
        .store
        .prepare_operation(crate::store::NewOperation {
            operation_id: "unconfirmed-after-reopen".into(),
            workspace_id: workspace.id.clone(),
            kind: crate::store::OperationKind::TurnStart,
            request_fingerprint: "unconfirmed-after-reopen".into(),
        })
        .unwrap();
    fixture
        .store
        .begin_operation_dispatch(
            &unconfirmed.id,
            workspace.codex_thread_id.as_deref().unwrap(),
        )
        .unwrap();
    fixture
        .store
        .mark_operation_uncertain(&unconfirmed.id)
        .unwrap();
    let fixture = reopen(fixture);
    for (thread, turn) in [
        (
            other.codex_thread_id.as_deref().unwrap(),
            sent.codex_turn_id.as_deref().unwrap(),
        ),
        (
            workspace.codex_thread_id.as_deref().unwrap(),
            "unconfirmed-native-turn",
        ),
    ] {
        fixture
            .coordinator
            .record_codex_event(CodexEvent::Notification {
                method: "turn/completed".into(),
                params: json!({"threadId": thread, "turn": {"id": turn, "status": "interrupted"}}),
            })
            .unwrap();
    }
    for key in ["exact-pair-only", "unconfirmed-after-reopen"] {
        let operation = fixture.store.operation_by_client_id(key).unwrap().unwrap();
        assert!(operation.terminal_status.is_none());
        assert!(operation.terminal_observed_at_ms.is_none());
    }
    assert!(
        fixture
            .worker
            .turn_control
            .lookups
            .lock()
            .unwrap()
            .is_empty()
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
    let observed = fixture
        .coordinator
        .get_turn(get_params(&fixture, &workspace, "unconfirmed-after-reopen"))
        .await
        .unwrap();
    assert_eq!(observed.dispatch, TurnDispatchState::Uncertain);
    assert_eq!(
        observed.unavailable_reason,
        Some(TurnUnavailableReason::AcceptanceUnconfirmed)
    );
    assert!(observed.codex_turn_id.is_none());
}

#[tokio::test]
async fn durable_acceptance_does_not_bypass_conflicting_early_events_on_its_own_guard() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let (fixture, workspace) = setup(FakeWorker {
        turn_control: WorkerState {
            history_unavailable: true,
            ..Default::default()
        },
        ..FakeWorker::paused_turn_start(entered.clone(), release.clone())
    })
    .await;
    let start = send(&fixture, &workspace, "conflict-across-binding");
    tokio::pin!(start);
    tokio::select! { _ = entered.notified() => {}, done = &mut start => panic!("start did not pause: {done:?}") }
    let thread = workspace.codex_thread_id.as_deref().unwrap();
    fixture
        .coordinator
        .observe_native_turn_completed(thread, "turn-1", TurnTerminalStatus::Completed)
        .unwrap();
    let operation = fixture
        .store
        .operation_by_client_id("conflict-across-binding")
        .unwrap()
        .unwrap();
    // Precisely between durable acceptance and binding the in-flight guard.
    fixture
        .store
        .accept_operation(&operation.id, "turn-1")
        .unwrap();
    fixture
        .coordinator
        .observe_native_turn_completed(thread, "turn-1", TurnTerminalStatus::Failed)
        .unwrap();
    release.notify_one();
    start.await;
    let observed = fixture
        .coordinator
        .get_turn(get_params(&fixture, &workspace, "conflict-across-binding"))
        .await
        .unwrap();
    assert_eq!(observed.dispatch, TurnDispatchState::Accepted);
    assert_eq!(observed.state, TurnObservationState::Unavailable);
    assert!(observed.outcome.is_none());
    assert!(
        fixture
            .coordinator
            .runtime_turn_for_workspace(&workspace)
            .unwrap()
            .completion_tracking_lost
    );
}
