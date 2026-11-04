use super::*;
use std::time::Duration;

#[tokio::test]
async fn a_stalled_lookup_returns_unknown_without_inventing_a_terminal_outcome() {
    let (fixture, workspace) = setup(FakeWorker {
        turn_control: WorkerState {
            lookup_release: Some(Arc::new(Notify::new())),
            ..Default::default()
        },
        ..Default::default()
    })
    .await;
    send(&fixture, &workspace, "stalled-read").await;
    let observed = tokio::time::timeout(
        Duration::from_secs(4),
        fixture
            .coordinator
            .get_turn(get_params(&fixture, &workspace, "stalled-read")),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(observed.dispatch, TurnDispatchState::Accepted);
    assert_eq!(observed.state, TurnObservationState::Unavailable);
    assert_eq!(
        observed.unavailable_reason,
        Some(TurnUnavailableReason::NativeLookupTimedOut)
    );
    assert!(observed.outcome.is_none());
}

#[tokio::test]
async fn unsupported_interruption_does_not_wait_for_metadata_or_mark_an_attempt() {
    let (fixture, workspace) = setup(FakeWorker {
        turn_control: WorkerState {
            interrupt_supported: Some(false),
            lookup_release: Some(Arc::new(Notify::new())),
            ..Default::default()
        },
        ..Default::default()
    })
    .await;
    send(&fixture, &workspace, "unsupported").await;
    let error = tokio::time::timeout(
        Duration::from_secs(1),
        fixture
            .coordinator
            .interrupt_turn(interrupt_params(&fixture, &workspace, "unsupported")),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert_eq!(error.code(), "TURN_INTERRUPT_UNSUPPORTED");
    let op = fixture
        .store
        .operation_by_client_id("unsupported")
        .unwrap()
        .unwrap();
    assert!(op.interrupt_requested_at_ms.is_none());
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
}

#[tokio::test]
async fn a_stalled_supported_interrupt_preserves_intent_and_explicit_uncertainty() {
    let (fixture, workspace) = setup(FakeWorker {
        turn_control: WorkerState {
            interrupt_release: Some(Arc::new(Notify::new())),
            ..Default::default()
        },
        ..Default::default()
    })
    .await;
    let sent = send(&fixture, &workspace, "interrupt-timeout").await;
    let error = tokio::time::timeout(
        Duration::from_secs(4),
        fixture.coordinator.interrupt_turn(interrupt_params(
            &fixture,
            &workspace,
            "interrupt-timeout",
        )),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert_eq!(error.code(), "TURN_INTERRUPT_UNCERTAIN");
    let op = fixture
        .store
        .operation_by_client_id("interrupt-timeout")
        .unwrap()
        .unwrap();
    assert!(op.interrupt_requested_at_ms.is_some());
    assert!(op.terminal_status.is_none());
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
        )]
    );
}

#[tokio::test]
async fn a_saved_completion_wins_even_when_an_inflight_native_read_times_out() {
    let entered = Arc::new(Notify::new());
    let (fixture, workspace) = setup(FakeWorker {
        turn_control: WorkerState {
            lookup_entered: Some(entered.clone()),
            lookup_release: Some(Arc::new(Notify::new())),
            ..Default::default()
        },
        ..Default::default()
    })
    .await;
    let sent = send(&fixture, &workspace, "completion-beats-timeout").await;
    let get =
        fixture
            .coordinator
            .get_turn(get_params(&fixture, &workspace, "completion-beats-timeout"));
    tokio::pin!(get);
    tokio::select! { _ = entered.notified() => {}, done = &mut get => panic!("read did not pause: {done:?}") }
    complete(&fixture, &sent, TurnTerminalStatus::Failed);
    let observed = get.await.unwrap();
    assert_eq!(observed.state, TurnObservationState::Finished);
    assert_eq!(observed.outcome, Some(TurnTerminalStatus::Failed));
    assert!(observed.unavailable_reason.is_none());
}

#[tokio::test]
async fn an_unindexed_history_after_restart_is_explicitly_unavailable() {
    let (fixture, workspace) = setup(FakeWorker {
        turn_control: WorkerState {
            history_unavailable: true,
            ..Default::default()
        },
        ..Default::default()
    })
    .await;
    send(&fixture, &workspace, "unindexed").await;
    let recovered = fixture.recovery_coordinator(fixture.worker.clone(), "no-index-restart");
    let observed = recovered
        .get_turn(get_params(&fixture, &workspace, "unindexed"))
        .await
        .unwrap();
    assert_eq!(observed.state, TurnObservationState::Unavailable);
    assert_eq!(
        observed.unavailable_reason,
        Some(TurnUnavailableReason::NativeHistoryUnavailable)
    );
    assert!(observed.outcome.is_none());
}
