use super::super::*;
use super::support::TEST_TIMEOUT;

#[tokio::test]
async fn bridge_closes_without_forwarding_an_unterminated_frame() {
    let (client_transport, server_transport) = tokio::io::duplex(64 * 1024);
    let server = tokio::spawn(async move {
        let mut websocket = accept_hdr_async(server_transport, AssertAuthorization)
            .await
            .unwrap();
        assert_eq!(
            websocket.next().await.unwrap().unwrap(),
            Message::Text(r#"{"method":"health"}"#.into())
        );
        assert!(matches!(
            websocket.next().await.unwrap().unwrap(),
            Message::Close(_)
        ));
    });
    let request = authenticated_request("ws://127.0.0.1:40123", "test-capability").unwrap();
    let (websocket, _) = tokio_tungstenite::client_async(request, client_transport)
        .await
        .unwrap();
    let (mut json_client, bridge_io) = tokio::io::duplex(64 * 1024);
    let stderr = Arc::new(tokio::sync::Mutex::new(StderrTail::new(STDERR_TAIL_BYTES)));
    let bridge = tokio::spawn(bridge_jsonl_websocket(
        bridge_io,
        websocket,
        Arc::clone(&stderr),
    ));
    json_client
        .write_all(b"{\"method\":\"health\"}\n{\"method\":\"truncated")
        .await
        .unwrap();
    json_client.shutdown().await.unwrap();
    timeout(TEST_TIMEOUT, server).await.unwrap().unwrap();
    timeout(TEST_TIMEOUT, bridge).await.unwrap().unwrap();
    assert!(
        stderr
            .lock()
            .await
            .display()
            .contains("ended before its newline")
    );
}

#[tokio::test]
async fn close_bounds_a_bridge_that_cannot_finish_its_handshake() {
    let (client, _events, _server) = client_pair(4096).await;
    let bridge = tokio::spawn(std::future::pending::<()>());
    let bridge_abort = bridge.abort_handle();
    client.inner.tasks.lock().await.bridge = Some(bridge);
    timeout(TEST_TIMEOUT, client.close())
        .await
        .expect("bridge shutdown was not bounded")
        .unwrap();
    assert!(bridge_abort.is_finished());
    assert!(matches!(
        client.notify("closed", None).await,
        Err(CodexError::Closed { .. })
    ));
}

#[tokio::test]
async fn client_close_shuts_down_the_split_write_half_and_sends_websocket_close() {
    let (client_transport, server_transport) = tokio::io::duplex(64 * 1024);
    let server = tokio::spawn(async move {
        let mut websocket = accept_hdr_async(server_transport, AssertAuthorization)
            .await
            .unwrap();
        assert!(matches!(
            websocket.next().await.unwrap().unwrap(),
            Message::Close(_)
        ));
    });
    let request = authenticated_request("ws://127.0.0.1:40123", "test-capability").unwrap();
    let (websocket, _) = tokio_tungstenite::client_async(request, client_transport)
        .await
        .unwrap();
    let (json_client, bridge_io) = tokio::io::duplex(64 * 1024);
    let (reader, writer) = tokio::io::split(json_client);
    let stderr = Arc::new(tokio::sync::Mutex::new(StderrTail::new(STDERR_TAIL_BYTES)));
    let (client, _events, _) =
        CodexClient::from_io(reader, writer, 4096, 8, Arc::clone(&stderr)).await;
    client.inner.tasks.lock().await.bridge = Some(tokio::spawn(bridge_jsonl_websocket(
        bridge_io, websocket, stderr,
    )));
    timeout(TEST_TIMEOUT, client.close())
        .await
        .unwrap()
        .unwrap();
    timeout(TEST_TIMEOUT, server).await.unwrap().unwrap();
}
