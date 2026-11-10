use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

pub(super) struct Fixture {
    pub root: tempfile::TempDir,
    pub socket: PathBuf,
    pub calls: Arc<Mutex<Vec<Value>>>,
    task: tokio::task::JoinHandle<()>,
}

impl Fixture {
    pub async fn new() -> Result<Self> {
        let root = tempfile::tempdir()?;
        let socket = root.path().join("rpc.sock");
        let listener = UnixListener::bind(&socket)?;
        let calls = Arc::new(Mutex::new(Vec::new()));
        let recorded = calls.clone();
        let task = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let (read, mut write) = stream.into_split();
                let mut line = String::new();
                if BufReader::new(read).read_line(&mut line).await.unwrap_or(0) == 0 {
                    continue;
                }
                let request: Value = serde_json::from_str(&line).unwrap();
                recorded.lock().unwrap().push(request.clone());
                let response = json!({"id":request["id"],"error":{"code":"FIXTURE_DISPATCHED","message":"isolated no-op backend"}});
                write
                    .write_all(format!("{response}\n").as_bytes())
                    .await
                    .unwrap();
            }
        });
        Ok(Self {
            root,
            socket,
            calls,
            task,
        })
    }

    pub fn apply(&self, command: &mut tokio::process::Command) {
        command
            .env("HOME", self.root.path())
            .env("CODEX_HOME", self.root.path().join("codex"))
            .env("COCO_DATA_DIR", self.root.path().join("data"))
            .env("COCO_SOCKET_PATH", &self.socket)
            .env("NO_COLOR", "1");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub(super) fn create_input() -> Value {
    json!({"operationId":"create-key","name":"fix/login", "worktree":{"kind":"newBranch","base":{"kind":"revision","revision":"main"}},
        "changes":"ignore","profile":"default","context":{"kind":"fork","workspace":"feat/auth","compact":false}})
}
