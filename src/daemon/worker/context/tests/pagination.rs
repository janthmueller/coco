use std::collections::VecDeque;
use std::future::ready;

use super::*;

#[tokio::test]
async fn requests_bounded_metadata_pages_and_selects_the_latest_terminal_boundary() {
    let mut pages = VecDeque::from([
        json!({"data": [{"id": "active", "status": "inProgress"}], "nextCursor": "second"}),
        json!({"data": [
            {"id": "newest", "status": "interrupted"},
            {"id": "older", "status": "completed"},
        ], "nextCursor": "not-requested"}),
    ]);
    let mut requests = Vec::new();
    let boundary = read_completed_boundary("source", |params| {
        requests.push(params);
        ready(Ok(pages.pop_front().expect("unexpected extra page")))
    })
    .await
    .unwrap();
    assert_eq!(boundary.as_deref(), Some("newest"));
    assert!(pages.is_empty());
    assert_eq!(requests.len(), 2);
    for (request, cursor) in requests.iter().zip([Value::Null, json!("second")]) {
        assert_eq!(
            request,
            &json!({
                "threadId": "source", "limit": TURN_PAGE_LIMIT,
                "sortDirection": "desc", "itemsView": "notLoaded", "cursor": cursor,
            })
        );
    }
}

#[tokio::test]
async fn stops_at_the_last_page_without_inventing_a_completed_turn() {
    let mut calls = 0;
    let boundary = read_completed_boundary("source", |_| {
        calls += 1;
        ready(Ok(json!({"data": [], "nextCursor": null})))
    })
    .await
    .unwrap();
    assert_eq!(boundary, None);
    assert_eq!(calls, 1);
}

#[tokio::test]
async fn page_cap_stops_before_an_extra_request() {
    let mut calls = 0;
    let error = read_completed_boundary("source", |_| {
        calls += 1;
        ready(Ok(
            json!({"data": [], "nextCursor": format!("page-{calls}")}),
        ))
    })
    .await
    .unwrap_err();
    assert_eq!(calls, MAX_TURN_PAGES);
    assert!(error.to_string().contains("exceeded its page limit"));
}

#[tokio::test]
async fn repeated_cursor_cannot_loop_or_issue_a_third_request() {
    let mut calls = 0;
    let error = read_completed_boundary("source", |_| {
        calls += 1;
        ready(Ok(json!({"data": [], "nextCursor": "same-page"})))
    })
    .await
    .unwrap_err();
    assert_eq!(calls, 2);
    assert!(error.to_string().contains("did not advance"));
}

#[tokio::test]
async fn server_error_is_propagated_without_retry_or_a_different_cutoff() {
    let mut calls = 0;
    let error = read_completed_boundary("source", |_| {
        calls += 1;
        ready(Err(WorkerError::runtime(std::io::Error::other(
            "lost page reply",
        ))))
    })
    .await
    .unwrap_err();
    assert_eq!(calls, 1);
    assert!(matches!(error, WorkerError::Runtime(_)));
}

#[tokio::test]
async fn unsupported_page_does_not_fall_back_to_a_following_page() {
    let mut calls = 0;
    let error = read_completed_boundary("source", |_| {
        calls += 1;
        ready(Ok(
            json!({"data": [{"id": "t", "status": "future"}], "nextCursor": "older"}),
        ))
    })
    .await
    .unwrap_err();
    assert_eq!(calls, 1);
    assert!(matches!(error, WorkerError::InvalidThreadRead(_)));
}
