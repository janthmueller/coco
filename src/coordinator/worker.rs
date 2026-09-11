use std::error::Error as StdError;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_json::Value;
use thiserror::Error;

use crate::domain::runtime::{
    WorkspaceResourceCapabilities, WorkspaceResourceControllerStatus,
    WorkspaceResourcePolicySnapshot, WorkspaceRuntimeResources, WorkspaceRuntimeState,
};
use crate::domain::usage::NativeThreadCostEstimate;
use crate::domain::{CodexModel, CodexThreadStatus};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StartedThread {
    pub(crate) id: String,
    pub(crate) status: CodexThreadStatus,
    pub(crate) cwd: PathBuf,
    pub(crate) response: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StartedTurn {
    pub(crate) id: String,
}

/// Opaque execution placement selected by the worker adapter for one CoCo
/// workspace. The coordinator only carries this to trusted interactive
/// clients; Codex-specific registration and process ownership stay in the
/// daemon adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkerExecutionEnvironment {
    pub(crate) environment_id: String,
    pub(crate) cwd: PathBuf,
    pub(crate) runtime_workspace_roots: Vec<PathBuf>,
}

/// Stable subset of a native Codex thread used to hydrate CoCo projections.
///
/// App Server wire payloads stay in the daemon adapter. In particular, callers
/// cannot accidentally make storage or public protocol types depend on the
/// complete native response shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeThread {
    pub(crate) id: String,
    pub(crate) cwd: PathBuf,
    pub(crate) name: Option<String>,
    pub(crate) status: CodexThreadStatus,
    pub(crate) forked_from_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocatedNativeThread {
    pub(crate) thread: NativeThread,
    pub(crate) archived: bool,
}

