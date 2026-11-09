use super::*;
use crate::domain::turns::{TurnInterruptMode, TurnTerminalStatus};

pub(super) fn operation(store: &Store) -> Operation {
    let repo = repository(Path::new("/tmp/receipt-fixture"));
    store.register_repository(&repo).unwrap();
    let workspace = ready_workspace(store, &repo.id, "receipt");
    let (operation, _) = store
        .prepare_operation(NewOperation {
            operation_id: "receipt-send".to_owned(),
            workspace_id: workspace.id,
            kind: OperationKind::TurnStart,
            request_fingerprint: "frozen".to_owned(),
        })
        .unwrap();
    operation
}

#[test]
fn technical_receipt_and_interrupt_intent_survive_actual_database_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("receipt.db");
    let expected = {
        let store = Store::open(&path).unwrap();
        let op = operation(&store);
        store
            .begin_operation_dispatch(&op.id, "original-thread")
            .unwrap();
        store.accept_operation(&op.id, "original-turn").unwrap();
        store
            .claim_turn_interrupt(&op.id, TurnInterruptMode::Strict)
            .unwrap();
        store
            .record_operation_completion(
                &op.id,
                "original-thread",
                "original-turn",
                TurnTerminalStatus::Interrupted,
            )
            .unwrap()
    };
    let store = Store::open(&path).unwrap();
    assert_eq!(
        store.operation_by_client_id("receipt-send").unwrap(),
        Some(expected.clone())
    );
    assert_eq!(
        expected.terminal_status,
        Some(TurnTerminalStatus::Interrupted)
    );
    assert!(expected.terminal_observed_at_ms.is_some());
    assert!(expected.interrupt_requested_at_ms.is_some());
    assert_eq!(
        store
            .record_operation_completion(
                &expected.id,
                "original-thread",
                "original-turn",
                TurnTerminalStatus::Interrupted
            )
            .unwrap(),
        expected
    );
}

#[test]
fn receipt_rejects_wrong_identity_conflicting_outcomes_and_unaccepted_dispatch() {
    let store = Store::in_memory().unwrap();
    let op = operation(&store);
    assert!(
        store
            .record_operation_completion(&op.id, "thread", "turn", TurnTerminalStatus::Completed)
            .is_err()
    );
    assert!(
        store
            .claim_turn_interrupt(&op.id, TurnInterruptMode::Strict)
            .is_err()
    );
    store.begin_operation_dispatch(&op.id, "thread").unwrap();
    store.accept_operation(&op.id, "turn").unwrap();
    for (thread, turn) in [("other-thread", "turn"), ("thread", "other-turn")] {
        assert!(
            store
                .record_operation_completion(&op.id, thread, turn, TurnTerminalStatus::Completed)
                .is_err()
        );
    }
    let completed = store
        .record_operation_completion(&op.id, "thread", "turn", TurnTerminalStatus::Completed)
        .unwrap();
    assert!(
        store
            .record_operation_completion(&op.id, "thread", "turn", TurnTerminalStatus::Interrupted)
            .is_err()
    );
    assert_eq!(
        store
            .claim_turn_interrupt(&op.id, TurnInterruptMode::Strict)
            .unwrap()
            .0,
        completed
    );
    assert!(completed.interrupt_requested_at_ms.is_none());
}

#[test]
fn v15_operations_keep_acceptance_but_gain_no_invented_thread_or_outcome() {
    let store = Store::in_memory().unwrap();
    let op = operation(&store);
    store.begin_operation_dispatch(&op.id, "thread").unwrap();
    store.accept_operation(&op.id, "turn").unwrap();
    let connection = store.lock().unwrap();
    connection
        .execute_batch(
            "ALTER TABLE operations DROP COLUMN native_thread_id;
        ALTER TABLE operations DROP COLUMN terminal_status;
        ALTER TABLE operations DROP COLUMN terminal_observed_at_ms;
        ALTER TABLE operations DROP COLUMN interrupt_requested_at_ms;
        PRAGMA user_version=15;",
        )
        .unwrap();
    crate::store::migrations::migrate(&connection).unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        SUPPORTED_SCHEMA_VERSION
    );
    drop(connection);
    let migrated = store
        .operation_by_client_id("receipt-send")
        .unwrap()
        .unwrap();
    assert_eq!(migrated.state, OperationState::Accepted);
    assert_eq!(migrated.native_result_id.as_deref(), Some("turn"));
    assert!(migrated.native_thread_id.is_none());
    assert!(migrated.terminal_status.is_none());
    assert!(migrated.terminal_observed_at_ms.is_none());
    assert!(migrated.interrupt_requested_at_ms.is_none());
}

#[test]
fn close_retains_the_receipt_and_workspace_deletion_removes_its_operation() {
    let store = Store::in_memory().unwrap();
    let op = operation(&store);
    store.begin_operation_dispatch(&op.id, "thread").unwrap();
    store.accept_operation(&op.id, "turn").unwrap();
    let receipt = store
        .record_operation_completion(&op.id, "thread", "turn", TurnTerminalStatus::Completed)
        .unwrap();
    store
        .begin_workspace_close(&op.workspace_id, "head", false)
        .unwrap();
    store
        .transition_workspace_availability(
            &op.workspace_id,
            WorkspaceAvailability::Closing,
            WorkspaceAvailability::Closed,
            None,
        )
        .unwrap();
    assert_eq!(
        store.operation_by_client_id(&op.operation_id).unwrap(),
        Some(receipt)
    );
    store
        .begin_workspace_deletion(&op.workspace_id, WorkspaceDeletionIntent::default(), None)
        .unwrap();
    store.delete_workspace_record(&op.workspace_id).unwrap();
    assert!(
        store
            .operation_by_client_id(&op.operation_id)
            .unwrap()
            .is_none()
    );
}
