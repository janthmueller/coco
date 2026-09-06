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
    async fn start_thread(
        &self,
        name: &str,
        cwd: &Path,
        config: Value,
    ) -> Result<StartedThread, WorkerError> {
        let response = self
            .client
            .request(
                "thread/start",
                json!({
                    "cwd": cwd,
                    "config": config,
                    "ephemeral": false,
                }),
            )
            .await
            .map_err(WorkerError::runtime)?;
        let started = decode_thread_response(response, cwd)?;
        self.set_thread_name(&started.id, name).await?;
        Ok(started)
    }

    async fn fork_thread(
        &self,
        name: &str,
        source_thread_id: &str,
        cwd: &Path,
        config: Value,
    ) -> Result<StartedThread, WorkerError> {
        let response = self
            .client
            .request(
                "thread/fork",
                json!({
                    "threadId": source_thread_id,
                    "cwd": cwd,
                    "config": config,
                    "ephemeral": false,
                    "deferGoalContinuation": true,
                }),
            )
            .await
            .map_err(WorkerError::runtime)?;
        let started = decode_thread_response(response, cwd)?;
        self.set_thread_name(&started.id, name).await?;
        Ok(started)
    }

    async fn compact_thread(&self, thread_id: &str) -> Result<(), WorkerError> {
        self.client
            .request("thread/compact/start", json!({"threadId": thread_id}))
            .await
            .map_err(WorkerError::runtime)?;
        Ok(())
    }

    async fn resume_thread(
        &self,
        thread_id: &str,
        cwd: &Path,
        config: Value,
    ) -> Result<StartedThread, WorkerError> {
        let response = self
            .client
            .request(
                "thread/resume",
                json!({
                    "threadId": thread_id,
                    "cwd": cwd,
                    "config": config,
                }),
            )
            .await
            .map_err(WorkerError::runtime)?;
        let resumed = decode_thread_response(response, cwd)?;
        if resumed.id != thread_id {
            return Err(WorkerError::ThreadIdMismatch {
                expected: thread_id.to_owned(),
                actual: resumed.id,
            });
        }
        Ok(resumed)
    }

    async fn start_turn(
        &self,
        thread_id: &str,
        cwd: &Path,
        client_message_id: &str,
        message: &str,
        additional_context: Option<Value>,
    ) -> Result<StartedTurn, WorkerError> {
        let mut params = json!({
            "threadId": thread_id,
            "cwd": cwd,
            "clientUserMessageId": client_message_id,
            "input": [{"type": "text", "text": message}],
        });
        if let Some(additional_context) = additional_context {
            params["additionalContext"] = additional_context;
        }
        let response = self
            .client
            .request("turn/start", params)
            .await
            .map_err(WorkerError::runtime)?;
        let id = response
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .ok_or(WorkerError::InvalidResponse("turn.id"))?
            .to_owned();
        Ok(StartedTurn { id })
    }

    async fn respond_to_request(&self, id: Value, result: Value) -> Result<(), WorkerError> {
        self.client
            .respond(id, result)
            .await
            .map_err(WorkerError::runtime)
    }
}

impl CodexWorker {
    async fn set_thread_name(&self, thread_id: &str, name: &str) -> Result<(), WorkerError> {
        self.client
            .request(
                "thread/name/set",
                json!({
                    "threadId": thread_id,
                    "name": name,
                }),
            )
            .await
            .map_err(WorkerError::runtime)?;
        Ok(())
    }
}

fn decode_thread_response(
    response: Value,
    expected_cwd: &Path,
) -> Result<StartedThread, WorkerError> {
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
    let cwd = response
        .get("cwd")
        .and_then(Value::as_str)
        .ok_or(WorkerError::InvalidResponse("cwd"))
        .map(PathBuf::from)?;
    if cwd != expected_cwd {
        return Err(WorkerError::CwdMismatch {
            expected: expected_cwd.to_owned(),
            actual: cwd,
        });
    }
    Ok(StartedThread {
        id,
        status,
        cwd,
        response,
    })
}
