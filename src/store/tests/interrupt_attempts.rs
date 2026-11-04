use super::*;
use crate::domain::turns::{TurnInterruptAttemptState as AttemptState, TurnInterruptMode as Mode};
use std::sync::Arc;

fn accepted(store: &Store) -> Operation {
    let operation = super::turn_control::operation(store);
    store
        .begin_operation_dispatch(&operation.id, "thread")
        .unwrap();
    store.accept_operation(&operation.id, "turn").unwrap()
}

#[test]
fn a_claim_is_one_shot_and_mode_changes_cannot_bypass_it() {
    let store = Store::in_memory().unwrap();
    let op = accepted(&store);
    let (claimed, dispatch) = store
        .claim_turn_interrupt(&op.id, Mode::BestEffort)
        .unwrap();
    assert!(dispatch);
    assert_eq!(
        claimed.interrupt_attempt.unwrap().state,
        AttemptState::Uncertain
    );
    assert_eq!(
        store
            .claim_turn_interrupt(&op.id, Mode::BestEffort)
            .unwrap(),
        (claimed.clone(), false)
    );
    assert!(matches!(
        store.claim_turn_interrupt(&op.id, Mode::Strict),
        Err(StoreError::InterruptModeConflict { .. })
    ));
    let rejected = store
        .record_turn_interrupt_reply(&op.id, AttemptState::Rejected)
        .unwrap();
    assert_eq!(
        rejected.interrupt_requested_at_ms,
        claimed.interrupt_requested_at_ms
    );
    assert_eq!(
        store
            .claim_turn_interrupt(&op.id, Mode::BestEffort)
            .unwrap(),
        (rejected.clone(), false)
    );
    assert!(
        store
            .record_turn_interrupt_reply(&op.id, AttemptState::Requested)
            .is_err()
    );
    assert_eq!(
        store
            .record_turn_interrupt_reply(&op.id, AttemptState::Rejected)
            .unwrap(),
        rejected
    );
}

#[test]
fn acknowledgements_and_receipts_survive_a_real_database_close_and_reopen() {
    for state in [
        AttemptState::Uncertain,
        AttemptState::Requested,
        AttemptState::Rejected,
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("attempt.db");
        let expected = {
            let store = Store::open(&path).unwrap();
            let op = accepted(&store);
            let claimed = store
                .claim_turn_interrupt(&op.id, Mode::BestEffort)
                .unwrap()
                .0;
            if state == AttemptState::Uncertain {
                claimed
            } else {
                store.record_turn_interrupt_reply(&op.id, state).unwrap()
            }
        };
        let store = Store::open(&path).unwrap();
        assert_eq!(
            store
                .claim_turn_interrupt(&expected.id, Mode::BestEffort)
                .unwrap(),
            (expected.clone(), false)
        );
        assert_eq!(expected.interrupt_attempt.unwrap().state, state);
        let receipt = store
            .record_operation_completion(
                &expected.id,
                "thread",
                "turn",
                crate::domain::turns::TurnTerminalStatus::Completed,
            )
            .unwrap();
        assert_eq!(receipt.interrupt_attempt, expected.interrupt_attempt);
        assert!(
            !store
                .claim_turn_interrupt(&expected.id, Mode::Strict)
                .unwrap()
                .1
        );
    }
}

#[test]
fn v16_markers_migrate_to_strict_uncertainty_never_an_invented_ack() {
    let store = Store::in_memory().unwrap();
    let op = accepted(&store);
    let connection = store.lock().unwrap();
    connection
        .execute_batch(
            "ALTER TABLE operations DROP COLUMN interrupt_mode;
        ALTER TABLE operations DROP COLUMN interrupt_state; PRAGMA user_version=16;",
        )
        .unwrap();
    connection
        .execute(
            "UPDATE operations SET interrupt_requested_at_ms = 42 WHERE id = ?1",
            [&op.id],
        )
        .unwrap();
    crate::store::migrations::migrate(&connection).unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        17
    );
    drop(connection);
    let migrated = store
        .operation_by_client_id(&op.operation_id)
        .unwrap()
        .unwrap();
    assert_eq!(migrated.interrupt_requested_at_ms, Some(42));
    assert_eq!(migrated.interrupt_attempt.unwrap().mode, Mode::Strict);
    assert_eq!(
        migrated.interrupt_attempt.unwrap().state,
        AttemptState::Uncertain
    );
    assert!(!store.claim_turn_interrupt(&op.id, Mode::Strict).unwrap().1);
}

#[test]
fn independent_database_connections_cannot_both_acquire_dispatch_permission() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("concurrent-attempt.db");
    let op = {
        let store = Store::open(&path).unwrap();
        accepted(&store)
    };
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let (path, id, barrier) = (path.clone(), op.id.clone(), barrier.clone());
            std::thread::spawn(move || {
                let store = Store::open(&path).unwrap();
                barrier.wait();
                store.claim_turn_interrupt(&id, Mode::BestEffort).unwrap()
            })
        })
        .collect();
    let results: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    assert_eq!(results.iter().filter(|(_, dispatch)| *dispatch).count(), 1);
    assert_eq!(results[0].0, results[1].0);
}

#[test]
fn a_reply_without_a_claim_and_incomplete_stored_attempts_fail_closed() {
    let store = Store::in_memory().unwrap();
    let op = accepted(&store);
    assert!(
        store
            .record_turn_interrupt_reply(&op.id, AttemptState::Requested)
            .is_err()
    );
    let connection = store.lock().unwrap();
    connection
        .execute(
            "UPDATE operations SET interrupt_requested_at_ms = 42 WHERE id = ?1",
            [&op.id],
        )
        .unwrap();
    drop(connection);
    assert!(store.operation_by_client_id(&op.operation_id).is_err());
}
