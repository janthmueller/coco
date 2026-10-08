use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{Context, Result, ensure};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio::time::timeout;

use super::{COMPATIBILITY_TIMEOUT, TestPaths, fs};

pub(super) struct LocalProvider {
    requests: Arc<AtomicUsize>,
    task: JoinHandle<Result<()>>,
}

impl LocalProvider {
    pub(super) async fn start(paths: &TestPaths) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        fs::write(
            paths.codex_home.join("capture-proof.config.toml"),
            format!(
                "model_provider = \"capture-proof\"\n[model_providers.capture-proof]\n\
             name = \"Local context proof\"\nbase_url = \"http://{address}/v1\"\n\
             wire_api = \"responses\"\nrequires_openai_auth = false\nsupports_websockets = false\n"
            ),
        )?;
        let requests = Arc::new(AtomicUsize::new(0));
        let count = requests.clone();
        let task = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await?;
                count.fetch_add(1, Ordering::SeqCst);
                timeout(COMPATIBILITY_TIMEOUT, respond(stream)).await??;
            }
        });
        Ok(Self { requests, task })
    }

    pub(super) fn request_count(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }
}

impl Drop for LocalProvider {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn respond(mut stream: TcpStream) -> Result<()> {
    let mut request = Vec::new();
    let body_start = loop {
        let mut bytes = [0; 4096];
        let length = stream.read(&mut bytes).await?;
        ensure!(
            length != 0 && request.len() < 2 * 1024 * 1024,
            "invalid test provider request"
        );
        request.extend_from_slice(&bytes[..length]);
        if let Some(offset) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
            break offset + 4;
        }
    };
    let headers = String::from_utf8_lossy(&request[..body_start]).to_ascii_lowercase();
    ensure!(
        headers.starts_with("post /v1/responses "),
        "unexpected local model endpoint"
    );
    let length: usize = headers
        .lines()
        .find_map(|line| line.strip_prefix("content-length:"))
        .context("test provider request length missing")?
        .trim()
        .parse()?;
    ensure!(
        length < 2 * 1024 * 1024,
        "test provider request is too large"
    );
    while request.len() < body_start + length {
        let mut bytes = [0; 4096];
        let length = stream.read(&mut bytes).await?;
        ensure!(length != 0, "truncated test provider request");
        request.extend_from_slice(&bytes[..length]);
    }
    let input: serde_json::Value =
        serde_json::from_slice(&request[body_start..body_start + length])?;
    let input = input["input"].to_string();
    ensure!(
        input.contains("coco-native-history") && input.contains(super::NEWEST_MARKER),
        "model did not receive captured context"
    );
    ensure!(
        !input.contains(super::LATER_MARKER),
        "model received later source work"
    );
    let events = [
        json!({"type": "response.created", "response": {"id": "context-proof-response"}}),
        json!({"type": "response.output_item.done", "output_index": 0, "item": {
            "id": "context-proof-message", "type": "message", "role": "assistant", "status": "completed",
            "content": [{"type": "output_text", "text": "context-capture-proof"}],
        }}),
        json!({"type": "response.completed", "response": {"id": "context-proof-response", "usage": {
            "input_tokens": 10, "output_tokens": 1, "total_tokens": 11,
        }}}),
    ];
    let body = events
        .iter()
        .map(|event| format!("data: {event}\n\n"))
        .collect::<String>();
    stream
        .write_all(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\
        Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .as_bytes(),
        )
        .await?;
    stream.shutdown().await?;
    Ok(())
}
