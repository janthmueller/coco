use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{WorkspaceChangesRequest, WorkspaceWorktreeRequest};
pub(crate) use crate::domain::setup::{WorkerMcpReference, WorkspaceCreationReceipt};

/// The public setup surface deliberately supports fresh, permanent workspaces.
/// Existing CLI context and temporary-workspace controls remain independent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspaceSetupRequest {
    pub operation_id: String,
    pub name: String,
    pub worktree: WorkspaceWorktreeRequest,
    pub changes: WorkspaceChangesRequest,
    pub profile: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub worker_mcp: Option<WorkerMcpReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspaceSetupParams {
    pub repository_path: PathBuf,
    pub request: WorkspaceSetupRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspaceCreationGetParams {
    pub repository_path: PathBuf,
    pub operation_id: String,
}

impl super::DaemonRequest for WorkspaceSetupParams {
    type Response = WorkspaceCreationReceipt;
    const METHOD: super::DaemonMethod = super::DaemonMethod::WorkspaceSetup;
}

impl super::DaemonRequest for WorkspaceCreationGetParams {
    type Response = WorkspaceCreationReceipt;
    const METHOD: super::DaemonMethod = super::DaemonMethod::WorkspaceCreationGet;
}
