use std::path::PathBuf;

use anyhow::Context;
use async_trait::async_trait;
use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Implementation, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tracing::warn;
use uuid::Uuid;

use crate::domain::AuditOutcome;
use crate::paths::CocoPaths;
use crate::protocol::{
    AuditRecordParams, DaemonRequest, RepositoryScope, TurnStartParams, WorkspaceDiffParams,
    WorkspaceGetParams, WorkspaceListParams,
};
use crate::rpc::{RpcClient, RpcClientError};

const WORKSPACES_LIST: &str = "workspaces.list";
const WORKSPACES_STATUS: &str = "workspaces.status";
const WORKSPACES_DIFF: &str = "workspaces.diff";
const WORKSPACES_SEND: &str = "workspaces.send";
const MAX_ERROR_CHARS: usize = 1_024;

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkspacesListInput {
    /// Only return workspaces whose phase is one of these values.
    phases: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkspaceStatusInput {
    /// Repository-local workspace name or globally unique workspace ID.
    workspace: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkspaceDiffInput {
    /// Repository-local workspace name or globally unique workspace ID.
    workspace: String,
    /// Optional upper bound for returned diff output, in bytes.
    max_bytes: Option<u64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkspaceSendInput {
    /// Repository-local workspace name or globally unique workspace ID.
    workspace: String,
    /// Text to send as the next turn.
    message: String,
    /// Idempotency key. CoCo generates one when omitted.
    operation_id: Option<String>,
}

#[derive(Clone)]
struct McpServer {
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "the tool_handler macro reads this router field")
    )]
    tool_router: ToolRouter<Self>,
    dispatcher: Dispatcher<RpcClient>,
}

impl McpServer {
    fn new(repository: PathBuf, allow_send: bool, socket_path: PathBuf) -> Self {
        let mut tool_router = Self::tool_router();
        if !allow_send {
            tool_router.disable_route(WORKSPACES_SEND);
        }
        Self {
            tool_router,
            dispatcher: Dispatcher::new(RpcClient::new(socket_path), repository),
        }
    }
}

#[tool_router]
impl McpServer {
    #[tool(
        name = "workspaces.list",
        description = "List CoCo workspaces in the configured repository.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn workspaces_list(
        &self,
        Parameters(input): Parameters<WorkspacesListInput>,
    ) -> CallToolResult {
        self.dispatcher.workspaces_list(input).await
    }

    #[tool(
        name = "workspaces.status",
        description = "Show one CoCo workspace and its workspace status.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn workspaces_status(
        &self,
        Parameters(input): Parameters<WorkspaceStatusInput>,
    ) -> CallToolResult {
        self.dispatcher.workspaces_status(input).await
    }

    #[tool(
        name = "workspaces.diff",
        description = "Show bounded tracked and untracked changes for one CoCo workspace.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn workspaces_diff(
        &self,
        Parameters(input): Parameters<WorkspaceDiffInput>,
    ) -> CallToolResult {
        self.dispatcher.workspaces_diff(input).await
    }

    #[tool(
        name = "workspaces.send",
        description = "Start another turn for one CoCo workspace.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn workspaces_send(
        &self,
        Parameters(input): Parameters<WorkspaceSendInput>,
    ) -> CallToolResult {
        self.dispatcher.workspaces_send(input).await
    }
}

#[tool_handler]
impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("coco", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Repository-scoped CoCo workspace inspection. workspaces.send is available only when explicitly enabled by the operator.",
            )
    }
}

#[derive(Clone)]
struct Dispatcher<C> {
    client: C,
    repository: PathBuf,
}

