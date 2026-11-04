use super::*;

#[tokio::test]
async fn repeated_interrupts_are_pending_until_the_exact_completion_is_observed() {
    let (fixture, workspace) = setup(FakeWorker::default()).await;
    let sent = send(&fixture, &workspace, "cancel-me").await;
    let params = interrupt_params(&fixture, &workspace, "cancel-me");
    let first = fixture
        .coordinator
        .interrupt_turn(params.clone())
        .await
        .unwrap();
    assert_eq!(first.interruption, TurnInterruptionState::Requested);
    assert_eq!(first.turn.state, TurnObservationState::Running);
    assert!(first.turn.outcome.is_none());
    let repeated = fixture
        .coordinator
        .interrupt_turn(params.clone())
        .await
        .unwrap();
    assert_eq!(
        first.turn.interrupt_requested_at_ms,
        repeated.turn.interrupt_requested_at_ms
    );
    complete(&fixture, &sent, TurnTerminalStatus::Interrupted);
    let confirmed = fixture.coordinator.interrupt_turn(params).await.unwrap();
    assert_eq!(confirmed.interruption, TurnInterruptionState::Confirmed);
    assert_eq!(
        confirmed.turn.outcome,
        Some(TurnTerminalStatus::Interrupted)
    );
    assert_eq!(
        fixture
            .worker
            .turn_control
            .interrupts
            .lock()
            .unwrap()
            .as_slice(),
        &[(
            sent.workspace.codex_thread_id.unwrap(),
            sent.codex_turn_id.unwrap()
        ),]
    );
}

#[tokio::test]
async fn late_interrupt_of_a_completed_operation_cannot_touch_the_next_turn() {
    let (fixture, workspace) = setup(FakeWorker::default()).await;
    let first = send(&fixture, &workspace, "old").await;
    complete(&fixture, &first, TurnTerminalStatus::Completed);
    let second = send(&fixture, &workspace, "new").await;
    let result = fixture
        .coordinator
        .interrupt_turn(interrupt_params(&fixture, &workspace, "old"))
        .await
        .unwrap();
    assert_eq!(result.interruption, TurnInterruptionState::AlreadyFinished);
    assert_eq!(result.turn.codex_turn_id, first.codex_turn_id);
    assert!(
        fixture
            .worker
            .turn_control
            .interrupts
            .lock()
            .unwrap()
            .is_empty()
    );
    let current = fixture
        .coordinator
        .get_turn(get_params(&fixture, &workspace, "new"))
        .await
        .unwrap();
    assert_eq!(current.state, TurnObservationState::Running);
    assert_eq!(current.codex_turn_id, second.codex_turn_id);
}

#[tokio::test]
async fn completion_and_a_new_turn_can_race_a_delayed_interrupt_reply() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let worker = FakeWorker {
        turn_control: WorkerState {
            interrupt_entered: Some(entered.clone()),
            interrupt_release: Some(release.clone()),
            ..Default::default()
        },
        ..Default::default()
    };
    let (fixture, workspace) = setup(worker).await;
    let first = send(&fixture, &workspace, "raced").await;
    let request = fixture
        .coordinator
        .interrupt_turn(interrupt_params(&fixture, &workspace, "raced"));
    tokio::pin!(request);
    tokio::select! { _ = entered.notified() => {}, result = &mut request => panic!("interrupt did not pause: {result:?}") }
    complete(&fixture, &first, TurnTerminalStatus::Completed);
    let next = send(&fixture, &workspace, "next").await;
    *fixture.worker.turn_control.interrupt_error.lock().unwrap() = true;
    release.notify_one();
    let result = request.await.unwrap();
    assert_eq!(result.interruption, TurnInterruptionState::AlreadyFinished);
    assert_eq!(result.turn.outcome, Some(TurnTerminalStatus::Completed));
    assert_eq!(
        fixture
            .worker
            .turn_control
            .interrupts
            .lock()
            .unwrap()
            .as_slice(),
        &[(
            first.workspace.codex_thread_id.unwrap(),
            first.codex_turn_id.unwrap()
        ),]
    );
    assert_eq!(
        fixture
            .coordinator
            .get_turn(get_params(&fixture, &workspace, "next"))
            .await
            .unwrap()
            .codex_turn_id,
        next.codex_turn_id
    );
}

#[tokio::test]
async fn lost_interrupt_ack_is_uncertain_and_replayed_without_redispatch() {
    let (fixture, workspace) = setup(FakeWorker::default()).await;
    send(&fixture, &workspace, "lost-cancel").await;
    *fixture.worker.turn_control.interrupt_error.lock().unwrap() = true;
    let params = interrupt_params(&fixture, &workspace, "lost-cancel");
    assert_eq!(
        fixture
            .coordinator
            .interrupt_turn(params.clone())
            .await
            .unwrap_err()
            .code(),
        "CODEX_ERROR"
    );
    let current = fixture
        .coordinator
        .get_turn(get_params(&fixture, &workspace, "lost-cancel"))
        .await
        .unwrap();
    assert!(current.interrupt_requested_at_ms.is_some());
    assert!(current.outcome.is_none());
    *fixture.worker.turn_control.interrupt_error.lock().unwrap() = false;
    assert_eq!(
        fixture
            .coordinator
            .interrupt_turn(params)
            .await
            .unwrap()
            .interruption,
        TurnInterruptionState::Uncertain
    );
    assert_eq!(
        fixture.worker.turn_control.interrupts.lock().unwrap().len(),
        1
    );
}
