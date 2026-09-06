#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, DuplexStream};
use tokio::time::timeout;
use tokio_tungstenite::accept_hdr_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{
    Callback, ErrorResponse, Request, Response,
};

use super::websocket::{
    authenticated_request, bridge_jsonl_websocket, prepare_shared_runtime, write_private_file,
};
use super::{
    CodexClient, CodexError, CodexEvent, STDERR_TAIL_BYTES, SharedAppServerOptions, StderrTail,
};

#[cfg(unix)]
mod real_git_approval;

async fn client_pair(
    max_message_bytes: usize,
) -> (
    CodexClient,
    tokio::sync::mpsc::Receiver<CodexEvent>,
    DuplexStream,
) {
    let (client_stream, server_stream) = tokio::io::duplex(64 * 1024);
    let (reader, writer) = tokio::io::split(client_stream);
    let stderr = Arc::new(tokio::sync::Mutex::new(StderrTail::new(STDERR_TAIL_BYTES)));
    let (client, events, _shutdown) =
        CodexClient::from_io(reader, writer, max_message_bytes, 8, stderr).await;
    (client, events, server_stream)
}

struct AssertAuthorization;

impl Callback for AssertAuthorization {
    fn on_request(self, request: &Request, response: Response) -> Result<Response, ErrorResponse> {
        assert_eq!(
            request
                .headers()
                .get("authorization")
                .and_then(|value| value.to_str().ok()),
            Some("Bearer test-capability")
        );
        Ok(response)
    }
}

#[tokio::test]
async fn bridges_jsonl_over_an_authenticated_websocket() {
    let (client_transport, server_transport) = tokio::io::duplex(64 * 1024);
    let server = tokio::spawn(async move {
        let mut websocket = accept_hdr_async(server_transport, AssertAuthorization)
            .await
            .unwrap();
        assert_eq!(
            websocket.next().await.unwrap().unwrap(),
            Message::Text(r#"{"method":"health","params":{}}"#.into())
        );
        websocket
            .send(Message::Text(
                r#"{"id":"coco-1","result":{"status":"ok"}}"#.into(),
            ))
            .await
            .unwrap();
    });
    let request = authenticated_request("ws://127.0.0.1:40123", "test-capability").unwrap();
    let (websocket, _) = tokio_tungstenite::client_async(request, client_transport)
        .await
        .unwrap();
    let (mut json_client, bridge_io) = tokio::io::duplex(64 * 1024);
    let stderr = Arc::new(tokio::sync::Mutex::new(StderrTail::new(STDERR_TAIL_BYTES)));
    let bridge = tokio::spawn(bridge_jsonl_websocket(bridge_io, websocket, stderr));

    json_client
        .write_all(b"{\"method\":\"health\",\"params\":{}}\n")
        .await
        .unwrap();
    let mut response = String::new();
    BufReader::new(&mut json_client)
        .read_line(&mut response)
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&response).unwrap(),
        json!({"id": "coco-1", "result": {"status": "ok"}})
    );
    server.await.unwrap();
    bridge.abort();
    let _ = bridge.await;
}

#[tokio::test]
async fn rejects_overlapping_shared_runtime_files() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("shared");
    let error = prepare_shared_runtime(&SharedAppServerOptions {
        endpoint_path: path.clone(),
        token_path: path,
    })
    .await
    .unwrap_err();

    assert!(matches!(error, CodexError::InvalidOptions(_)));
}

