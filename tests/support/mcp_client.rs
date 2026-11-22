use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::time::timeout;

const TIMEOUT: Duration = Duration::from_secs(15);

/// A real stdio client, shared by the fake-worker and installed-Codex tests.
pub(crate) struct McpClient {
    child: Child,
    input: ChildStdin,
    output: Lines<BufReader<ChildStdout>>,
    next_id: u64,
    request_meta: Option<Value>,
}

impl McpClient {
    pub(crate) async fn spawn(command: &mut Command) -> Result<Self> {
        Self::spawn_with_protocol(command, "2025-03-26").await
    }

    pub(crate) async fn spawn_with_protocol(command: &mut Command, protocol: &str) -> Result<Self> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .context("could not start the MCP test client")?;
        let input = child.stdin.take().context("MCP had no stdin")?;
        let output = BufReader::new(child.stdout.take().context("MCP had no stdout")?).lines();
        let mut client = Self {
            child,
            input,
            output,
            next_id: 0,
            request_meta: (protocol == "2026-07-28").then(|| {
                json!({
                    "io.modelcontextprotocol/protocolVersion":protocol,
                    "io.modelcontextprotocol/clientInfo":{"name":"coco-process-test","version":"0"},
                    "io.modelcontextprotocol/clientCapabilities":{},
                })
            }),
        };
        if client.request_meta.is_some() {
            let discovery = client.request("server/discover", json!({})).await?;
            ensure!(
                discovery["supportedVersions"]
                    .as_array()
                    .is_some_and(|versions| versions.iter().any(|version| version == protocol)),
                "missing MCP discovery version: {discovery}"
            );
        } else {
            let initialized = client
                .request(
                    "initialize",
                    json!({
                        "protocolVersion":protocol, "capabilities":{},
                        "clientInfo":{"name":"coco-process-test","version":"0"},
                    }),
                )
                .await?;
            ensure!(
                initialized["protocolVersion"] == protocol,
                "unexpected MCP negotiation: {}",
                initialized["protocolVersion"]
            );
            client
                .send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
                .await?;
        }
        Ok(client)
    }

    pub(crate) async fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.send_request(method, params).await?;
        self.receive_response(id, method).await
    }

    pub(crate) async fn send_request(&mut self, method: &str, mut params: Value) -> Result<u64> {
        if let Some(meta) = self.request_meta.as_ref().and_then(Value::as_object) {
            if params["_meta"].is_null() {
                params["_meta"] = json!({});
            }
            params["_meta"]
                .as_object_mut()
                .context("invalid MCP metadata")?
                .extend(meta.clone());
        }
        self.next_id += 1;
        let id = self.next_id;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
            .await?;
        Ok(id)
    }

    async fn receive_response(&mut self, id: u64, method: &str) -> Result<Value> {
        timeout(TIMEOUT, async {
            loop {
                let line = self
                    .output
                    .next_line()
                    .await?
                    .context("MCP closed stdout")?;
                let frame: Value = serde_json::from_str(&line)?;
                if frame.get("id") == Some(&json!(id)) {
                    ensure!(frame.get("error").is_none(), "MCP {method} failed: {frame}");
                    return frame
                        .get("result")
                        .cloned()
                        .context("MCP response had no result");
                }
            }
        })
        .await
        .context("MCP request timed out")?
    }

    pub(crate) async fn call(&mut self, tool: &str, arguments: Value) -> Result<Value> {
        let result = self
            .request("tools/call", json!({"name": tool, "arguments": arguments}))
            .await?;
        ensure!(
            result.get("isError") != Some(&json!(true)),
            "MCP {tool} failed: {result}"
        );
        tool_data(&result)
    }

    async fn send(&mut self, frame: Value) -> Result<()> {
        let mut bytes = serde_json::to_vec(&frame)?;
        bytes.push(b'\n');
        self.input.write_all(&bytes).await?;
        self.input.flush().await?;
        Ok(())
    }

    pub(crate) async fn stop(mut self) -> Result<()> {
        drop(self.input);
        let status = timeout(TIMEOUT, self.child.wait())
            .await
            .context("MCP did not exit")??;
        ensure!(status.success(), "MCP exited unsuccessfully: {status}");
        Ok(())
    }
}

pub(crate) fn tool_data(result: &Value) -> Result<Value> {
    if let Some(value) = result.get("structuredContent") {
        return Ok(value.clone());
    }
    // Legacy MCP revisions allow only object-valued structuredContent.
    // The server retains list/scalar data as the first JSON text block.
    let first = result["content"]
        .as_array()
        .and_then(|content| content.first())
        .context("MCP tool had no data content")?;
    ensure!(first["type"] == "text", "MCP tool data was not text");
    serde_json::from_str(
        first["text"]
            .as_str()
            .context("MCP tool text was missing")?,
    )
    .context("MCP tool data text was not JSON")
}

#[test]
fn tool_data_preserves_structured_values_and_decodes_legacy_json_without_guessing() {
    for value in [
        json!([]),
        json!([{"name":"review"}]),
        json!({"state":"finished"}),
        json!(null),
    ] {
        assert_eq!(
            tool_data(&json!({"structuredContent":value})).unwrap(),
            value
        );
        assert_eq!(
            tool_data(&json!({"content":[{"type":"text","text":value.to_string()}]})).unwrap(),
            value,
        );
    }
    for missing in [
        json!({}),
        json!({"content":[]}),
        json!({"content":[{"type":"image","text":"[]"}]}),
        json!({"content":[{"type":"text","text":"not JSON"}]}),
        json!({"content":[{"type":"text","text":"description"},{"type":"text","text":"[]"}]}),
    ] {
        assert!(tool_data(&missing).is_err(), "unexpected data: {missing}");
    }
}
