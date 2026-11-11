use std::process::{Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::process::Command;
use tokio::task::JoinHandle;

pub(super) struct Fixture {
    root: tempfile::TempDir,
    calls: Arc<Mutex<Vec<Value>>>,
    task: JoinHandle<()>,
}

impl Fixture {
    pub(super) async fn new(frames: Vec<Vec<Value>>, usage: Vec<Value>) -> Result<Self> {
        let root = tempfile::tempdir()?;
        let listener = UnixListener::bind(root.path().join("rpc.sock"))?;
        let calls = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&calls);
        let frame = AtomicUsize::new(0);
        let task = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let (read, mut write) = stream.into_split();
                let mut line = String::new();
                if BufReader::new(read).read_line(&mut line).await.unwrap_or(0) == 0 {
                    continue;
                }
                let request: Value = serde_json::from_str(&line).unwrap();
                recorded.lock().unwrap().push(request.clone());
                let result = match request["method"].as_str().unwrap() {
                    "workspace.list" => {
                        json!(frames[frame.fetch_add(1, Ordering::SeqCst).min(frames.len() - 1)])
                    }
                    "workspace.usage.list" => json!(usage),
                    "workspace.usage.get" => usage
                        .iter()
                        .find(|item| item["workspace"]["id"] == request["params"]["workspace"])
                        .cloned()
                        .unwrap_or(Value::Null),
                    "account.quota.get" => {
                        json!({"status":"unavailable", "reason":"notReported", "checkedAtMs":1})
                    }
                    method => panic!("unexpected read-only query {method}"),
                };
                let response = if result.is_null() {
                    json!({"id":request["id"],"error":{"code":"WORKSPACE_NOT_FOUND","message":"removed"}})
                } else {
                    json!({"id":request["id"], "result":result})
                };
                write
                    .write_all(format!("{response}\n").as_bytes())
                    .await
                    .unwrap();
            }
        });
        Ok(Self { root, calls, task })
    }

    pub(super) fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_coco"));
        command
            .args(args)
            .current_dir(self.root.path())
            .env("COCO_SOCKET_PATH", self.root.path().join("rpc.sock"))
            .env("COCO_DATA_DIR", self.root.path().join("data"))
            .env("CODEX_HOME", self.root.path().join("codex"))
            .env("NO_COLOR", "1")
            .env("RUST_LOG", "error")
            .stdin(Stdio::null())
            .kill_on_drop(true);
        command
    }

    pub(super) async fn run(&self, args: &[&str]) -> Result<Output> {
        Ok(
            tokio::time::timeout(Duration::from_secs(8), self.command(args).output())
                .await
                .context("status CLI timed out")??,
        )
    }

    pub(super) async fn json(&self, args: &[&str]) -> Result<Value> {
        let output = self.run(args).await?;
        ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(serde_json::from_slice(&output.stdout)?)
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

pub(super) fn row(repo: &str, name: &str, phase: &str) -> Value {
    json!({
        "id":format!("{repo}-{name}"),"repositoryId":repo,"name":name,
        "contextMode":"fresh","context":{},
        "profile":{"name":"default","sourceHash":"test","effectiveSettings":{}},
        "lifecycle":"ready","availability":if phase == "closed" {"closed"} else {"open"},
        "phase":phase,"waitReasons":[],"worktreeMode":"new_branch","branchName":format!("coco/{name}"),
        "createdAtMs":1,"updatedAtMs":1,"threadArchived":false,"clients":[],
        "repository":{"id":repo,"displayName":repo,"rootPath":format!("/repos/{repo}")}
    })
}

pub(super) fn usage(row: &Value, total: u64) -> Value {
    let mut workspace = row.clone();
    let repository = workspace
        .as_object_mut()
        .unwrap()
        .remove("repository")
        .unwrap();
    workspace.as_object_mut().unwrap().remove("clients");
    workspace["phase"] = json!("idle");
    json!({"workspace":workspace,"repository":repository,
        "tokens":{"schemaVersion":1,"threadId":"test","turnId":"test","runtimeGeneration":"test", "observedAtMs":1,
            "source":"threadTokenUsageUpdated","isFresh":true,"modelContextWindow":1000,
            "total":{"totalTokens":total,"inputTokens":0,"cachedInputTokens":0,"outputTokens":0,"reasoningOutputTokens":0},
            "last":{"totalTokens":10,"inputTokens":0,"cachedInputTokens":0,"outputTokens":0,"reasoningOutputTokens":0}},
        "cost":{"status":"unavailable","reason":"notReported"}})
}

pub(super) fn row_with_recency(repo: &str, name: &str, phase: &str, time: Option<i64>) -> Value {
    let mut row = row(repo, name, phase);
    row["threadRuntime"] = json!({"status":{"type":"idle"},
        "recencyAtMs":time,"runtimeGeneration":"test", "observedAtMs":1,
        "isFresh":true});
    row
}

pub(super) fn names(value: &Value) -> Vec<&str> {
    value["workspaces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["name"].as_str().unwrap())
        .collect()
}
