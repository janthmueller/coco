use std::collections::HashSet;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::codex::{CodexClient, CodexError};
use crate::coordinator::{
    LocatedNativeThread, NativeThread, StartedThread, StartedTurn, WorkerError,
    WorkerExecutionEnvironment, WorkerRuntime,
};
use crate::domain::runtime::{
    WorkspaceResourceCapabilities, WorkspaceResourceControllerStatus,
    WorkspaceResourcePolicySnapshot, WorkspaceRuntimeResources, WorkspaceRuntimeState,
};
use crate::domain::usage::{NativeThreadCostEstimate, NativeThreadCostGroup};
use crate::domain::{CodexModel, CodexThreadStatus};

use super::execution::WorkspaceExecutors;

const MODEL_PAGE_LIMIT: u32 = 100;
const MAX_MODEL_PAGES: usize = 100;
const THREAD_PAGE_LIMIT: u32 = 100;
const MAX_THREAD_PAGES: usize = 100;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelPage {
    data: Vec<CodexModel>,
    #[serde(default)]
    next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadPage {
    data: Vec<ThreadIdentity>,
    #[serde(default)]
    next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ThreadIdentity {
    id: String,
}

#[derive(Debug, Deserialize)]
struct CollectionPage {
    data: Vec<Value>,
    #[serde(default)]
    next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ThreadReadResponse {
    thread: ThreadReadWire,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountUsageResponse {
    #[serde(default)]
    thread_usage: Option<ThreadUsageWire>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadUsageWire {
    thread_id: String,
    estimated_usage_credits_micros: u64,
    #[serde(default)]
    estimated_usage_usd_micros: Option<u64>,
    groups: Vec<ThreadUsageGroupWire>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadUsageGroupWire {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    reasoning_effort: Option<String>,
    #[serde(default)]
    speed: Option<String>,
    estimated_usage_credits_micros: u64,
    #[serde(default)]
    net_new_input_tokens: Option<u64>,
    #[serde(default)]
    cached_input_tokens: Option<u64>,
    #[serde(default)]
    input_tokens: Option<u64>,
    #[serde(default)]
    output_tokens: Option<u64>,
    #[serde(default)]
    total_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadReadWire {
    id: String,
    cwd: PathBuf,
    #[serde(default)]
    path: Option<PathBuf>,
    #[serde(default)]
    name: Option<String>,
    status: CodexThreadStatus,
    #[serde(default)]
    forked_from_id: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct CodexWorker {
    client: CodexClient,
    workspace_executors: Option<WorkspaceExecutors>,
}

impl CodexWorker {
    pub(super) fn new(
        client: CodexClient,
        workspace_executors: Option<WorkspaceExecutors>,
    ) -> Self {
        Self {
            client,
            workspace_executors,
        }
    }

    async fn workspace_environment(
        &self,
        workspace_id: &str,
        cwd: &Path,
    ) -> Result<Option<WorkerExecutionEnvironment>, WorkerError> {
        let Some(executors) = self.workspace_executors.as_ref() else {
            return Ok(None);
        };
        executors
            .ensure(workspace_id, cwd)
            .await
            .map(Some)
            .map_err(WorkerError::runtime)
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

    async fn read_thread(&self, thread_id: &str) -> Result<NativeThread, WorkerError> {
        let response = self
            .client
            .request(
                "thread/read",
                json!({
                    "threadId": thread_id,
                    "includeTurns": false,
                }),
            )
            .await
            .map_err(WorkerError::runtime)?;
        decode_thread_read_response(response)
    }

    async fn read_thread_cost(
        &self,
        thread_id: &str,
    ) -> Result<Option<NativeThreadCostEstimate>, WorkerError> {
        let response = self
            .client
            .request("account/usage/read", json!({"threadId": thread_id}))
            .await
            .map_err(WorkerError::runtime)?;
        decode_thread_cost_response(response, thread_id)
    }

    async fn find_materialized_thread(
        &self,
        thread_id: &str,
        _cwd: &Path,
    ) -> Result<Option<NativeThread>, WorkerError> {
        let response = self
            .client
            .request(
                "thread/read",
                json!({"threadId": thread_id, "includeTurns": false}),
            )
            .await
            .map_err(WorkerError::runtime)?;
        let response = serde_json::from_value::<ThreadReadResponse>(response)
            .map_err(|error| WorkerError::InvalidThreadRead(error.to_string()))?;
        let Some(rollout_path) = response
            .thread
            .path
            .as_ref()
            .filter(|path| !path.as_os_str().is_empty())
        else {
            return Ok(None);
        };
        match tokio::fs::metadata(rollout_path).await {
            Ok(metadata) if metadata.is_file() && metadata.len() > 0 => {}
            Ok(_) => return Ok(None),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(WorkerError::runtime(error)),
        }
        decode_thread_wire(response.thread).map(Some)
    }

    async fn set_thread_name(&self, thread_id: &str, name: &str) -> Result<(), WorkerError> {
        self.set_thread_name_request(thread_id, name).await
    }

    async fn locate_thread(
        &self,
        thread_id: &str,
    ) -> Result<Option<LocatedNativeThread>, WorkerError> {
        let response = match self
            .client
            .request(
                "thread/read",
                json!({"threadId": thread_id, "includeTurns": false}),
            )
            .await
        {
            Ok(response) => response,
            Err(error) if is_missing_thread_read(&error, thread_id) => return Ok(None),
            Err(error) => return Err(WorkerError::runtime(error)),
        };
        decode_located_thread_response(response).map(Some)
    }

    async fn list_thread_descendants(&self, thread_id: &str) -> Result<Vec<String>, WorkerError> {
        let mut descendants = Vec::new();
        for archived in [false, true] {
            let mut cursor: Option<String> = None;
            let mut seen_cursors = HashSet::new();
            let mut exhausted = false;
            for _ in 0..MAX_THREAD_PAGES {
                let mut params = json!({
                    "limit": THREAD_PAGE_LIMIT,
                    "archived": archived,
                    "ancestorThreadId": thread_id,
                });
                if let Some(cursor) = cursor.as_deref() {
                    params["cursor"] = Value::String(cursor.to_owned());
                }
                let response = self
                    .client
                    .request("thread/list", params)
                    .await
                    .map_err(WorkerError::runtime)?;
                let page = serde_json::from_value::<ThreadPage>(response)
                    .map_err(|error| WorkerError::InvalidThreadRead(error.to_string()))?;
                descendants.extend(page.data.into_iter().map(|thread| thread.id));
                let Some(next_cursor) = page.next_cursor else {
                    exhausted = true;
                    break;
                };
                if !seen_cursors.insert(next_cursor.clone()) {
                    return Err(WorkerError::InvalidThreadRead(
                        "thread/list repeated a pagination cursor".to_owned(),
                    ));
                }
                cursor = Some(next_cursor);
            }
            if !exhausted {
                return Err(WorkerError::InvalidThreadRead(format!(
                    "thread/list exceeded {MAX_THREAD_PAGES} pages"
                )));
            }
        }
        descendants.sort_unstable();
        descendants.dedup();
        Ok(descendants)
    }

    async fn background_terminal_count(&self, thread_id: &str) -> Result<usize, WorkerError> {
        let mut count = 0;
        let mut cursor: Option<String> = None;
        let mut seen_cursors = HashSet::new();
        for _ in 0..MAX_THREAD_PAGES {
            let mut params = json!({
                "threadId": thread_id,
                "limit": THREAD_PAGE_LIMIT,
            });
            if let Some(cursor) = cursor.as_deref() {
                params["cursor"] = Value::String(cursor.to_owned());
            }
            let response = self
                .client
                .request("thread/backgroundTerminals/list", params)
                .await
                .map_err(WorkerError::runtime)?;
            let page = serde_json::from_value::<CollectionPage>(response)
                .map_err(|error| WorkerError::InvalidThreadRead(error.to_string()))?;
            count += page.data.len();
            let Some(next_cursor) = page.next_cursor else {
                return Ok(count);
            };
            if !seen_cursors.insert(next_cursor.clone()) {
                return Err(WorkerError::InvalidThreadRead(
                    "background-terminal list repeated a pagination cursor".to_owned(),
                ));
            }
            cursor = Some(next_cursor);
        }
        Err(WorkerError::InvalidThreadRead(format!(
            "background-terminal list exceeded {MAX_THREAD_PAGES} pages"
        )))
    }

    async fn unsubscribe_thread(&self, thread_id: &str) -> Result<(), WorkerError> {
        self.client
            .request("thread/unsubscribe", json!({"threadId": thread_id}))
            .await
            .map_err(WorkerError::runtime)?;
        Ok(())
    }

    async fn archive_thread(&self, thread_id: &str) -> Result<(), WorkerError> {
        self.client
            .request("thread/archive", json!({"threadId": thread_id}))
            .await
            .map_err(WorkerError::runtime)?;
        Ok(())
    }

    async fn unarchive_thread(&self, thread_id: &str) -> Result<NativeThread, WorkerError> {
        let response = self
            .client
            .request("thread/unarchive", json!({"threadId": thread_id}))
            .await
            .map_err(WorkerError::runtime)?;
        decode_thread_read_response(response)
    }

    async fn delete_thread(&self, thread_id: &str) -> Result<(), WorkerError> {
        self.client
            .request("thread/delete", json!({"threadId": thread_id}))
            .await
            .map_err(WorkerError::runtime)?;
        Ok(())
    }

    async fn prepare_workspace_execution(
        &self,
        workspace_id: &str,
        cwd: &Path,
    ) -> Result<Option<WorkerExecutionEnvironment>, WorkerError> {
        self.workspace_environment(workspace_id, cwd).await
    }

    async fn stop_workspace_execution(&self, workspace_id: &str) -> Result<(), WorkerError> {
        let Some(executors) = self.workspace_executors.as_ref() else {
            return Ok(());
        };
        executors
            .stop(workspace_id)
            .await
            .map_err(WorkerError::runtime)
    }

    async fn workspace_resources(
        &self,
        workspace_id: &str,
    ) -> Result<Option<WorkspaceRuntimeResources>, WorkerError> {
        let Some(executors) = self.workspace_executors.as_ref() else {
            return Ok(None);
        };
        executors
            .resources(workspace_id)
            .await
            .map(Some)
            .map_err(WorkerError::runtime)
    }

    fn workspace_resource_capabilities(&self) -> WorkspaceResourceCapabilities {
        self.workspace_executors.as_ref().map_or_else(
            WorkspaceResourceCapabilities::unavailable,
            WorkspaceExecutors::resource_capabilities,
        )
    }

    async fn workspace_resource_policy_status(
        &self,
        workspace_id: &str,
    ) -> Result<WorkspaceResourceControllerStatus, WorkerError> {
        let Some(executors) = self.workspace_executors.as_ref() else {
            return Ok(WorkspaceResourceControllerStatus {
                capabilities: WorkspaceResourceCapabilities::unavailable(),
                runtime_state: WorkspaceRuntimeState::Inactive,
                applied_policy: None,
            });
        };
        executors
            .resource_policy_status(workspace_id)
            .await
            .map_err(WorkerError::runtime)
    }

    async fn configure_workspace_resource_policy(
        &self,
        workspace_id: &str,
        snapshot: WorkspaceResourcePolicySnapshot,
    ) -> Result<WorkspaceResourceControllerStatus, WorkerError> {
        let Some(executors) = self.workspace_executors.as_ref() else {
            let unsupported =
                WorkspaceResourceCapabilities::unavailable().unsupported_fields(&snapshot.policy);
            if unsupported.is_empty() {
                return self.workspace_resource_policy_status(workspace_id).await;
            }
            return Err(WorkerError::ResourcePolicyUnsupported {
                fields: unsupported,
            });
        };
        executors
            .configure_resource_policy(workspace_id, snapshot)
            .await
            .map_err(WorkerError::runtime)
    }

    async fn start_thread(
        &self,
        workspace_id: &str,
        name: &str,
        cwd: &Path,
        config: Value,
        model: Option<&str>,
    ) -> Result<StartedThread, WorkerError> {
        let environment = self.workspace_environment(workspace_id, cwd).await?;
        let params = with_environment(
            with_model(
                json!({
                    "cwd": cwd,
                    "config": config,
                    "ephemeral": false,
                }),
                model,
            ),
            environment.as_ref(),
        );
        let response = self
            .client
            .request("thread/start", params)
            .await
            .map_err(WorkerError::runtime)?;
        let started = decode_thread_response(response, cwd)?;
        self.set_thread_name_request(&started.id, name).await?;
        Ok(started)
    }

    async fn fork_thread(
        &self,
        workspace_id: &str,
        name: &str,
        source_thread_id: &str,
        cwd: &Path,
        config: Value,
        model: Option<&str>,
    ) -> Result<StartedThread, WorkerError> {
        let _environment = self.workspace_environment(workspace_id, cwd).await?;
        let params = with_model(
            json!({
                "threadId": source_thread_id,
                "cwd": cwd,
                "config": config,
                "ephemeral": false,
                "excludeTurns": true,
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
        self.set_thread_name_request(&started.id, name).await?;
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
        workspace_id: &str,
        thread_id: &str,
        cwd: &Path,
        config: Value,
        model: Option<&str>,
    ) -> Result<StartedThread, WorkerError> {
        let _environment = self.workspace_environment(workspace_id, cwd).await?;
        let params = with_model(
            json!({
                "threadId": thread_id,
                "cwd": cwd,
                "config": config,
                "excludeTurns": true,
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
        workspace_id: &str,
        thread_id: &str,
        cwd: &Path,
        client_message_id: &str,
        message: &str,
        additional_context: Option<Value>,
    ) -> Result<StartedTurn, WorkerError> {
        let environment = self.workspace_environment(workspace_id, cwd).await?;
        let mut params = with_environment(
            json!({
                "threadId": thread_id,
                "cwd": cwd,
                "clientUserMessageId": client_message_id,
                "input": [{"type": "text", "text": message}],
            }),
            environment.as_ref(),
        );
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

fn with_environment(mut params: Value, environment: Option<&WorkerExecutionEnvironment>) -> Value {
    if let Some(environment) = environment {
        params["environments"] = json!([{
            "environmentId": environment.environment_id,
            "cwd": environment.cwd,
            "runtimeWorkspaceRoots": environment.runtime_workspace_roots,
        }]);
    }
    params
}

impl CodexWorker {
    async fn set_thread_name_request(
        &self,
        thread_id: &str,
        name: &str,
    ) -> Result<(), WorkerError> {
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

fn decode_thread_read_response(response: Value) -> Result<NativeThread, WorkerError> {
    let response = serde_json::from_value::<ThreadReadResponse>(response)
        .map_err(|error| WorkerError::InvalidThreadRead(error.to_string()))?;
    decode_thread_wire(response.thread)
}

fn decode_thread_cost_response(
    response: Value,
    expected_thread_id: &str,
) -> Result<Option<NativeThreadCostEstimate>, WorkerError> {
    let response = serde_json::from_value::<AccountUsageResponse>(response)
        .map_err(|error| WorkerError::InvalidThreadUsage(error.to_string()))?;
    let Some(usage) = response.thread_usage else {
        return Ok(None);
    };
    if usage.thread_id != expected_thread_id {
        return Err(WorkerError::ThreadIdMismatch {
            expected: expected_thread_id.to_owned(),
            actual: usage.thread_id,
        });
    }
    Ok(Some(NativeThreadCostEstimate {
        thread_id: expected_thread_id.to_owned(),
        estimated_usage_credits_micros: usage.estimated_usage_credits_micros,
        estimated_usage_usd_micros: usage.estimated_usage_usd_micros,
        groups: usage
            .groups
            .into_iter()
            .map(|group| NativeThreadCostGroup {
                model: group.model,
                reasoning_effort: group.reasoning_effort,
                speed: group.speed,
                estimated_usage_credits_micros: group.estimated_usage_credits_micros,
                net_new_input_tokens: group.net_new_input_tokens,
                cached_input_tokens: group.cached_input_tokens,
                input_tokens: group.input_tokens,
                output_tokens: group.output_tokens,
                total_tokens: group.total_tokens,
            })
            .collect(),
    }))
}

fn decode_located_thread_response(response: Value) -> Result<LocatedNativeThread, WorkerError> {
    let response = serde_json::from_value::<ThreadReadResponse>(response)
        .map_err(|error| WorkerError::InvalidThreadRead(error.to_string()))?;
    let archived = response
        .thread
        .path
        .as_deref()
        .map(rollout_path_is_archived)
        .ok_or_else(|| {
            WorkerError::InvalidThreadRead(
                "thread/read did not expose a rollout path for archive-state verification"
                    .to_owned(),
            )
        })?;
    Ok(LocatedNativeThread {
        thread: decode_thread_wire(response.thread)?,
        archived,
    })
}

fn rollout_path_is_archived(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str() == "archived_sessions")
}

fn is_missing_thread_read(error: &CodexError, thread_id: &str) -> bool {
    matches!(
        error,
        CodexError::Rpc { message, .. }
            if message == &format!("thread not loaded: {thread_id}")
    )
}

fn decode_thread_wire(thread: ThreadReadWire) -> Result<NativeThread, WorkerError> {
    Ok(NativeThread {
        id: thread.id,
        cwd: thread.cwd,
        name: thread.name,
        status: thread.status.canonicalized(),
        forked_from_id: thread.forked_from_id,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::json;

    use super::{
        decode_located_thread_response, decode_thread_cost_response, decode_thread_read_response,
        with_environment,
    };
    use crate::coordinator::WorkerExecutionEnvironment;
    use crate::domain::CodexThreadStatus;

    #[test]
    fn selects_one_workspace_environment_without_replacing_other_params() {
        let environment = WorkerExecutionEnvironment {
            environment_id: "coco-runtime".to_owned(),
            cwd: PathBuf::from("/worktree"),
            runtime_workspace_roots: vec![PathBuf::from("/worktree")],
        };
        let params = with_environment(
            json!({"threadId": "thread-1", "input": []}),
            Some(&environment),
        );

        assert_eq!(params["threadId"], "thread-1");
        assert_eq!(
            params["environments"],
            json!([{
                "environmentId": "coco-runtime",
                "cwd": "/worktree",
                "runtimeWorkspaceRoots": ["/worktree"]
            }])
        );
    }

    #[test]
    fn decodes_only_the_stable_thread_metadata_projection() {
        let response = json!({
            "thread": {
                "id": "thread-child",
                "cwd": "/worktrees/fix-login",
                "name": "fix/login",
                "status": {
                    "type": "active",
                    "activeFlags": ["waitingOnUserInput", "waitingOnApproval", "waitingOnApproval"]
                },
                "forkedFromId": "thread-parent",
                "turns": [
                    {
                        "id": "turn-1",
                        "status": "completed",
                        "items": [
                            {"id": "user-1", "type": "userMessage", "content": []},
                            {"id": "agent-1", "type": "agentMessage", "text": "first"},
                            {"id": "agent-2", "type": "agentMessage", "text": "last"}
                        ]
                    },
                    {
                        "id": "turn-2",
                        "status": "futureStatus",
                        "items": []
                    }
                ]
            }
        });

        let thread = decode_thread_read_response(response).unwrap();
        assert_eq!(thread.id, "thread-child");
        assert_eq!(thread.cwd, PathBuf::from("/worktrees/fix-login"));
        assert_eq!(thread.name.as_deref(), Some("fix/login"));
        assert_eq!(thread.forked_from_id.as_deref(), Some("thread-parent"));
        assert_eq!(
            thread.status,
            CodexThreadStatus::Active {
                active_flags: vec![
                    "waitingOnApproval".to_owned(),
                    "waitingOnUserInput".to_owned(),
                ]
            }
        );
    }

    #[test]
    fn accepts_a_metadata_response_with_empty_native_history() {
        let response = json!({
            "thread": {
                "id": "thread-1",
                "cwd": "/worktree",
                "status": {"type": "idle"},
                "turns": []
            }
        });
        let thread = decode_thread_read_response(response).unwrap();
        assert_eq!(thread.status, CodexThreadStatus::Idle);
    }

    #[test]
    fn rejects_a_response_without_required_native_thread_fields() {
        let error = decode_thread_read_response(json!({
            "thread": {"id": "thread-1", "turns": []}
        }))
        .unwrap_err();
        assert!(error.to_string().contains("invalid thread read response"));
    }

    #[test]
    fn locates_active_and_archived_threads_from_the_exact_read_path() {
        for (path, archived) in [
            ("/codex/sessions/2026/09/rollout.jsonl", false),
            (
                "/codex/archived_sessions/rollout-2026-09-thread.jsonl",
                true,
            ),
        ] {
            let located = decode_located_thread_response(json!({
                "thread": {
                    "id": "thread-1",
                    "cwd": "/worktree",
                    "path": path,
                    "status": {"type": "notLoaded"}
                }
            }))
            .unwrap();
            assert_eq!(located.thread.id, "thread-1");
            assert_eq!(located.archived, archived);
        }
    }

    #[test]
    fn refuses_to_guess_archive_state_without_a_rollout_path() {
        let error = decode_located_thread_response(json!({
            "thread": {
                "id": "thread-1",
                "cwd": "/worktree",
                "path": null,
                "status": {"type": "notLoaded"}
            }
        }))
        .unwrap_err();
        assert!(error.to_string().contains("rollout path"));
    }

    #[test]
    fn decodes_optional_native_thread_cost_without_account_wide_fields() {
        let estimate = decode_thread_cost_response(
            json!({
                "summary": {"lifetimeTokens": 99},
                "dailyUsageBuckets": [],
                "threadUsage": {
                    "threadId": "thread-1",
                    "estimatedUsageCreditsMicros": 1_250_000,
                    "estimatedUsageUsdMicros": 420_000,
                    "groups": [{
                        "model": "gpt-test",
                        "reasoningEffort": "high",
                        "speed": "fast",
                        "estimatedUsageCreditsMicros": 1_250_000,
                        "netNewInputTokens": 100,
                        "cachedInputTokens": 50,
                        "inputTokens": 150,
                        "outputTokens": 25,
                        "totalTokens": 175
                    }]
                }
            }),
            "thread-1",
        )
        .unwrap()
        .unwrap();
        assert_eq!(estimate.estimated_usage_credits_micros, 1_250_000);
        assert_eq!(estimate.estimated_usage_usd_micros, Some(420_000));
        assert_eq!(estimate.groups[0].total_tokens, Some(175));

        assert_eq!(
            decode_thread_cost_response(json!({"summary": {}, "threadUsage": null}), "thread-1")
                .unwrap(),
            None
        );
    }

    #[test]
    fn rejects_a_cost_result_for_another_thread() {
        let error = decode_thread_cost_response(
            json!({
                "summary": {},
                "threadUsage": {
                    "threadId": "thread-other",
                    "estimatedUsageCreditsMicros": 0,
                    "groups": []
                }
            }),
            "thread-1",
        )
        .unwrap_err();
        assert!(error.to_string().contains("thread ID mismatch"));
    }
}
