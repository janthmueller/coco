use super::super::writer::{FRAME_WRITE_TIMEOUT, OUTBOUND_FRAME_BUFFER};
use super::*;

mod support;
mod websocket;
use support::{TEST_TIMEOUT, failing_client, first_byte, large_payload, next_frame};

#[tokio::test]
async fn cancelled_partial_request_finishes_its_frame_and_preserves_correlation() {
    let (client, _events, server) = client_pair(256 * 1024).await;
    let (reader, mut writer) = tokio::io::split(server);
    let mut reader = BufReader::new(reader);
    let request = tokio::spawn({
        let client = client.clone();
        async move { client.request("large", large_payload()).await }
    });
    let prefix = first_byte(&mut reader).await;
    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());
    assert!(client.inner.state.lock().await.pending.is_empty());

    let current = tokio::spawn({
        let client = client.clone();
        async move { client.request("health", json!({})).await }
    });
    let first = next_frame(&mut reader, Some(prefix)).await;
    assert_eq!(first["method"], "large");
    assert_eq!(first["params"], large_payload());
    let second = next_frame(&mut reader, None).await;
    assert_eq!(second["method"], "health");
    writer
        .write_all(
            format!(
                "{}\n{}\n",
                json!({"id": first["id"], "result": "late"}),
                json!({"id": second["id"], "result": "healthy"}),
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    assert_eq!(
        timeout(TEST_TIMEOUT, current)
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
        json!("healthy")
    );
    assert!(client.inner.state.lock().await.pending.is_empty());
    client.close().await.unwrap();
}

#[tokio::test]
async fn cancelled_partial_notification_finishes_before_the_next_frame() {
    let (client, _events, server) = client_pair(256 * 1024).await;
    let mut reader = BufReader::new(server);
    let notification = tokio::spawn({
        let client = client.clone();
        async move { client.notify("large", Some(large_payload())).await }
    });
    let prefix = first_byte(&mut reader).await;
    notification.abort();
    assert!(notification.await.unwrap_err().is_cancelled());
    let current = tokio::spawn({
        let client = client.clone();
        async move { client.notify("health", None).await }
    });
    let first = next_frame(&mut reader, Some(prefix)).await;
    assert_eq!(first, json!({"method": "large", "params": large_payload()}));
    assert_eq!(
        next_frame(&mut reader, None).await,
        json!({"method": "health"})
    );
    timeout(TEST_TIMEOUT, current)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    client.close().await.unwrap();
}

#[tokio::test]
async fn cancelled_partial_server_response_finishes_before_the_next_frame() {
    let (client, _events, server) = client_pair(256 * 1024).await;
    let mut reader = BufReader::new(server);
    let response = tokio::spawn({
        let client = client.clone();
        async move { client.respond(json!(42), large_payload()).await }
    });
    let prefix = first_byte(&mut reader).await;
    response.abort();
    assert!(response.await.unwrap_err().is_cancelled());
    let current = tokio::spawn({
        let client = client.clone();
        async move { client.notify("health", None).await }
    });
    assert_eq!(
        next_frame(&mut reader, Some(prefix)).await,
        json!({"id": 42, "result": large_payload()})
    );
    assert_eq!(
        next_frame(&mut reader, None).await,
        json!({"method": "health"})
    );
    timeout(TEST_TIMEOUT, current)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    client.close().await.unwrap();
}

#[tokio::test]
async fn cancelled_queued_request_is_skipped_without_writing_bytes() {
    let (client, _events, server) = client_pair(256 * 1024).await;
    let mut reader = BufReader::new(server);
    let first = tokio::spawn({
        let client = client.clone();
        async move { client.notify("large", Some(large_payload())).await }
    });
    let prefix = first_byte(&mut reader).await;
    {
        let queued = client.request("cancelled", json!({}));
        tokio::pin!(queued);
        assert!(futures_util::poll!(&mut queued).is_pending());
        assert_eq!(client.inner.outbound.capacity(), OUTBOUND_FRAME_BUFFER - 1);
    }
    assert!(client.inner.state.lock().await.pending.is_empty());
    let current = tokio::spawn({
        let client = client.clone();
        async move { client.notify("health", None).await }
    });
    assert_eq!(
        next_frame(&mut reader, Some(prefix)).await["method"],
        "large"
    );
    assert_eq!(next_frame(&mut reader, None).await["method"], "health");
    timeout(TEST_TIMEOUT, first)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    timeout(TEST_TIMEOUT, current)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    client.close().await.unwrap();
}

#[tokio::test]
async fn cancelled_request_waiting_for_queue_capacity_is_not_enqueued() {
    let (client, _events, server) = client_pair(256 * 1024).await;
    let mut reader = BufReader::new(server);
    let first = tokio::spawn({
        let client = client.clone();
        async move { client.notify("large", Some(large_payload())).await }
    });
    let prefix = first_byte(&mut reader).await;
    let mut queued = Vec::new();
    for index in 0..OUTBOUND_FRAME_BUFFER {
        let mut notification = Box::pin(client.notify(format!("queued-{index}"), None));
        assert!(futures_util::poll!(&mut notification).is_pending());
        queued.push(notification);
    }
    assert_eq!(client.inner.outbound.capacity(), 0);
    {
        let blocked = client.request("cancelled", json!({}));
        tokio::pin!(blocked);
        assert!(futures_util::poll!(&mut blocked).is_pending());
    }
    assert!(client.inner.state.lock().await.pending.is_empty());
    assert_eq!(
        next_frame(&mut reader, Some(prefix)).await["method"],
        "large"
    );
    for index in 0..OUTBOUND_FRAME_BUFFER {
        assert_eq!(
            next_frame(&mut reader, None).await["method"],
            format!("queued-{index}")
        );
    }
    timeout(TEST_TIMEOUT, first)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    for notification in queued {
        timeout(TEST_TIMEOUT, notification).await.unwrap().unwrap();
    }
    client.close().await.unwrap();
}

#[tokio::test]
async fn close_interrupts_a_blocked_writer_and_rejects_queued_requests() {
    let (client, _events, server) = client_pair(256 * 1024).await;
    let mut reader = BufReader::new(server);
    let first = tokio::spawn({
        let client = client.clone();
        async move { client.request("large", large_payload()).await }
    });
    first_byte(&mut reader).await;
    let queued = client.request("queued", json!({}));
    tokio::pin!(queued);
    assert!(futures_util::poll!(&mut queued).is_pending());

    timeout(TEST_TIMEOUT, client.close())
        .await
        .expect("close waited for the blocked frame's write deadline")
        .unwrap();
    let error = timeout(TEST_TIMEOUT, first)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(matches!(&error, CodexError::Closed { reason, .. } if reason == "closed by client"));
    assert_eq!(
        timeout(TEST_TIMEOUT, queued).await.unwrap().unwrap_err(),
        error
    );
    assert_eq!(client.notify("after-close", None).await.unwrap_err(), error);
    assert!(client.inner.state.lock().await.pending.is_empty());
    let mut remainder = Vec::new();
    timeout(TEST_TIMEOUT, reader.read_to_end(&mut remainder))
        .await
        .unwrap()
        .unwrap();
    assert!(
        !remainder.contains(&b'\n'),
        "close wrote a queued frame after a truncated one"
    );
}

#[tokio::test]
async fn partial_write_failure_is_terminal_before_another_frame_is_attempted() {
    let (client, _events, started, fail, evidence, _reader_peer) = failing_client().await;
    let first = tokio::spawn({
        let client = client.clone();
        async move { client.request("first", json!({})).await }
    });
    timeout(TEST_TIMEOUT, started).await.unwrap().unwrap();
    let second = client.request("second", json!({}));
    tokio::pin!(second);
    assert!(futures_util::poll!(&mut second).is_pending());
    fail.send(()).unwrap();

    let error = timeout(TEST_TIMEOUT, first)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(matches!(
        &error,
        CodexError::Io {
            operation: "writing stdin",
            ..
        }
    ));
    assert_eq!(
        timeout(TEST_TIMEOUT, second).await.unwrap().unwrap_err(),
        error
    );
    assert_eq!(client.notify("after-error", None).await.unwrap_err(), error);
    assert!(client.inner.state.lock().await.pending.is_empty());
    {
        let evidence = evidence.lock().unwrap();
        assert_eq!(evidence.prefix.len(), 8);
        assert_eq!(evidence.writes_after_failure, 0);
    }
    client.close().await.unwrap();
}

#[tokio::test]
async fn close_stops_a_blocked_writer_even_when_a_failure_was_already_recorded() {
    let (client, _events, server) = client_pair(256 * 1024).await;
    let mut reader = BufReader::new(server);
    let first = tokio::spawn({
        let client = client.clone();
        async move { client.request("large", large_payload()).await }
    });
    first_byte(&mut reader).await;
    let original = CodexError::Closed {
        reason: "process monitor already observed an exit".to_owned(),
        stderr: "test stderr".to_owned(),
    };
    client.inner.fail(original.clone(), false).await;
    timeout(TEST_TIMEOUT, client.close())
        .await
        .expect("an existing failure prevented writer shutdown")
        .unwrap();
    assert_eq!(
        timeout(TEST_TIMEOUT, first)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err(),
        original
    );
    assert_eq!(
        client.notify("after-close", None).await.unwrap_err(),
        original
    );
    assert!(client.inner.state.lock().await.pending.is_empty());
}

#[tokio::test]
async fn stalled_writer_expires_and_rejects_all_remaining_frames() {
    let (client, _events, server) = client_pair(256 * 1024).await;
    let mut reader = BufReader::new(server);
    let first = tokio::spawn({
        let client = client.clone();
        async move { client.request("large", large_payload()).await }
    });
    first_byte(&mut reader).await;
    let queued = client.request("queued", json!({}));
    tokio::pin!(queued);
    assert!(futures_util::poll!(&mut queued).is_pending());
    let error = timeout(FRAME_WRITE_TIMEOUT + TEST_TIMEOUT, first)
        .await
        .expect("stalled frame was not bounded by its write deadline")
        .unwrap()
        .unwrap_err();
    assert!(matches!(
        &error,
        CodexError::Io { operation: "writing stdin", message, .. }
            if message == "outbound frame write timed out after 15 seconds"
    ));
    assert_eq!(
        timeout(TEST_TIMEOUT, queued).await.unwrap().unwrap_err(),
        error
    );
    assert_eq!(
        client.notify("after-timeout", None).await.unwrap_err(),
        error
    );
    assert!(client.inner.state.lock().await.pending.is_empty());
    client.close().await.unwrap();
}