#[cfg(unix)]
#[test]
fn writes_capability_files_with_owner_only_permissions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("token");
    write_private_file(&path, b"secret").unwrap();

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "secret");
    assert_eq!(
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[tokio::test]
async fn decodes_a_response_split_across_arbitrary_chunks() {
    let (client, _events, server) = client_pair(4096).await;
    let (server_reader, mut server_writer) = tokio::io::split(server);
    let mut server_reader = BufReader::new(server_reader);

    let request = tokio::spawn({
        let client = client.clone();
        async move { client.request("thread/start", json!({"cwd": "/tmp"})).await }
    });

    let mut line = String::new();
    server_reader.read_line(&mut line).await.unwrap();
    let sent: Value = serde_json::from_str(&line).unwrap();
    let response = serde_json::to_vec(&json!({
        "id": sent["id"],
        "result": {"thread": {"id": "thr_1"}}
    }))
    .unwrap();
    for chunk in response.chunks(3) {
        server_writer.write_all(chunk).await.unwrap();
        tokio::task::yield_now().await;
    }
    server_writer.write_all(b"\n").await.unwrap();

    assert_eq!(
        request.await.unwrap().unwrap(),
        json!({"thread": {"id": "thr_1"}})
    );
    drop(server_writer);
    drop(server_reader);
    client.close().await.unwrap();
}

#[tokio::test]
async fn correlates_out_of_order_responses() {
    let (client, _events, server) = client_pair(4096).await;
    let (server_reader, mut server_writer) = tokio::io::split(server);
    let mut server_reader = BufReader::new(server_reader);

    let first = tokio::spawn({
        let client = client.clone();
        async move { client.request("first", json!({})).await }
    });
    let second = tokio::spawn({
        let client = client.clone();
        async move { client.request("second", json!({})).await }
    });

    let mut requests = Vec::new();
    for _ in 0..2 {
        let mut line = String::new();
        server_reader.read_line(&mut line).await.unwrap();
        requests.push(serde_json::from_str::<Value>(&line).unwrap());
    }
    let first_wire = requests
        .iter()
        .find(|request| request["method"] == "first")
        .unwrap();
    let second_wire = requests
        .iter()
        .find(|request| request["method"] == "second")
        .unwrap();
    server_writer
        .write_all(
            format!(
                "{}\n{}\n",
                json!({"id": second_wire["id"], "result": "second result"}),
                json!({"id": first_wire["id"], "result": "first result"}),
            )
            .as_bytes(),
        )
        .await
        .unwrap();

    assert_eq!(first.await.unwrap().unwrap(), json!("first result"));
    assert_eq!(second.await.unwrap().unwrap(), json!("second result"));
    drop(server_writer);
    drop(server_reader);
    client.close().await.unwrap();
}

#[tokio::test]
async fn emits_server_requests_without_automatically_answering_them() {
    let (client, mut events, server) = client_pair(4096).await;
    let (mut server_reader, mut server_writer) = tokio::io::split(server);
    server_writer
        .write_all(
            b"{\"id\":17,\"method\":\"item/commandExecution/requestApproval\",\"params\":{\"reason\":\"network\"}}\n",
        )
        .await
        .unwrap();

    assert_eq!(
        events.recv().await,
        Some(CodexEvent::ServerRequest {
            id: json!(17),
            method: "item/commandExecution/requestApproval".to_owned(),
            params: json!({"reason": "network"}),
        })
    );

    let mut byte = [0_u8; 1];
    assert!(
        timeout(Duration::from_millis(50), server_reader.read(&mut byte))
            .await
            .is_err(),
        "a server request must remain unanswered until respond is called"
    );

    client
        .respond(json!(17), json!({"decision": "decline"}))
        .await
        .unwrap();
    let read = timeout(Duration::from_secs(1), server_reader.read(&mut byte))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(read, 1);
    drop(server_writer);
    drop(server_reader);
    client.close().await.unwrap();
}

#[tokio::test]
async fn preserves_structured_rpc_errors_without_closing_the_connection() {
    let (client, _events, server) = client_pair(4096).await;
    let (server_reader, mut server_writer) = tokio::io::split(server);
    let mut server_reader = BufReader::new(server_reader);

    let request = tokio::spawn({
        let client = client.clone();
        async move { client.request("thread/start", json!({})).await }
    });
    let mut line = String::new();
    server_reader.read_line(&mut line).await.unwrap();
    let sent: Value = serde_json::from_str(&line).unwrap();
    server_writer
        .write_all(
            format!(
                "{}\n",
                json!({
                    "id": sent["id"],
                    "error": {"code": -32000, "message": "denied", "data": {"retry": false}}
                })
            )
            .as_bytes(),
        )
        .await
        .unwrap();

    assert_eq!(
        request.await.unwrap().unwrap_err(),
        CodexError::Rpc {
            code: -32000,
            message: "denied".to_owned(),
            data: Some(json!({"retry": false})),
        }
    );
    drop(server_writer);
    drop(server_reader);
    client.close().await.unwrap();
}
