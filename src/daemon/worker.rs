use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::codex::CodexClient;
use crate::coordinator::{StartedThread, StartedTurn, WorkerError, WorkerRuntime};
use crate::domain::CodexThreadStatus;

#[derive(Debug, Clone)]
pub(super) struct CodexWorker {
    client: CodexClient,
}

impl CodexWorker {
    pub(super) fn new(client: CodexClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl WorkerRuntime for CodexWorker {
    async fn start_thread(&self, cwd: &Path, config: Value) -> Result<StartedThread, WorkerError> {
        let response = self
            .client
            .request(
                "thread/start",
                json!({
                    "cwd": cwd,
                    "runtimeWorkspaceRoots": [cwd],
                    "config": config,
                    "ephemeral": false,
                }),
            )
            .await
            .map_err(WorkerError::runtime)?;
        let id = response
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .ok_or(WorkerError::InvalidResponse("thread.id"))?
            .to_owned();
        let status = response
            .pointer("/thread/status")
            .cloned()
            .ok_or(WorkerError::InvalidResponse("thread.status"))
            .and_then(|status| {
                serde_json::from_value::<CodexThreadStatus>(status)
                    .map(CodexThreadStatus::canonicalized)
                    .map_err(|error| WorkerError::InvalidThreadStatus(error.to_string()))
            })?;
        let returned_cwd = response
            .get("cwd")
            .and_then(Value::as_str)
            .ok_or(WorkerError::InvalidResponse("cwd"))
            .map(PathBuf::from)?;
        if returned_cwd != cwd {
            return Err(WorkerError::CwdMismatch {
                expected: cwd.to_owned(),
                actual: returned_cwd,
            });
        }
        Ok(StartedThread {
            id,
            status,
            response,
        })
    }

    async fn start_turn(
        &self,
        thread_id: &str,
        cwd: &Path,
        client_message_id: &str,
        message: &str,
    ) -> Result<StartedTurn, WorkerError> {
        let response = self
            .client
            .request(
                "turn/start",
                json!({
                    "threadId": thread_id,
                    "cwd": cwd,
                    "clientUserMessageId": client_message_id,
                    "input": [{"type": "text", "text": message}],
                }),
            )
            .await
            .map_err(WorkerError::runtime)?;
        let id = response
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .ok_or(WorkerError::InvalidResponse("turn.id"))?
            .to_owned();
        Ok(StartedTurn { id })
    }
}
