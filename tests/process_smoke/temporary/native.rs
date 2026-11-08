use super::*;
use std::collections::HashMap;

/// Match native empty-rollout reads, including after the TUI unsubscribes.
pub(super) struct EmptyThreads {
    sessions: PathBuf,
    paths: Mutex<HashMap<String, PathBuf>>,
}

impl EmptyThreads {
    pub(super) fn new(sessions: PathBuf) -> Self {
        Self {
            sessions,
            paths: Mutex::new(HashMap::new()),
        }
    }

    pub(super) fn reply(&self, request: &Value) -> Result<Option<Value>> {
        let mut response = match request["method"].as_str() {
            Some("initialize" | "thread/unsubscribe") => json!({"result": {}}),
            Some("initialized") => return Ok(None),
            Some("thread/start") => {
                let id = uuid::Uuid::now_v7().to_string();
                let path = self.sessions.join(format!("rollout-2026-11-08-{id}.jsonl"));
                fs::create_dir_all(&self.sessions)?;
                fs::write(&path, "")?;
                self.paths.lock().unwrap().insert(id.clone(), path.clone());
                json!({"result": {"thread": {"id": id, "path": path,
                    "cwd": request["params"]["cwd"], "status": {"type": "idle"}}}})
            }
            Some("thread/read") => {
                let id = request["params"]["threadId"]
                    .as_str()
                    .context("missing thread ID")?;
                let paths = self.paths.lock().unwrap();
                let path = paths.get(id).context("unexpected thread read")?;
                json!({"error": {"code": -32603, "message": format!(
                    "failed to read thread: thread-store internal error: failed to read session metadata {}: rollout at {} is empty",
                    path.display(), path.display()
                )}})
            }
            _ => bail!(
                "unexpected native work in empty TUI fixture: {}",
                request["method"]
            ),
        };
        response["id"] = request["id"].clone();
        Ok(Some(response))
    }
}
