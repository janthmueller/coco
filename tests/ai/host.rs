use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

use super::support::Fixture;

pub(super) struct Host {
    child: Child,
    input: ChildStdin,
    output: Lines<BufReader<ChildStdout>>,
}

impl Host {
    pub async fn start(
        fixture: &Fixture,
        confirmation: bool,
        capability: bool,
        granted: bool,
    ) -> Result<Self> {
        let mut command = Command::new(env!("CARGO_BIN_EXE_coco-mcp"));
        fixture.apply(&mut command);
        command.arg("--repository").arg(fixture.root.path());
        if granted {
            command.args(["--allow-create", "--allow-send", "--allow-interrupt"]);
        }
        if confirmation {
            command.arg("--confirm-changes");
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap()).lines();
        let mut host = Self {
            child,
            input,
            output,
        };
        host.send(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-11-25","capabilities":if capability { json!({"elicitation":{"form":{}}}) } else { json!({}) },
            "clientInfo":{"name":"isolated-confirmation-test","version":"0"}}})).await?;
        ensure!(
            host.next().await?["result"]["capabilities"].is_object(),
            "initialization failed"
        );
        host.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .await?;
        Ok(host)
    }

    pub async fn begin(&mut self, tool: &str, input: Value) -> Result<()> {
        self.send(json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":tool,"arguments":input}})).await
    }

    pub async fn next(&mut self) -> Result<Value> {
        tokio::time::timeout(Duration::from_secs(8), async {
            loop {
                let line = self
                    .output
                    .next_line()
                    .await?
                    .context("MCP closed stdout")?;
                let value: Value = serde_json::from_str(&line)?;
                if value.get("id").is_some() {
                    return Ok(value);
                }
            }
        })
        .await
        .context("MCP host timed out")?
    }

    pub async fn answer(&mut self, request: &Value, response: Value) -> Result<()> {
        self.send(json!({"jsonrpc":"2.0", "id":request["id"], "result":response}))
            .await
    }

    pub async fn reject_transport(&mut self, request: &Value) -> Result<()> {
        self.send(json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":-32603,"message":"isolated host failure"}})).await
    }

    pub async fn send(&mut self, frame: Value) -> Result<()> {
        self.input
            .write_all(format!("{frame}\n").as_bytes())
            .await?;
        self.input.flush().await?;
        Ok(())
    }

    pub async fn stop(mut self) -> Result<()> {
        drop(self.input);
        let status = tokio::time::timeout(Duration::from_secs(8), self.child.wait()).await??;
        ensure!(status.success(), "MCP did not exit cleanly: {status}");
        Ok(())
    }
}
