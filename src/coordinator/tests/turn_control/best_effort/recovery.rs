use super::*;

mod delayed_completions;

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
    let store = Arc::new(crate::store::Store::open(_temp.path().join("coordinator.db")).unwrap());
    let coordinator = Coordinator::new(
        store.clone(),
        crate::git::Git::default(),
        worker.clone(),
        worktrees.clone(),
        codex_home.clone(),
        Arc::new(HookRegistry::empty()),
        "after-real-db-reopen".into(),
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
async fn lost_native_and_client_replies_survive_actual_database_reopen_without_retrying() {
    for lost_native_reply in [false, true] {
        let fixture = Fixture::new_persistent(worker());
        fixture.register().await;
        let workspace = fixture
            .create_and_materialize(fixture.create_params())
            .await;
        send(&fixture, &workspace, "reopen-attempt").await;
        *fixture.worker.turn_control.interrupt_error.lock().unwrap() = lost_native_reply;
        let first = fixture
            .coordinator
            .interrupt_turn(params(&fixture, &workspace, "reopen-attempt"))
            .await;
        if lost_native_reply {
            assert_eq!(first.unwrap_err().code(), "CODEX_ERROR");
        } else {
            assert_eq!(
                first.unwrap().interruption,
                TurnInterruptionState::Requested
            );
        }
        let saved = fixture
            .store
            .operation_by_client_id("reopen-attempt")
            .unwrap()
            .unwrap();
        let fixture = reopen(fixture);
        *fixture.worker.turn_control.interrupt_error.lock().unwrap() = false;
        let repeated = fixture
            .coordinator
            .interrupt_turn(params(&fixture, &workspace, "reopen-attempt"))
            .await
            .unwrap();
        assert_eq!(
            repeated.turn.interrupt_requested_at_ms,
            saved.interrupt_requested_at_ms
        );
        assert_eq!(repeated.turn.interrupt_attempt, saved.interrupt_attempt);
        assert_eq!(
            repeated.interruption,
            if lost_native_reply {
                TurnInterruptionState::Uncertain
            } else {
                TurnInterruptionState::Requested
            }
        );
        assert!(repeated.turn.outcome.is_none());
        assert_eq!(
            fixture.worker.turn_control.interrupts.lock().unwrap().len(),
            1
        );
    }
}

#[tokio::test]
async fn a_crash_after_claim_before_native_dispatch_stays_uncertain_and_is_never_auto_sent() {
    let fixture = Fixture::new_persistent(worker());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    send(&fixture, &workspace, "claim-before-write").await;
    let op = fixture
        .store
        .operation_by_client_id("claim-before-write")
        .unwrap()
        .unwrap();
    assert!(
        fixture
            .store
            .claim_turn_interrupt(&op.id, TurnInterruptMode::BestEffort)
            .unwrap()
            .1
    );
    let fixture = reopen(fixture);
    let result = fixture
        .coordinator
        .interrupt_turn(params(&fixture, &workspace, "claim-before-write"))
        .await
        .unwrap();
    assert_eq!(result.interruption, TurnInterruptionState::Uncertain);
    assert_eq!(result.turn.dispatch, TurnDispatchState::Accepted);
    assert!(result.turn.outcome.is_none());
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
async fn an_interrupt_timeout_is_uncertain_and_even_an_identical_retry_does_not_send_again() {
    let (fixture, workspace) = setup(FakeWorker {
        turn_control: WorkerState {
            interrupt_supported: Some(false),
            best_effort_supported: true,
            interrupt_release: Some(Arc::new(Notify::new())),
            ..Default::default()
        },
        ..Default::default()
    })
    .await;
    send(&fixture, &workspace, "best-effort-timeout").await;
    let best = params(&fixture, &workspace, "best-effort-timeout");
    assert_eq!(
        fixture
            .coordinator
            .interrupt_turn(best.clone())
            .await
            .unwrap_err()
            .code(),
        "TURN_INTERRUPT_UNCERTAIN"
    );
    let replay = fixture.coordinator.interrupt_turn(best).await.unwrap();
    assert_eq!(replay.interruption, TurnInterruptionState::Uncertain);
    assert_eq!(
        replay.turn.interrupt_attempt.unwrap().state,
        TurnInterruptAttemptState::Uncertain
    );
    assert!(replay.turn.outcome.is_none());
    assert_eq!(
        fixture.worker.turn_control.interrupts.lock().unwrap().len(),
        1
    );
}
