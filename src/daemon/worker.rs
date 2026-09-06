use std::collections::HashSet;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::codex::CodexClient;
use crate::coordinator::{StartedThread, StartedTurn, WorkerError, WorkerRuntime};
use crate::domain::{CodexModel, CodexThreadStatus};

const MODEL_PAGE_LIMIT: u32 = 100;
const MAX_MODEL_PAGES: usize = 100;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelPage {
    data: Vec<CodexModel>,
    #[serde(default)]
    next_cursor: Option<String>,
}

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
    async fn list_models(&self) -> Result<Vec<CodexModel>, WorkerError> {
        let mut models = Vec::new();
        let mut cursor: Option<String> = None;
        let mut seen_cursors = HashSet::new();
        for _ in 0..MAX_MODEL_PAGES {
            let mut params = json!({
                "limit": MODEL_PAGE_LIMIT,
                "includeHidden": false,
            });
            if let Some(cursor) = cursor.as_deref() {
                params["cursor"] = Value::String(cursor.to_owned());
            }
            let response = self
                .client
                .request("model/list", params)
                .await
                .map_err(WorkerError::runtime)?;
            let page = serde_json::from_value::<ModelPage>(response)
                .map_err(|error| WorkerError::InvalidModelCatalog(error.to_string()))?;
            models.extend(page.data);
            let Some(next_cursor) = page.next_cursor else {
                return Ok(models);
            };
            if !seen_cursors.insert(next_cursor.clone()) {
                return Err(WorkerError::InvalidModelCatalog(
                    "the App Server repeated a pagination cursor".to_owned(),
                ));
            }
            cursor = Some(next_cursor);
        }
        Err(WorkerError::InvalidModelCatalog(format!(
            "the catalog exceeded {MAX_MODEL_PAGES} pages"
        )))
    }

    async fn start_thread(
        &self,
        name: &str,
        cwd: &Path,
        config: Value,
        model: Option<&str>,
    ) -> Result<StartedThread, WorkerError> {
        let params = with_model(
            json!({
                "cwd": cwd,
                "config": config,
                "ephemeral": false,
            }),
            model,
        );
        let response = self
            .client
            .request("thread/start", params)
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
        model: Option<&str>,
    ) -> Result<StartedThread, WorkerError> {
        let params = with_model(
            json!({
                "threadId": source_thread_id,
                "cwd": cwd,
                "config": config,
                "ephemeral": false,
                "deferGoalContinuation": true,
            }),
            model,
        );
        let response = self
            .client
            .request("thread/fork", params)
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
        model: Option<&str>,
    ) -> Result<StartedThread, WorkerError> {
        let params = with_model(
            json!({
                "threadId": thread_id,
                "cwd": cwd,
                "config": config,
            }),
            model,
        );
        let response = self
            .client
            .request("thread/resume", params)
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

fn with_model(mut params: Value, model: Option<&str>) -> Value {
    if let Some(model) = model {
        params["model"] = Value::String(model.to_owned());
    }
    params
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