#[derive(Debug, Error)]
pub(crate) enum WorkerError {
    #[error(transparent)]
    Runtime(Box<dyn StdError + Send + Sync>),
    #[error("Codex response is missing required field {0}")]
    InvalidResponse(&'static str),
    #[error("Codex response contains an invalid native thread status: {0}")]
    InvalidThreadStatus(String),
    #[error("Codex returned an invalid model catalog: {0}")]
    InvalidModelCatalog(String),
    #[error("Codex returned an invalid thread read response: {0}")]
    InvalidThreadRead(String),
    #[error("Codex returned an invalid thread usage response: {0}")]
    InvalidThreadUsage(String),
    #[error("Codex thread ID mismatch: expected {expected}, received {actual}")]
    ThreadIdMismatch { expected: String, actual: String },
    #[error("Codex thread cwd mismatch: expected {expected}, received {actual}")]
    CwdMismatch { expected: PathBuf, actual: PathBuf },
    #[error("workspace resource limits are unsupported by the selected execution backend: {fields}", fields = .fields.join(", "))]
    ResourcePolicyUnsupported { fields: Vec<&'static str> },
}

impl WorkerError {
    pub(crate) fn runtime(source: impl StdError + Send + Sync + 'static) -> Self {
        Self::Runtime(Box::new(source))
    }
}

#[async_trait]
pub(crate) trait WorkerRuntime: Send + Sync + 'static {
    async fn list_models(&self) -> Result<Vec<CodexModel>, WorkerError>;

    /// Reads native thread truth without loading it or applying configuration.
    ///
    /// The default keeps existing test and alternate worker implementations
    /// source-compatible while the read path is introduced behind this port.
    async fn read_thread(&self, _thread_id: &str) -> Result<NativeThread, WorkerError> {
        Err(WorkerError::InvalidThreadRead(
            "thread/read is not supported by this worker".to_owned(),
        ))
    }

    /// Reads an optional backend-owned billing estimate without loading the
    /// native thread or starting its workspace executor.
    async fn read_thread_cost(
        &self,
        _thread_id: &str,
    ) -> Result<Option<NativeThreadCostEstimate>, WorkerError> {
        Ok(None)
    }

    /// Finds a thread only after Codex has materialized durable history for it.
    /// A newly started empty thread must return `None` even while it is loaded.
    async fn find_materialized_thread(
        &self,
        thread_id: &str,
        _cwd: &Path,
    ) -> Result<Option<NativeThread>, WorkerError> {
        self.read_thread(thread_id).await.map(Some)
    }

    async fn set_thread_name(&self, thread_id: &str, name: &str) -> Result<(), WorkerError>;

    /// Locates a persisted thread without loading it, including archived storage.
    async fn locate_thread(
        &self,
        thread_id: &str,
    ) -> Result<Option<LocatedNativeThread>, WorkerError>;

    /// Lists every native descendant because archive/delete cascade in Codex.
    async fn list_thread_descendants(&self, thread_id: &str) -> Result<Vec<String>, WorkerError>;

    async fn background_terminal_count(&self, thread_id: &str) -> Result<usize, WorkerError>;

    async fn unsubscribe_thread(&self, thread_id: &str) -> Result<(), WorkerError>;

    async fn archive_thread(&self, thread_id: &str) -> Result<(), WorkerError>;

    async fn unarchive_thread(&self, thread_id: &str) -> Result<NativeThread, WorkerError>;

    async fn delete_thread(&self, thread_id: &str) -> Result<(), WorkerError>;

    /// Lazily prepares an execution environment for a workspace. Workers that
    /// execute in their control-plane process retain the legacy `None`
    /// behavior.
    async fn prepare_workspace_execution(
        &self,
        _workspace_id: &str,
        _cwd: &Path,
    ) -> Result<Option<WorkerExecutionEnvironment>, WorkerError> {
        Ok(None)
    }

    /// Stops an execution boundary owned by one workspace. Implementations
    /// without per-workspace runtimes have nothing to stop.
    async fn stop_workspace_execution(&self, _workspace_id: &str) -> Result<(), WorkerError> {
        Ok(())
    }

    /// Returns an ephemeral observation without starting an inactive runtime.
    async fn workspace_resources(
        &self,
        _workspace_id: &str,
    ) -> Result<Option<WorkspaceRuntimeResources>, WorkerError> {
        Ok(None)
    }

    fn workspace_resource_capabilities(&self) -> WorkspaceResourceCapabilities {
        WorkspaceResourceCapabilities::unavailable()
    }

    async fn workspace_resource_policy_status(
        &self,
        _workspace_id: &str,
    ) -> Result<WorkspaceResourceControllerStatus, WorkerError> {
        Ok(WorkspaceResourceControllerStatus {
            capabilities: self.workspace_resource_capabilities(),
            runtime_state: WorkspaceRuntimeState::Inactive,
            applied_policy: None,
        })
    }

    async fn configure_workspace_resource_policy(
        &self,
        workspace_id: &str,
        snapshot: WorkspaceResourcePolicySnapshot,
    ) -> Result<WorkspaceResourceControllerStatus, WorkerError> {
        let unsupported = self
            .workspace_resource_capabilities()
            .unsupported_fields(&snapshot.policy);
        if !unsupported.is_empty() {
            return Err(WorkerError::ResourcePolicyUnsupported {
                fields: unsupported,
            });
        }
        self.workspace_resource_policy_status(workspace_id).await
    }

    async fn start_thread(
        &self,
        workspace_id: &str,
        name: &str,
        cwd: &Path,
        config: Value,
        model: Option<&str>,
    ) -> Result<StartedThread, WorkerError>;

    async fn fork_thread(
        &self,
        workspace_id: &str,
        name: &str,
        source_thread_id: &str,
        cwd: &Path,
        config: Value,
        model: Option<&str>,
    ) -> Result<StartedThread, WorkerError>;

    /// Requests compaction. Completion is observed through App Server events
    /// by the coordinator rather than inferred from this immediate response.
    async fn compact_thread(&self, thread_id: &str) -> Result<(), WorkerError>;

    async fn resume_thread(
        &self,
        workspace_id: &str,
        thread_id: &str,
        cwd: &Path,
        config: Value,
        model: Option<&str>,
    ) -> Result<StartedThread, WorkerError>;

    async fn start_turn(
        &self,
        workspace_id: &str,
        thread_id: &str,
        cwd: &Path,
        client_message_id: &str,
        message: &str,
        additional_context: Option<Value>,
    ) -> Result<StartedTurn, WorkerError>;

    async fn respond_to_request(&self, id: Value, result: Value) -> Result<(), WorkerError>;
}

impl super::Coordinator {
    pub(crate) async fn list_models(&self) -> Result<Vec<CodexModel>, super::CoordinatorError> {
        Ok(self.worker.list_models().await?)
    }
}
