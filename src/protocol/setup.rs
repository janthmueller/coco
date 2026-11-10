use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{WorkspaceChangesRequest, WorkspaceWorktreeRequest};
pub(crate) use crate::domain::setup::{WorkerMcpReference, WorkspaceCreationReceipt};

/// Public setup keeps a fixed repository scope and permanent lifetime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspaceSetupRequest {
    pub operation_id: String,
    pub name: String,
    pub worktree: WorkspaceWorktreeRequest,
    pub changes: WorkspaceChangesRequest,
    pub profile: String,
    #[serde(default, skip_serializing_if = "WorkspaceSetupContext::is_fresh")]
    pub context: WorkspaceSetupContext,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub worker_mcp: Option<WorkerMcpReference>,
}

/// Raw native IDs and cross-repository context remain CLI-only. Public hosts
/// cannot escape their repository grant through a context source UUID.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum WorkspaceSetupContext {
    #[default]
    Fresh,
    Fork {
        workspace: String,
        #[serde(default)]
        compact: bool,
    },
}

impl WorkspaceSetupContext {
    fn is_fresh(&self) -> bool {
        matches!(self, Self::Fresh)
    }
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
