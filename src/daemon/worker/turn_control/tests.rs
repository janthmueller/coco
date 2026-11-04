use super::*;
use std::collections::VecDeque;

#[tokio::test]
async fn native_best_effort_uses_only_stored_ids_and_keeps_rejections_distinct_from_unknown_errors()
{
    let mut frames = Vec::new();
    native_interrupt("original-thread", "original-turn", |params| {
        frames.push(params);
        std::future::ready(Ok(json!({})))
    })
    .await
    .unwrap();
    assert_eq!(
        frames,
        [json!({"threadId": "original-thread", "turnId": "original-turn"})]
    );
    for message in [
        "no active turn to interrupt",
        "expected active turn id old but found newer",
    ] {
        let error = native_interrupt("thread", "old", |_| {
            std::future::ready(Err(CodexError::Rpc {
                code: -32600,
                message: message.into(),
                data: None,
            }))
        })
        .await
        .unwrap_err();
        assert!(matches!(error, WorkerError::TurnInterruptRejected));
    }
    for (code, message) in [
        (-32603, "no active turn to interrupt"),
        (-32600, "unclassified upstream error"),
    ] {
        assert!(matches!(
            native_interrupt("thread", "old", |_| std::future::ready(Err(
                CodexError::Rpc {
                    code,
                    message: message.into(),
                    data: None,
                }
            )))
            .await,
            Err(WorkerError::Runtime(_))
        ));
    }
    assert!(matches!(
        native_interrupt("thread", "turn", |_| std::future::ready(Ok(Value::Null))).await,
        Err(WorkerError::InvalidResponse(_))
    ));
}

#[tokio::test]
async fn empty_native_ids_cannot_become_a_startup_or_current_turn_interrupt() {
    for (thread, turn) in [("thread", ""), ("", "turn")] {
        let mut called = false;
        let result = native_interrupt(thread, turn, |_| {
            called = true;
            std::future::ready(Ok(json!({})))
        })
        .await;
        assert!(result.is_err());
        assert!(!called);
    }
    assert!(supports_native_interrupt(Some("0.166.0")));
    for version in [None, Some("0.166.1"), Some("0.167.0"), Some("custom")] {
        assert!(!supports_native_interrupt(version));
    }
}

#[tokio::test]
async fn legacy_missing_and_unknown_modes_never_issue_a_turn_history_request() {
    for mode in [json!("legacy"), json!(null), json!("future-mode")] {
        let mut methods = Vec::new();
        let observed = read_indexed_outcome("thread", "turn", |method, params| {
            methods.push(method);
            assert_eq!(params, json!({"threadId": "thread", "includeTurns": false}));
            std::future::ready(Ok(json!({"thread": {"id": "thread", "historyMode": mode}})))
        })
        .await
        .unwrap();
        assert_eq!(observed, NativeTurnOutcome::HistoryUnavailable);
        assert_eq!(methods, ["thread/read"]);
    }
    let missing = read_indexed_outcome("thread", "turn", |method, _| {
        assert_eq!(method, "thread/read");
        std::future::ready(Ok(json!({"thread": {"id": "thread"}})))
    })
    .await
    .unwrap();
    assert_eq!(missing, NativeTurnOutcome::HistoryUnavailable);
}

#[tokio::test]
async fn indexed_lookup_requires_an_exact_preflight_identity() {
    for metadata in [
        json!({"thread": {"id": "wrong", "historyMode": "paginated"}}),
        json!({"thread": {"historyMode": "paginated"}}),
    ] {
        let mut calls = 0;
        assert!(
            read_indexed_outcome("thread", "turn", |method, _| {
                calls += 1;
                assert_eq!(method, "thread/read");
                std::future::ready(Ok(metadata.clone()))
            })
            .await
            .is_err()
        );
        assert_eq!(calls, 1);
    }
}

