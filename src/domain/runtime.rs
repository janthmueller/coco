use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkspaceRuntimeBackend {
    ExecServer,
}

/// Lifecycle of the executor process owned by one workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkspaceRuntimeState {
    Inactive,
    Running,
    Exited,
}

/// Boundary covered by one resource observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkspaceResourceScope {
    ProcessTree,
    RootProcess,
}

/// Ephemeral resource observation for a workspace-owned execution process.
///
/// This is never persisted. Optional measurements stay absent when the host
/// cannot attribute them without overstating the available evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspaceRuntimeResources {
    pub(crate) backend: WorkspaceRuntimeBackend,
    pub(crate) state: WorkspaceRuntimeState,
    pub(crate) scope: WorkspaceResourceScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) process_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) process_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) resident_memory_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) cpu_percent: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) sampled_at_ms: Option<i64>,
}
