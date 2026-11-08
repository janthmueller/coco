use super::*;
use serde_json::json;

mod transport;

fn guard(original: Option<&str>, enabled: bool) -> CleanupGuard {
    CleanupGuard::new(ThreadBinding {
        thread_id: original.map(str::to_owned),
        temporary: enabled,
    })
}

#[test]
fn original_conversation_and_catalog_requests_do_not_disarm_cleanup() {
    let mut guard = guard(Some("original"), true);
    for method in ["initialize", "thread/list", "thread/loaded/list"] {
        assert!(!guard.uses_additional_conversation(&json!({"method": method}), None));
    }
    for method in [
        "thread/read",
        "thread/resume",
        "turn/start",
        "turn/steer",
        "review/start",
    ] {
        assert!(!guard.uses_additional_conversation(
            &json!({"method": method, "params": {"threadId": "original"}}),
            None
        ));
    }
}

#[test]
fn starting_forking_or_using_another_conversation_requires_durable_disarm() {
    for method in ["thread/start", "thread/fork", "thread/resume", "turn/start"] {
        let mut guard = guard(Some("original"), true);
        assert!(guard.uses_additional_conversation(
            &json!({"id": 1, "method": method, "params": {"threadId": "other"}}),
            None
        ));
    }
    assert!(guard(Some("original"), true).uses_additional_conversation(
        &json!({"id": 1, "method": "thread/fork", "params": {"threadId": "original"}}),
        None
    ));
}

#[test]
fn fresh_first_start_is_allowed_but_another_start_is_not_even_after_reconnect() {
    let mut guard = guard(None, true);
    let start = json!({"id": 1, "method": "thread/start", "params": {}});
    assert!(!guard.uses_additional_conversation(&start, None));
    // An unanswered request's ID may be reused in a new transport generation.
    // The guard never mistakes its retry for a known-safe first creation.
    assert!(guard.uses_additional_conversation(&start, None));
    assert!(!guard.uses_additional_conversation(
        &json!({"method": "turn/start", "params": {"threadId": "first"}}),
        Some("first")
    ));
    assert!(guard.uses_additional_conversation(
        &json!({"method": "turn/start", "params": {"threadId": "other"}}),
        Some("first")
    ));
}

#[test]
fn missing_first_start_identity_is_not_cleanup_authority() {
    let mut guard = guard(None, true);
    assert!(guard.uses_additional_conversation(&json!({"method": "thread/start"}), None));
    assert!(guard.uses_additional_conversation(
        &json!({"method": "thread/resume", "params": {"threadId": "unknown"}}),
        None
    ));
}

#[test]
fn passive_lookup_of_another_threads_history_keeps_cleanup_armed() {
    for original in [None, Some("original")] {
        for (method, extra) in [
            ("thread/read", json!({"includeTurns": false})),
            ("thread/read", json!({"includeTurns": true})),
            ("thread/turns/list", json!({"itemsView": "notLoaded"})),
            ("thread/turns/list", json!({"itemsView": "full"})),
            ("thread/items/list", json!({"turnId": "turn"})),
        ] {
            let mut guard = guard(original, true);
            let mut params = extra;
            params["threadId"] = json!("context-source");
            assert!(!guard.uses_additional_conversation(
                &json!({"id": 1, "method": method, "params": params}),
                original
            ));
            // Reading history never authorizes running that conversation.
            assert!(guard.uses_additional_conversation(
                &json!({
                    "id": 2, "method": "thread/resume", "params": {"threadId": "context-source"}
                }),
                original
            ));
        }
    }
    assert!(guard(Some("original"), true).uses_additional_conversation(
        &json!({"id": 1, "method": "thread/unknown", "params": {"threadId": "other"}}),
        None
    ));
}
