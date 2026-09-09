use std::path::PathBuf;
use std::sync::Arc;

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
use crate::domain::signals::SignalType;
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

mod signals;

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
    tool_router: ToolRouter<Self>,
    dispatcher: Dispatcher<RpcClient>,
    allowed_signals: Vec<String>,
    signal_catalog: Option<Arc<Vec<SignalType>>>,
}

impl McpServer {
    fn new(
        repository: PathBuf,
        allow_send: bool,
        socket_path: PathBuf,
        allowed_signals: Vec<String>,
    ) -> Self {
        let mut tool_router = Self::tool_router() + Self::signal_tool_router();
        if !allow_send {
            tool_router.disable_route(WORKSPACES_SEND);
        }
        if allowed_signals.is_empty() {
            tool_router.disable_route("signals.emit");
        }
        Self {
            tool_router,
            dispatcher: Dispatcher::new(RpcClient::new(socket_path), repository),
            allowed_signals,
            signal_catalog: None,
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

#[tool_handler(router = self.tool_router)]
impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("coco", env!("CARGO_PKG_VERSION")))
            .with_instructions(format!(
                "Repository-scoped CoCo inspection. Mutating tools require operator opt-in. signals.emit uses Codex thread metadata and records a claim without starting work or changing workspace state. Allowed signal grants (a bare name means version 1): {}.",
                if self.allowed_signals.is_empty() { "none".to_owned() } else { self.allowed_signals.join(", ") },
            ))
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

pub async fn run_from_env(
    repository: PathBuf,
    allow_send: bool,
    allowed_signals: Vec<String>,
    signal_catalog: Option<PathBuf>,
) -> anyhow::Result<()> {
    let paths = CocoPaths::from_env()?;
    serve(
        repository,
        allow_send,
        paths.socket_path,
        allowed_signals,
        signal_catalog,
    )
    .await
}

pub(crate) async fn serve(
    repository: PathBuf,
    allow_send: bool,
    socket_path: PathBuf,
    allowed_signals: Vec<String>,
    signal_catalog: Option<PathBuf>,
) -> anyhow::Result<()> {
    signals::validate_grants(&allowed_signals)?;
    anyhow::ensure!(
        allowed_signals.is_empty() || signal_catalog.is_some(),
        "--allow-emit requires an explicitly selected --signal-catalog directory"
    );
    let repository =
        std::path::absolute(repository).context("could not resolve repository path")?;
    let mut server = McpServer::new(repository, allow_send, socket_path, allowed_signals);
    if let Some(directory) = signal_catalog {
        server.load_signal_catalog(directory).await?;
    }
    let service = server
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
mod tests;
