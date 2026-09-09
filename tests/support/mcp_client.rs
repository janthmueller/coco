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
}

impl McpClient {
    pub(crate) async fn spawn(command: &mut Command) -> Result<Self> {
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
        };
        client
            .request(
                "initialize",
                json!({
                    "protocolVersion": "2025-03-26",
                    "capabilities": {},
                    "clientInfo": {"name": "coco-process-test", "version": "0"}
                }),
            )
            .await?;
        client
            .send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
            .await?;
        Ok(client)
    }

    pub(crate) async fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
            .await?;
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
        result
            .get("structuredContent")
            .cloned()
            .context("MCP tool had no structured result")
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
