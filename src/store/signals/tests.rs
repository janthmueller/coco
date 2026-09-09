use super::*;
use crate::domain::signals::SignalFilter;
use crate::domain::{Workspace, WorkspaceAvailability};
use crate::store::{
    WorkspaceDeletionIntent,
    tests::{ready_workspace, repository},
};
use serde_json::json;
use std::path::Path;

fn prepare(store: &Store) -> Workspace {
    let repo = repository(Path::new("/tmp/signal-repository"));
    store.register_repository(&repo).unwrap();
    store
        .register_signal_types(vec![SignalType {
            repository_id: repo.id.clone(),
            name: "review.requested".into(),
            version: 1,
            description: "Please review".into(),
            payload_schema: None,
            registered_at_ms: 1,
        }])
        .unwrap();
    ready_workspace(store, &repo.id, "signals")
}

fn draft(workspace: &Workspace, key: &str) -> Signal {
    Signal {
        id: uuid::Uuid::now_v7().to_string(),
        sequence: 0,
        repository_id: workspace.repository_id.clone(),
        repository_name: "fixture".into(),
        workspace_id: workspace.id.clone(),
        workspace_name: workspace.name.clone(),
        thread_id: workspace.codex_thread_id.clone().unwrap(),
        name: "review.requested".into(),
        version: 1,
        payload: json!({"pr": 12}),
        idempotency_key: key.into(),
        occurred_at_ms: 0,
    }
}

#[test]
fn registration_and_emission_are_immutable_and_idempotent() {
    let store = Store::in_memory().unwrap();
    let workspace = prepare(&store);
    let mut definition = store
        .list_signal_types(&workspace.repository_id)
        .unwrap()
        .remove(0);
    assert_eq!(
        store
            .register_signal_types(vec![definition.clone()])
            .unwrap(),
        vec![definition.clone()]
    );
    definition.description = "changed".into();
    assert!(matches!(
        store.register_signal_types(vec![definition]),
        Err(StoreError::Signal(SignalError::VersionConflict { .. }))
    ));
    let signal = store.emit_signal(draft(&workspace, "once")).unwrap();
    assert_eq!(
        signal,
        store.emit_signal(draft(&workspace, "once")).unwrap()
    );
    let mut conflict = draft(&workspace, "once");
    conflict.payload = json!({"pr": 13});
    assert!(matches!(
        store.emit_signal(conflict),
        Err(StoreError::Signal(SignalError::IdempotencyConflict))
    ));
    assert_eq!(
        store
            .list_signals(&SignalFilter::default(), None, 100)
            .unwrap()
            .signals,
        [signal]
    );
}

#[test]
fn catalog_conflicts_and_capacity_errors_roll_back_every_new_definition() {
    let store = Store::in_memory().unwrap();
    let workspace = prepare(&store);
    let original = store.list_signal_types(&workspace.repository_id).unwrap();
    let mut second = original[0].clone();
    second.version = 2;
    let mut conflict = original[0].clone();
    conflict.payload_schema = Some(json!({"type": "string"}));
    assert!(matches!(
        store.register_signal_types(vec![second.clone(), conflict]),
        Err(StoreError::Signal(SignalError::VersionConflict { .. }))
    ));
    assert_eq!(
        store.list_signal_types(&workspace.repository_id).unwrap(),
        original
    );
    let oversized = (2..=129)
        .map(|version| SignalType {
            version,
            ..second.clone()
        })
        .collect();
    assert!(matches!(
        store.register_signal_types(oversized),
        Err(StoreError::Signal(SignalError::CatalogFull))
    ));
    assert_eq!(
        store.list_signal_types(&workspace.repository_id).unwrap(),
        original
    );
    let added = store.register_signal_types(vec![second]).unwrap();
    let mut reloaded = added.clone();
    reloaded[0].registered_at_ms += 100;
    assert_eq!(store.register_signal_types(reloaded).unwrap(), added);
}

