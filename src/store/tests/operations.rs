use super::*;

fn fixture() -> (Store, Operation) {
    let store = Store::in_memory().unwrap();
    let repo = repository(Path::new("/tmp/source-rejected-operation"));
    store.register_repository(&repo).unwrap();
    let workspace = ready_workspace(&store, &repo.id, "rejected-operation");
    let (operation, _) = store
        .prepare_operation(NewOperation {
            operation_id: "rejected-operation".to_owned(),
            workspace_id: workspace.id,
            kind: OperationKind::TurnStart,
            request_fingerprint: "unchanged-fingerprint".to_owned(),
        })
        .unwrap();
    (store, operation)
}

#[test]
fn proven_rejection_restores_exact_intent_and_can_be_explicitly_dispatched_again() {
    let (store, prepared) = fixture();
    store
        .begin_operation_dispatch(&prepared.id, "thread-test")
        .unwrap();
    assert_eq!(
        store.reset_rejected_operation(&prepared.id).unwrap(),
        prepared
    );
    let dispatched = store
        .begin_operation_dispatch(&prepared.id, "thread-test")
        .unwrap();
    assert_eq!(dispatched.state, OperationState::Dispatching);
    let accepted = store
        .accept_operation(&prepared.id, "accepted-after-retry")
        .unwrap();
    assert_eq!(accepted.state, OperationState::Accepted);
    assert_eq!(accepted.request_fingerprint, prepared.request_fingerprint);
}

#[test]
fn rejection_cannot_reset_prepared_uncertain_or_accepted_operations() {
    for state in [
        OperationState::Prepared,
        OperationState::Uncertain,
        OperationState::Accepted,
    ] {
        let (store, prepared) = fixture();
        let retained = match state {
            OperationState::Prepared => prepared,
            OperationState::Uncertain => {
                store
                    .begin_operation_dispatch(&prepared.id, "thread-test")
                    .unwrap();
                store.mark_operation_uncertain(&prepared.id).unwrap()
            }
            OperationState::Accepted => {
                store
                    .begin_operation_dispatch(&prepared.id, "thread-test")
                    .unwrap();
                store
                    .accept_operation(&prepared.id, "already-accepted")
                    .unwrap()
            }
            OperationState::Dispatching => unreachable!(),
        };
        assert!(matches!(
            store.reset_rejected_operation(&retained.id),
            Err(StoreError::InvalidOperationState { .. })
        ));
        assert_eq!(store.operation_by_id(&retained.id).unwrap(), Some(retained));
    }
}