impl<C> Dispatcher<C>
where
    C: DaemonRpc,
{
    fn new(client: C, repository: PathBuf) -> Self {
        Self { client, repository }
    }

    async fn workspaces_list(&self, input: WorkspacesListInput) -> CallToolResult {
        self.call(
            WORKSPACES_LIST,
            WorkspaceListParams {
                scope: RepositoryScope::repository(self.repository.clone()),
                phases: input.phases,
            },
            None,
            None,
        )
        .await
    }

    async fn workspaces_status(&self, input: WorkspaceStatusInput) -> CallToolResult {
        let workspace = input.workspace;
        self.call(
            WORKSPACES_STATUS,
            WorkspaceGetParams {
                scope: RepositoryScope::repository(self.repository.clone()),
                workspace: workspace.clone(),
            },
            Some(workspace),
            None,
        )
        .await
    }

    async fn workspaces_diff(&self, input: WorkspaceDiffInput) -> CallToolResult {
        let workspace = input.workspace;
        self.call(
            WORKSPACES_DIFF,
            WorkspaceDiffParams {
                scope: RepositoryScope::repository(self.repository.clone()),
                workspace: workspace.clone(),
                max_bytes: input.max_bytes,
            },
            Some(workspace),
            None,
        )
        .await
    }

    async fn workspaces_send(&self, input: WorkspaceSendInput) -> CallToolResult {
        let operation_id = input
            .operation_id
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let workspace = input.workspace;
        self.call(
            WORKSPACES_SEND,
            TurnStartParams {
                scope: RepositoryScope::repository(self.repository.clone()),
                workspace: workspace.clone(),
                message: input.message,
                operation_id: operation_id.clone(),
            },
            Some(workspace),
            Some(operation_id),
        )
        .await
    }

    async fn call<R>(
        &self,
        action: &'static str,
        request: R,
        workspace_id: Option<String>,
        operation_id: Option<String>,
    ) -> CallToolResult
    where
        R: DaemonRequest + Send,
        R::Response: Send,
    {
        let response = self.client.request(request).await.and_then(|result| {
            serde_json::to_value(result)
                .map_err(|_| DaemonFailure::new("INTERNAL", "could not encode the daemon response"))
        });
        self.audit(
            action,
            workspace_id.as_deref(),
            operation_id.as_deref(),
            &response,
        )
        .await;

        match response {
            Ok(value) => CallToolResult::structured(value),
            Err(error) => CallToolResult::structured_error(error.as_json()),
        }
    }

    async fn audit(
        &self,
        action: &'static str,
        workspace_id: Option<&str>,
        operation_id: Option<&str>,
        response: &Result<Value, DaemonFailure>,
    ) {
        let mut details = json!({"repositoryPath": self.repository});
        if let Err(error) = response {
            details["error"] = Value::String(error.code.clone());
        }

        if let Err(error) = self
            .client
            .request(AuditRecordParams {
                source: "mcp".to_owned(),
                action: action.to_owned(),
                workspace_id: workspace_id.map(ToOwned::to_owned),
                operation_id: operation_id.map(ToOwned::to_owned),
                outcome: if response.is_ok() {
                    AuditOutcome::Succeeded
                } else {
                    AuditOutcome::Failed
                },
                details,
            })
            .await
        {
            warn!(
                action,
                error_code = %error.code,
                "could not record MCP audit event"
            );
        }
    }
}

#[async_trait]
trait DaemonRpc: Clone + Send + Sync + 'static {
    async fn request<R>(&self, request: R) -> Result<R::Response, DaemonFailure>
    where
        R: DaemonRequest + Send,
        R::Response: Send;
}

#[async_trait]
impl DaemonRpc for RpcClient {
    async fn request<R>(&self, request: R) -> Result<R::Response, DaemonFailure>
    where
        R: DaemonRequest + Send,
        R::Response: Send,
    {
        RpcClient::request(self, request)
            .await
            .map_err(DaemonFailure::from)
    }
}

#[derive(Debug, Clone)]
struct DaemonFailure {
    code: String,
    message: String,
}

impl DaemonFailure {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: bounded(message.into()),
        }
    }

    fn as_json(&self) -> Value {
        json!({
            "error": {
                "code": self.code,
                "message": self.message,
            }
        })
    }
}

