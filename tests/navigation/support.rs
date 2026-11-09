use std::path::PathBuf;
use std::process::{Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::process::Command;
use tokio::task::JoinHandle;

pub(super) struct Fixture {
    pub(super) root: tempfile::TempDir,
    pub(super) destination: PathBuf,
    calls: Arc<Mutex<Vec<Value>>>,
    task: JoinHandle<()>,
}

impl Fixture {
    pub(super) async fn new() -> Result<Self> {
        let root = tempfile::tempdir()?;
        let destination = root
            .path()
            .join("worktree über 'quotes' $vars ; [glob] $(command)");
        std::fs::create_dir(&destination)?;
        let listener = UnixListener::bind(root.path().join("rpc.sock"))?;
        let calls = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&calls);
        let path = destination.clone();
        let task = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let (read, mut write) = stream.into_split();
                let mut line = String::new();
                if BufReader::new(read).read_line(&mut line).await.unwrap_or(0) == 0 {
                    continue;
                }
                let request: Value = serde_json::from_str(&line).unwrap();
                recorded.lock().unwrap().push(request.clone());
                assert_eq!(request["method"], "workspace.path.get");
                let response = if request["params"]["workspace"] == "missing" {
                    json!({"id":request["id"], "error":{"code":"WORKSPACE_NOT_FOUND","message":"not found"}})
                } else {
                    json!({"id":request["id"], "result":path})
                };
                write
                    .write_all(format!("{response}\n").as_bytes())
                    .await
                    .unwrap();
            }
        });
        Ok(Self {
            root,
            destination,
            calls,
            task,
        })
    }

    pub(super) async fn run(&self, args: &[&str]) -> Result<Output> {
        let mut command = Command::new(env!("CARGO_BIN_EXE_coco"));
        command
            .args(args)
            .current_dir(self.root.path())
            .env("COCO_SOCKET_PATH", self.root.path().join("rpc.sock"))
            .env("COCO_DATA_DIR", self.root.path().join("data"))
            .env("CODEX_HOME", self.root.path().join("codex"))
            .env("NO_COLOR", "1")
            .stdin(Stdio::null())
            .kill_on_drop(true);
        Ok(
            tokio::time::timeout(Duration::from_secs(8), command.output())
                .await
                .context("navigation CLI timed out")??,
        )
    }

    pub(super) fn calls(&self) -> Vec<Value> {
        self.calls.lock().unwrap().clone()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}
