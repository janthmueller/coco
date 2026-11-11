use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub(crate) use crate::domain::retirement::{
    WorkspaceDeletionReceipt, WorkspaceRetirementPlan, WorkspaceThreadDisposition,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspaceDeleteRequest {
    pub operation_id: String,
    pub workspace: String,
    #[serde(default)]
    pub keep_thread: bool,
    #[serde(default)]
    pub keep_branch: bool,
    #[serde(default)]
    pub discard_changes: bool,
    #[serde(default)]
    pub discard_unretained_commits: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspaceDeleteOperationParams {
    pub repository_path: PathBuf,
    pub request: WorkspaceDeleteRequest,
    #[serde(default)]
    pub dry_run: bool,
    /// A precondition, not a new intent or authorization to retry dispatch.
    #[serde(default)]
    pub expected_plan_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspaceDeletionGetParams {
    pub repository_path: PathBuf,
    pub operation_id: String,
}

impl super::DaemonRequest for WorkspaceDeleteOperationParams {
    type Response = WorkspaceDeletionReceipt;
    const METHOD: super::DaemonMethod = super::DaemonMethod::WorkspaceDeleteOperation;
}

impl super::DaemonRequest for WorkspaceDeletionGetParams {
    type Response = WorkspaceDeletionReceipt;
    const METHOD: super::DaemonMethod = super::DaemonMethod::WorkspaceDeletionGet;
}