impl From<RpcClientError> for DaemonFailure {
    fn from(error: RpcClientError) -> Self {
        match error {
            RpcClientError::Remote(error) => Self::new(error.code, error.message),
            RpcClientError::Connect { .. }
            | RpcClientError::ClosedWithoutResponse
            | RpcClientError::Io(_) => {
                Self::new("DAEMON_UNAVAILABLE", "could not communicate with cocod")
            }
            RpcClientError::MessageTooLarge
            | RpcClientError::MismatchedId { .. }
            | RpcClientError::MalformedResponse
            | RpcClientError::Json(_) => {
                Self::new("INTERNAL", "cocod returned an invalid response")
            }
        }
    }
}

fn bounded(message: String) -> String {
    let mut characters = message.chars();
    let prefix: String = characters.by_ref().take(MAX_ERROR_CHARS).collect();
    if characters.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
}

pub async fn run_from_env(repository: PathBuf, allow_send: bool) -> anyhow::Result<()> {
    let paths = CocoPaths::from_env()?;
    let repository = if repository.is_absolute() {
        repository
    } else {
        std::env::current_dir()
            .context("could not determine current directory")?
            .join(repository)
    };
    serve(repository, allow_send, paths.socket_path).await
}

pub(crate) async fn serve(
    repository: PathBuf,
    allow_send: bool,
    socket_path: PathBuf,
) -> anyhow::Result<()> {
    let service = McpServer::new(repository, allow_send, socket_path)
        .serve(rmcp::transport::stdio())
        .await
        .context("could not start the CoCo MCP stdio server")?;
    service
        .waiting()
        .await
        .context("CoCo MCP stdio server stopped with an error")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::protocol::DaemonMethod;

    #[derive(Clone, Default)]
    struct RecordingDaemon {
        calls: Arc<Mutex<Vec<(String, Value)>>>,
        failures: Arc<Mutex<HashMap<String, DaemonFailure>>>,
    }

    impl RecordingDaemon {
        fn fail(&self, method: &str, error: DaemonFailure) {
            self.failures
                .lock()
                .unwrap()
                .insert(method.to_owned(), error);
        }

        fn calls(&self) -> Vec<(String, Value)> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl DaemonRpc for RecordingDaemon {
        async fn request<R>(&self, request: R) -> Result<R::Response, DaemonFailure>
        where
            R: DaemonRequest + Send,
            R::Response: Send,
        {
            let method = R::METHOD.as_str();
            let params = serde_json::to_value(request).unwrap();
            self.calls.lock().unwrap().push((method.to_owned(), params));
            if let Some(error) = self.failures.lock().unwrap().get(method).cloned() {
                Err(error)
            } else {
                serde_json::from_value(fake_response(R::METHOD)).map_err(|error| {
                    DaemonFailure::new("TEST_RESPONSE", format!("invalid fake response: {error}"))
                })
            }
        }
    }

    fn fake_response(method: DaemonMethod) -> Value {
        match method {
            DaemonMethod::Health => json!({"status": "ok"}),
            DaemonMethod::RepositoryRegister => json!({
                "id": "repo-test",
                "rootPath": "/repo",
                "gitCommonDir": "/repo/.git",
                "displayName": "repo",
                "isLinkedWorktree": false,
                "createdAtMs": 1,
                "updatedAtMs": 1,
            }),
            DaemonMethod::RepositoryList => json!([]),
            DaemonMethod::WorkspaceCreate => json!({"workspace": fake_workspace()}),
            DaemonMethod::WorkspaceList => json!([]),
            DaemonMethod::WorkspaceGet => json!({
                "workspace": fake_workspace(),
                "git": {"observed": false, "reason": "test fixture"},
                "nextSequence": 0,
            }),
            DaemonMethod::TurnStart => json!({
                "workspace": fake_workspace(),
                "turnId": "turn-test",
                "codexTurnId": "codex-turn-test",
            }),
            DaemonMethod::EventList => json!({
                "workspace": fake_workspace(),
                "events": [],
                "nextSequence": 0,
            }),
            DaemonMethod::WorkspaceDiff => json!({
                "patch": "",
                "patchTruncated": false,
                "untrackedPaths": [],
            }),
            DaemonMethod::AuditRecord => json!({
                "sequence": 1,
                "id": "audit-test",
                "source": "mcp",
                "action": "test",
                "workspaceId": null,
                "operationId": null,
                "outcome": "succeeded",
                "details": {},
                "occurredAtMs": 1,
            }),
        }
    }

    fn fake_workspace() -> Value {
        json!({
            "id": "workspace-test",
            "createOperationId": "create-test",
            "repositoryId": "repo-test",
            "name": "workspace",
            "contextMode": "fresh",
            "context": {},
            "profile": {
                "name": "default",
                "sourcePath": null,
                "sourceHash": "test",
                "effectiveSettings": {},
            },
            "lifecycle": "ready",
            "threadRuntime": {
                "status": {"type": "idle"},
                "runtimeGeneration": "runtime-test",
                "observedAtMs": 1,
                "isFresh": true,
            },
            "phase": "idle",
            "waitReasons": [],
            "branchName": "coco/workspace",
            "baseSha": "base-test",
            "worktreePath": "/worktree",
            "codexThreadId": "thread-test",
            "parentThreadId": null,
            "activeTurnId": null,
            "lastErrorCode": null,
            "lastErrorMessage": null,
            "createdAtMs": 1,
            "updatedAtMs": 1,
            "completedAtMs": null,
        })
    }

    #[test]
    fn workspaces_send_is_only_exposed_when_enabled() {
        let socket = PathBuf::from("/tmp/cocod-test.sock");
        let read_only = McpServer::new(PathBuf::from("/repo"), false, socket.clone());
        let writable = McpServer::new(PathBuf::from("/repo"), true, socket);

        let read_only_names: Vec<_> = read_only
            .tool_router
            .list_all()
            .into_iter()
            .map(|tool| tool.name.into_owned())
            .collect();
        assert_eq!(
            read_only_names,
            vec![WORKSPACES_DIFF, WORKSPACES_LIST, WORKSPACES_STATUS]
        );
        assert!(!read_only.tool_router.has_route(WORKSPACES_SEND));

        let writable_names: Vec<_> = writable
            .tool_router
            .list_all()
            .into_iter()
            .map(|tool| tool.name.into_owned())
            .collect();
        assert_eq!(
            writable_names,
            vec![
                WORKSPACES_DIFF,
                WORKSPACES_LIST,
                WORKSPACES_SEND,
                WORKSPACES_STATUS,
            ]
        );
        assert!(writable.tool_router.has_route(WORKSPACES_SEND));
    }

    #[tokio::test]
    async fn maps_all_tools_to_repository_scoped_wire_calls() {
        let daemon = RecordingDaemon::default();
        let dispatcher = Dispatcher::new(daemon.clone(), PathBuf::from("/fixed/repository"));

        dispatcher
            .workspaces_list(WorkspacesListInput {
                phases: Some(vec!["running".into(), "idle".into()]),
            })
            .await;
        dispatcher
            .workspaces_status(WorkspaceStatusInput {
                workspace: "workspace-one".into(),
            })
            .await;
        dispatcher
            .workspaces_diff(WorkspaceDiffInput {
                workspace: "workspace-two".into(),
                max_bytes: Some(4_096),
            })
            .await;
        dispatcher
            .workspaces_send(WorkspaceSendInput {
                workspace: "workspace-three".into(),
                message: "private prompt text".into(),
                operation_id: Some("operation-7".into()),
            })
            .await;

        let calls = daemon.calls();
        assert_eq!(calls.len(), 8);
        assert_eq!(calls[0].0, "workspace.list");
        assert_eq!(
            calls[0].1,
            json!({
                "scope": {"kind": "repository", "path": "/fixed/repository"},
                "phases": ["running", "idle"]
            })
        );
        assert_eq!(calls[2].0, "workspace.get");
        assert_eq!(
            calls[2].1,
            json!({
                "scope": {"kind": "repository", "path": "/fixed/repository"},
                "workspace": "workspace-one"
            })
        );
        assert_eq!(calls[4].0, "workspace.diff");
        assert_eq!(
            calls[4].1,
            json!({
                "scope": {"kind": "repository", "path": "/fixed/repository"},
                "workspace": "workspace-two",
                "maxBytes": 4_096
            })
        );
        assert_eq!(calls[6].0, "turn.start");
        assert_eq!(
            calls[6].1,
            json!({
                "scope": {"kind": "repository", "path": "/fixed/repository"},
                "workspace": "workspace-three",
                "message": "private prompt text",
                "operationId": "operation-7"
            })
        );

        for audit_index in [1, 3, 5, 7] {
            assert_eq!(calls[audit_index].0, "audit.record");
            assert_eq!(
                calls[audit_index].1.pointer("/details/repositoryPath"),
                Some(&json!("/fixed/repository"))
            );
            assert_eq!(
                calls[audit_index].1.get("outcome"),
                Some(&json!("succeeded"))
            );
        }
        assert_eq!(calls[7].1.get("source"), Some(&json!("mcp")));
        assert_eq!(calls[7].1.get("action"), Some(&json!(WORKSPACES_SEND)));
        assert_eq!(
            calls[7].1.get("workspaceId"),
            Some(&json!("workspace-three"))
        );
        assert_eq!(calls[7].1.get("operationId"), Some(&json!("operation-7")));
        assert!(!calls[7].1.to_string().contains("private prompt text"));
    }

    #[tokio::test]
    async fn generates_send_operation_id_and_reuses_it_for_audit() {
        let daemon = RecordingDaemon::default();
        let dispatcher = Dispatcher::new(daemon.clone(), PathBuf::from("/repo"));

        dispatcher
            .workspaces_send(WorkspaceSendInput {
                workspace: "workspace".into(),
                message: "message".into(),
                operation_id: None,
            })
            .await;

        let calls = daemon.calls();
        let operation_id = calls[0].1["operationId"].as_str().unwrap();
        Uuid::parse_str(operation_id).unwrap();
        assert_eq!(calls[1].1["operationId"], operation_id);
        assert!(!calls[1].1.to_string().contains("message"));
    }

    #[tokio::test]
    async fn returns_structured_errors_and_audits_only_the_safe_code() {
        let daemon = RecordingDaemon::default();
        daemon.fail(
            "workspace.get",
            DaemonFailure::new("WORKSPACE_NOT_FOUND", "workspace does not exist"),
        );
        let dispatcher = Dispatcher::new(daemon.clone(), PathBuf::from("/repo"));

        let result = dispatcher
            .workspaces_status(WorkspaceStatusInput {
                workspace: "missing".into(),
            })
            .await;

        assert_eq!(result.is_error, Some(true));
        assert_eq!(
            result.structured_content,
            Some(json!({
                "error": {
                    "code": "WORKSPACE_NOT_FOUND",
                    "message": "workspace does not exist"
                }
            }))
        );
        assert_eq!(result.content.len(), 1);

        let calls = daemon.calls();
        assert_eq!(calls[1].0, "audit.record");
        assert_eq!(calls[1].1["outcome"], "failed");
        assert_eq!(calls[1].1["details"]["error"], "WORKSPACE_NOT_FOUND");
        assert!(!calls[1].1.to_string().contains("workspace does not exist"));
    }

    #[tokio::test]
    async fn audit_failure_does_not_replace_a_successful_tool_result() {
        let daemon = RecordingDaemon::default();
        daemon.fail(
            "audit.record",
            DaemonFailure::new("INTERNAL", "audit unavailable"),
        );
        let dispatcher = Dispatcher::new(daemon, PathBuf::from("/repo"));

        let result = dispatcher
            .workspaces_list(WorkspacesListInput::default())
            .await;

        assert_eq!(result.is_error, Some(false));
        assert_eq!(result.structured_content, Some(json!([])));
    }
}