#[test]
fn independent_readers_and_cursor_filters_survive_restart() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("signals.db");
    let store = Store::open(&path).unwrap();
    let workspace = prepare(&store);
    let first = store.emit_signal(draft(&workspace, "first")).unwrap();
    let second = store.emit_signal(draft(&workspace, "second")).unwrap();
    let filter = SignalFilter::default();
    let page = store.list_signals(&filter, None, 1).unwrap();
    assert!(page.has_more);
    assert_eq!(page.signals, [first]);
    assert_eq!(page, store.list_signals(&filter, None, 1).unwrap());
    drop(store);
    let store = Store::open(path).unwrap();
    let next = store
        .list_signals(&filter, Some(&page.next_cursor), 1)
        .unwrap();
    assert_eq!(next.signals.as_slice(), std::slice::from_ref(&second));
    assert!(!next.has_more);
    assert_eq!(
        second,
        store.emit_signal(draft(&workspace, "second")).unwrap()
    );
    let other = SignalFilter {
        name: Some("review.requested".into()),
        ..filter
    };
    assert!(matches!(
        store.list_signals(&other, Some(&page.next_cursor), 1),
        Err(StoreError::Signal(SignalError::InvalidCursor))
    ));
    let unrelated = Store::in_memory().unwrap();
    assert!(matches!(
        unrelated.list_signals(&SignalFilter::default(), Some(&page.next_cursor), 1),
        Err(StoreError::Signal(SignalError::InvalidCursor))
    ));
}

#[test]
fn closing_and_deleting_do_not_erase_signal_history_or_allow_new_emissions() {
    let store = Store::in_memory().unwrap();
    let workspace = prepare(&store);
    let signal = store.emit_signal(draft(&workspace, "first")).unwrap();
    store
        .begin_workspace_close(&workspace.id, "abcdef", false)
        .unwrap();
    store
        .transition_workspace_availability(
            &workspace.id,
            WorkspaceAvailability::Closing,
            WorkspaceAvailability::Closed,
            None,
        )
        .unwrap();
    assert!(matches!(
        store.emit_signal(draft(&workspace, "second")),
        Err(StoreError::Signal(SignalError::InvalidSender))
    ));
    assert_eq!(
        store.emit_signal(draft(&workspace, "first")).unwrap(),
        signal
    );
    store
        .begin_workspace_deletion(
            &workspace.id,
            WorkspaceDeletionIntent {
                delete_thread: false,
                delete_branch: false,
            },
        )
        .unwrap();
    store.delete_workspace_record(&workspace.id).unwrap();
    let filter = SignalFilter {
        workspace_id: Some(workspace.id.clone()),
        ..Default::default()
    };
    assert_eq!(
        store.list_signals(&filter, None, 100).unwrap().signals,
        [signal]
    );
    assert!(matches!(
        store.emit_signal(draft(&workspace, "new")),
        Err(StoreError::Signal(SignalError::InvalidSender))
    ));
}

#[test]
fn retention_reports_expired_cursors_and_does_not_reuse_sequences() {
    let store = Store::in_memory().unwrap();
    let workspace = prepare(&store);
    let filter = SignalFilter::default();
    let empty = store.list_signals(&filter, None, 100).unwrap();
    store.emit_signal(draft(&workspace, "old")).unwrap();
    // Move the high-water mark without generating 10,000 model messages or waiting on rate limits.
    store
        .lock()
        .unwrap()
        .execute(
            "UPDATE signal_stream SET high_water = ?1",
            [SIGNAL_RETENTION],
        )
        .unwrap();
    let newest = store.emit_signal(draft(&workspace, "new")).unwrap();
    assert_eq!(newest.sequence, SIGNAL_RETENTION + 1);
    assert!(matches!(
        store.list_signals(&filter, Some(&empty.next_cursor), 100),
        Err(StoreError::Signal(SignalError::ExpiredCursor))
    ));
    assert_eq!(
        store.list_signals(&filter, None, 100).unwrap().signals,
        [newest]
    );
}

#[test]
fn accepted_retries_do_not_consume_rate_allowance() {
    let store = Store::in_memory().unwrap();
    let workspace = prepare(&store);
    let first = store.emit_signal(draft(&workspace, "0")).unwrap();
    for key in 1..10 {
        store
            .emit_signal(draft(&workspace, &key.to_string()))
            .unwrap();
    }
    assert!(matches!(
        store.emit_signal(draft(&workspace, "overflow")),
        Err(StoreError::Signal(SignalError::RateLimited))
    ));
    assert_eq!(store.emit_signal(draft(&workspace, "0")).unwrap(), first);
}
