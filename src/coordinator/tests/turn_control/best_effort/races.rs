use super::*;

#[tokio::test]
async fn concurrent_retries_observe_the_uncertain_claim_without_another_native_call() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let (fixture, workspace) = setup(FakeWorker {
        turn_control: WorkerState {
            interrupt_supported: Some(false),
            best_effort_supported: true,
            interrupt_entered: Some(entered.clone()),
            interrupt_release: Some(release.clone()),
            ..Default::default()
        },
        ..Default::default()
    })
    .await;
    send(&fixture, &workspace, "concurrent-cancel").await;
    let best = params(&fixture, &workspace, "concurrent-cancel");
    let first = fixture.coordinator.interrupt_turn(best.clone());
    tokio::pin!(first);
    tokio::select! { _ = entered.notified() => {}, done = &mut first => panic!("interrupt did not pause: {done:?}") }
    let retry = fixture.coordinator.interrupt_turn(best).await.unwrap();
    assert_eq!(retry.interruption, TurnInterruptionState::Uncertain);
    assert_eq!(
        retry.turn.interrupt_attempt.unwrap().state,
        TurnInterruptAttemptState::Uncertain
    );
    assert!(retry.turn.outcome.is_none());
    assert_eq!(
        fixture.worker.turn_control.interrupts.lock().unwrap().len(),
        1
    );
    release.notify_one();
    let requested = first.await.unwrap();
    assert_eq!(requested.interruption, TurnInterruptionState::Requested);
    assert_eq!(
        requested.turn.interrupt_requested_at_ms,
        retry.turn.interrupt_requested_at_ms
    );
}

#[tokio::test]
async fn a_native_check_queue_race_cannot_attribute_the_next_turns_interruption_to_the_original() {
    // Controlled reproduction of the KNOWN native ordering gap: request A
    // validates, A completes, native input starts B, queued interrupt hits B.
    // This is not proof that the real native implementation is atomic/safe.
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let (fixture, workspace) = setup(FakeWorker {
        turn_control: WorkerState {
            interrupt_supported: Some(false),
            best_effort_supported: true,
            interrupt_entered: Some(entered.clone()),
            interrupt_release: Some(release.clone()),
            ..Default::default()
        },
        ..Default::default()
    })
    .await;
    let first = send(&fixture, &workspace, "race-original").await;
    let interrupt =
        fixture
            .coordinator
            .interrupt_turn(params(&fixture, &workspace, "race-original"));
    tokio::pin!(interrupt);
    tokio::select! { _ = entered.notified() => {}, done = &mut interrupt => panic!("interrupt did not pause: {done:?}") }
    complete(&fixture, &first, TurnTerminalStatus::Completed);
    let next = send(&fixture, &workspace, "race-next").await;
    complete(&fixture, &next, TurnTerminalStatus::Interrupted);
    release.notify_one();
    let original = interrupt.await.unwrap();
    assert_eq!(
        original.interruption,
        TurnInterruptionState::AlreadyFinished
    );
    assert_eq!(original.turn.outcome, Some(TurnTerminalStatus::Completed));
    assert_eq!(original.turn.codex_turn_id, first.codex_turn_id);
    assert_eq!(
        original.turn.interrupt_attempt.unwrap().state,
        TurnInterruptAttemptState::Requested
    );
    let other = fixture
        .coordinator
        .get_turn(get_params(&fixture, &workspace, "race-next"))
        .await
        .unwrap();
    assert_eq!(other.outcome, Some(TurnTerminalStatus::Interrupted));
    assert!(other.interrupt_attempt.is_none());
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
        )]
    );
}

#[tokio::test]
async fn completion_beats_a_lost_reply_without_becoming_a_fake_interruption() {
    for status in [TurnTerminalStatus::Completed, TurnTerminalStatus::Failed] {
        let entered = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let (fixture, workspace) = setup(FakeWorker {
            turn_control: WorkerState {
                interrupt_supported: Some(false),
                best_effort_supported: true,
                interrupt_entered: Some(entered.clone()),
                interrupt_release: Some(release.clone()),
                interrupt_error: StdMutex::new(true),
                ..Default::default()
            },
            ..Default::default()
        })
        .await;
        let sent = send(&fixture, &workspace, "completion-race").await;
        let interrupt =
            fixture
                .coordinator
                .interrupt_turn(params(&fixture, &workspace, "completion-race"));
        tokio::pin!(interrupt);
        tokio::select! { _ = entered.notified() => {}, done = &mut interrupt => panic!("interrupt did not pause: {done:?}") }
        complete(&fixture, &sent, status);
        release.notify_one();
        let result = interrupt.await.unwrap();
        assert_eq!(result.interruption, TurnInterruptionState::AlreadyFinished);
        assert_eq!(result.turn.outcome, Some(status));
        assert_eq!(
            result.turn.interrupt_attempt.unwrap().state,
            TurnInterruptAttemptState::Uncertain
        );
    }
}
