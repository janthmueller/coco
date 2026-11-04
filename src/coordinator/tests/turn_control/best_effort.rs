use super::*;

mod races;
mod recovery;

fn worker() -> FakeWorker {
    FakeWorker {
        turn_control: WorkerState {
            interrupt_supported: Some(false),
            best_effort_supported: true,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn params(fixture: &Fixture, workspace: &Workspace, key: &str) -> TurnInterruptParams {
    TurnInterruptParams {
        mode: TurnInterruptMode::BestEffort,
        ..interrupt_params(fixture, workspace, key)
    }
}

#[tokio::test]
async fn strict_stays_unsupported_but_explicit_best_effort_uses_saved_ids_once() {
    let (fixture, workspace) = setup(worker()).await;
    let sent = send(&fixture, &workspace, "native-best-effort").await;
    let strict = interrupt_params(&fixture, &workspace, "native-best-effort");
    assert_eq!(
        fixture
            .coordinator
            .interrupt_turn(strict.clone())
            .await
            .unwrap_err()
            .code(),
        "TURN_INTERRUPT_UNSUPPORTED"
    );
    assert!(
        fixture
            .store
            .operation_by_client_id("native-best-effort")
            .unwrap()
            .unwrap()
            .interrupt_attempt
            .is_none()
    );
    let best = params(&fixture, &workspace, "native-best-effort");
    let first = fixture
        .coordinator
        .interrupt_turn(best.clone())
        .await
        .unwrap();
    assert_eq!(first.interruption, TurnInterruptionState::Requested);
    assert!(first.turn.outcome.is_none());
    assert_eq!(
        first.turn.interrupt_attempt.unwrap().mode,
        TurnInterruptMode::BestEffort
    );
    assert_eq!(
        first.turn.interrupt_attempt.unwrap().state,
        TurnInterruptAttemptState::Requested
    );
    let repeated = fixture
        .coordinator
        .interrupt_turn(best.clone())
        .await
        .unwrap();
    assert_eq!(
        first.turn.interrupt_requested_at_ms,
        repeated.turn.interrupt_requested_at_ms
    );
    assert_eq!(
        fixture
            .coordinator
            .interrupt_turn(strict)
            .await
            .unwrap_err()
            .code(),
        "TURN_INTERRUPT_MODE_CONFLICT"
    );
    complete(&fixture, &sent, TurnTerminalStatus::Interrupted);
    let confirmed = fixture.coordinator.interrupt_turn(best).await.unwrap();
    assert_eq!(confirmed.interruption, TurnInterruptionState::Confirmed);
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
async fn native_stale_target_rejection_is_retained_and_never_falls_back_or_retries() {
    let (fixture, workspace) = setup(worker()).await;
    send(&fixture, &workspace, "stale-target").await;
    *fixture
        .worker
        .turn_control
        .interrupt_rejected
        .lock()
        .unwrap() = true;
    let best = params(&fixture, &workspace, "stale-target");
    assert_eq!(
        fixture
            .coordinator
            .interrupt_turn(best.clone())
            .await
            .unwrap_err()
            .code(),
        "TURN_INTERRUPT_REJECTED"
    );
    let snapshot = fixture.coordinator.interrupt_turn(best).await.unwrap();
    assert_eq!(snapshot.interruption, TurnInterruptionState::Rejected);
    assert_eq!(
        snapshot.turn.interrupt_attempt.unwrap().state,
        TurnInterruptAttemptState::Rejected
    );
    assert!(snapshot.turn.outcome.is_none());
    assert_eq!(snapshot.turn.state, TurnObservationState::Unavailable);
    assert_eq!(
        fixture.worker.turn_control.interrupts.lock().unwrap().len(),
        1
    );
}

#[tokio::test]
async fn terminal_unconfirmed_and_empty_bindings_never_send_even_in_best_effort() {
    let (fixture, workspace) = setup(worker()).await;
    let (operation, _) = fixture
        .store
        .prepare_operation(crate::store::NewOperation {
            operation_id: "no-target".into(),
            workspace_id: workspace.id.clone(),
            kind: crate::store::OperationKind::TurnStart,
            request_fingerprint: "not-dispatched".into(),
        })
        .unwrap();
    let best = params(&fixture, &workspace, "no-target");
    assert_eq!(
        fixture
            .coordinator
            .interrupt_turn(best.clone())
            .await
            .unwrap()
            .interruption,
        TurnInterruptionState::NotRequested
    );
    fixture
        .store
        .begin_operation_dispatch(&operation.id, workspace.codex_thread_id.as_deref().unwrap())
        .unwrap();
    fixture
        .store
        .mark_operation_uncertain(&operation.id)
        .unwrap();
    assert_eq!(
        fixture
            .coordinator
            .interrupt_turn(best.clone())
            .await
            .unwrap()
            .interruption,
        TurnInterruptionState::NotRequested
    );
    fixture.store.accept_operation(&operation.id, "").unwrap();
    assert_eq!(
        fixture
            .coordinator
            .interrupt_turn(best)
            .await
            .unwrap()
            .interruption,
        TurnInterruptionState::NotRequested
    );
    let sent = send(&fixture, &workspace, "already-complete").await;
    complete(&fixture, &sent, TurnTerminalStatus::Completed);
    let result = fixture
        .coordinator
        .interrupt_turn(params(&fixture, &workspace, "already-complete"))
        .await
        .unwrap();
    assert_eq!(result.interruption, TurnInterruptionState::AlreadyFinished);
    assert!(result.turn.interrupt_attempt.is_none());
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
async fn an_unverified_best_effort_backend_is_rejected_before_recording_any_attempt() {
    let (fixture, workspace) = setup(FakeWorker::default()).await;
    send(&fixture, &workspace, "unsupported-best-effort").await;
    assert_eq!(
        fixture
            .coordinator
            .interrupt_turn(params(&fixture, &workspace, "unsupported-best-effort"))
            .await
            .unwrap_err()
            .code(),
        "TURN_INTERRUPT_UNSUPPORTED"
    );
    assert!(
        fixture
            .store
            .operation_by_client_id("unsupported-best-effort")
            .unwrap()
            .unwrap()
            .interrupt_attempt
            .is_none()
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
