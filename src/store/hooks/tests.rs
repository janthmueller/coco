use std::path::Path;

use serde_json::json;

use super::*;
use crate::domain::hooks::{
    HOOK_EVENT_SCHEMA_VERSION, HookEvent, HookRepository, HookTarget, HookWorkspace,
};
use crate::domain::signals::{Signal, SignalType};
use crate::store::tests::{ready_workspace, repository};

fn dispatch(workspace: &crate::domain::Workspace, event_id: &str) -> HookDispatch {
    HookDispatch {
        event: HookEvent {
            schema_version: HOOK_EVENT_SCHEMA_VERSION,
            id: event_id.into(),
            kind: HookEventKind::SignalEmitted,
            occurred_at_ms: 10,
            repository: HookRepository {
                id: workspace.repository_id.clone(),
                name: "fixture".into(),
                path: Path::new("/tmp/hook-repository").into(),
            },
            workspace: HookWorkspace {
                id: workspace.id.clone(),
                name: workspace.name.clone(),
                thread_id: workspace.codex_thread_id.clone(),
                worktree_path: workspace.worktree_path.clone(),
                branch_name: workspace.branch_name.clone(),
            },
            data: json!({
                "signalId": "signal-id",
                "name": "review.requested",
                "version": 1,
                "payload": {"pr": 12}
            }),
        },
        targets: vec![HookTarget {
            hook_id: "review-notify".into(),
            definition_hash: "sha256:definition".into(),
        }],
    }
}

fn prepare() -> (Store, crate::domain::Workspace) {
    let store = Store::in_memory().unwrap();
    let repository = repository(Path::new("/tmp/hook-repository"));
    store.register_repository(&repository).unwrap();
    store
        .register_signal_types(vec![SignalType {
            repository_id: repository.id.clone(),
            name: "review.requested".into(),
            version: 1,
            description: "Request review".into(),
            payload_schema: None,
            registered_at_ms: 1,
        }])
        .unwrap();
    let workspace = ready_workspace(&store, &repository.id, "hook-workspace");
    (store, workspace)
}

fn signal(workspace: &crate::domain::Workspace) -> Signal {
    Signal {
        id: "signal-id".into(),
        sequence: 0,
        repository_id: workspace.repository_id.clone(),
        repository_name: "fixture".into(),
        workspace_id: workspace.id.clone(),
        workspace_name: workspace.name.clone(),
        thread_id: workspace.codex_thread_id.clone().unwrap(),
        name: "review.requested".into(),
        version: 1,
        payload: json!({"pr": 12}),
        idempotency_key: "once".into(),
        occurred_at_ms: 0,
    }
}

#[test]
fn signal_and_matching_delivery_commit_once_then_retry_independently() {
    let (store, workspace) = prepare();
    let emitted = store
        .emit_signal_with_hook(signal(&workspace), Some(dispatch(&workspace, "event-one")))
        .unwrap();
    let replayed = store
        .emit_signal_with_hook(signal(&workspace), Some(dispatch(&workspace, "event-two")))
        .unwrap();
    assert_eq!(replayed, emitted);
    assert_eq!(store.list_hook_deliveries(100).unwrap().len(), 1);

    let first = store.claim_hook_delivery(i64::MAX).unwrap().unwrap();
    assert_eq!(first.summary.attempts, 1);
    let event: HookEvent = serde_json::from_slice(&first.event_body).unwrap();
    assert_eq!(event.id, "event-one");
    assert_eq!(event.data["payload"], json!({"pr": 12}));
    let retry = store
        .complete_hook_delivery(&first.summary.id, 2, Some("temporary\nfailure"))
        .unwrap();
    assert_eq!(retry.state, HookDeliveryState::Pending);
    assert_eq!(retry.last_error.as_deref(), Some("temporary failure"));

    let second = store.claim_hook_delivery(i64::MAX).unwrap().unwrap();
    assert_eq!(second.summary.attempts, 2);
    let completed = store
        .complete_hook_delivery(&second.summary.id, 2, None)
        .unwrap();
    assert_eq!(completed.state, HookDeliveryState::Succeeded);
    assert!(store.claim_hook_delivery(i64::MAX).unwrap().is_none());
}

#[test]
fn restart_requeues_an_owned_delivery_and_stale_definitions_cancel() {
    let (store, workspace) = prepare();
    store
        .emit_signal_with_hook(
            signal(&workspace),
            Some(dispatch(&workspace, "event-recovery")),
        )
        .unwrap();
    let claimed = store.claim_hook_delivery(i64::MAX).unwrap().unwrap();
    assert_eq!(store.recover_hook_deliveries().unwrap(), 1);
    let recovered = store.claim_hook_delivery(i64::MAX).unwrap().unwrap();
    assert_eq!(recovered.summary.id, claimed.summary.id);
    assert_eq!(recovered.summary.attempts, 2);
    let cancelled = store
        .cancel_hook_delivery(&recovered.summary.id, "definition removed")
        .unwrap();
    assert_eq!(cancelled.state, HookDeliveryState::Cancelled);
}

#[test]
fn one_hook_observes_committed_event_order_even_while_retrying() {
    let (store, workspace) = prepare();
    store
        .emit_signal_with_hook(signal(&workspace), Some(dispatch(&workspace, "event-one")))
        .unwrap();
    let mut later_signal = signal(&workspace);
    later_signal.id = "signal-two".into();
    later_signal.idempotency_key = "twice".into();
    store
        .emit_signal_with_hook(later_signal, Some(dispatch(&workspace, "event-two")))
        .unwrap();

    let first = store.claim_hook_delivery(i64::MAX).unwrap().unwrap();
    let first_event: HookEvent = serde_json::from_slice(&first.event_body).unwrap();
    assert_eq!(first_event.id, "event-one");
    assert!(store.claim_hook_delivery(i64::MAX).unwrap().is_none());
    let retry = store
        .complete_hook_delivery(&first.summary.id, 2, Some("retry"))
        .unwrap();
    assert_eq!(retry.state, HookDeliveryState::Pending);
    let retried = store.claim_hook_delivery(i64::MAX).unwrap().unwrap();
    assert_eq!(retried.summary.id, first.summary.id);
    store
        .complete_hook_delivery(&retried.summary.id, 2, None)
        .unwrap();

    let second = store.claim_hook_delivery(i64::MAX).unwrap().unwrap();
    let second_event: HookEvent = serde_json::from_slice(&second.event_body).unwrap();
    assert_eq!(second_event.id, "event-two");
}

#[test]
fn migrates_a_v10_signal_store_without_losing_its_records() {
    let (store, workspace) = prepare();
    store.emit_signal(signal(&workspace)).unwrap();
    let connection = store.lock().unwrap();
    connection
        .execute_batch(
            "DROP TABLE hook_deliveries;
             DROP TABLE hook_events;
             PRAGMA user_version = 10;",
        )
        .unwrap();

    crate::store::migrations::migrate(&connection).unwrap();

    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    let signals: i64 = connection
        .query_row("SELECT count(*) FROM signals", [], |row| row.get(0))
        .unwrap();
    let hook_tables: i64 = connection
        .query_row(
            "SELECT count(*) FROM sqlite_schema
             WHERE type = 'table' AND name IN ('hook_events', 'hook_deliveries')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, 11);
    assert_eq!(signals, 1);
    assert_eq!(hook_tables, 2);
}