#[tokio::test]
async fn paginated_preflight_is_followed_only_by_item_free_turn_pages() {
    let mut methods = Vec::new();
    let observed = read_indexed_outcome("thread", "wanted", |method, params| {
        methods.push(method);
        let response = if method == "thread/read" {
            json!({"thread": {"id": "thread", "historyMode": "paginated"}})
        } else {
            assert_eq!(params["itemsView"], "notLoaded");
            json!({"data": [{"id": "wanted", "status": "completed"}], "nextCursor": null})
        };
        std::future::ready(Ok(response))
    })
    .await
    .unwrap();
    assert_eq!(
        observed,
        NativeTurnOutcome::Finished(TurnTerminalStatus::Completed)
    );
    assert_eq!(methods, ["thread/read", "thread/turns/list"]);
}

#[test]
fn indexed_lookup_fails_closed_for_missing_old_new_or_custom_versions() {
    assert!(supports_indexed_lookup(Some("0.166.0")));
    for version in [
        None,
        Some("0.160.1"),
        Some("0.167.0"),
        Some("0.166.0-custom"),
        Some(""),
    ] {
        assert!(!supports_indexed_lookup(version));
    }
}

#[tokio::test]
async fn native_worker_never_sends_unsafe_interrupts_or_unverified_history_reads() {
    let (client, _events, mut peer) = crate::codex::CodexClient::test_io_pair(4096).await;
    let worker = CodexWorker::new(client, None);
    assert!(!crate::coordinator::WorkerRuntime::supports_exact_turn_interrupt(&worker));
    assert_eq!(
        worker.lookup_turn_outcome("thread", "turn").await.unwrap(),
        NativeTurnOutcome::HistoryUnavailable
    );
    assert!(matches!(
        crate::coordinator::WorkerRuntime::interrupt_turn(&worker, "thread", "turn").await,
        Err(WorkerError::ExactInterruptUnsupported)
    ));
    use tokio::io::AsyncReadExt;
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(20), peer.read_u8())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn exact_lookup_pages_without_items_and_ignores_other_turns() {
    let mut pages = VecDeque::from([
        json!({"data": [{"id": "new", "status": "inProgress"}], "nextCursor": "page-2"}),
        json!({"data": [{"id": "wanted", "status": "failed"}], "nextCursor": null}),
    ]);
    let mut calls = Vec::new();
    let result = read_outcome("thread", "wanted", |params| {
        calls.push(params);
        std::future::ready(Ok(pages.pop_front().unwrap()))
    })
    .await
    .unwrap();
    assert_eq!(
        result,
        NativeTurnOutcome::Finished(TurnTerminalStatus::Failed)
    );
    assert_eq!(calls.len(), 2);
    assert!(calls.iter().all(|call| call["threadId"] == "thread"
        && call["itemsView"] == "notLoaded"
        && call["limit"] == 50));
    assert_eq!(calls[1]["cursor"], "page-2");
}

#[tokio::test]
async fn in_progress_absence_limit_and_malformed_metadata_never_invent_completion() {
    for (response, expected) in [
        (
            json!({"data": [], "nextCursor": null}),
            NativeTurnOutcome::Unavailable,
        ),
        (
            json!({"data": [{"id": "wanted", "status": "inProgress"}], "nextCursor": null}),
            NativeTurnOutcome::InProgress,
        ),
    ] {
        assert_eq!(
            read_outcome("thread", "wanted", |_| std::future::ready(Ok(
                response.clone()
            )))
            .await
            .unwrap(),
            expected
        );
    }
    for response in [
        json!({"data": [{"id": "wanted", "status": "futureState"}], "nextCursor": null}),
        json!({"data": [{"id": "wanted"}], "nextCursor": null}),
    ] {
        assert!(
            read_outcome("thread", "wanted", |_| std::future::ready(Ok(
                response.clone()
            )))
            .await
            .is_err()
        );
    }
    let mut count = 0;
    let limited = read_outcome("thread", "wanted", |_| {
        count += 1;
        std::future::ready(Ok(
            json!({"data": [], "nextCursor": format!("cursor-{count}")}),
        ))
    })
    .await
    .unwrap();
    assert_eq!(limited, NativeTurnOutcome::Unavailable);
    assert_eq!(count, MAX_PAGES);
    assert!(
        read_outcome("thread", "wanted", |_| std::future::ready(Ok(
            json!({"data": [], "nextCursor": "repeated"})
        )))
        .await
        .is_err()
    );
}
