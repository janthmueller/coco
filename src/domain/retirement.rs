use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum WorkspaceThreadDisposition {
    Retain,
    Archive,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspaceRetirementPlan {
    pub workspace_id: String,
    pub workspace_name: String,
    pub worktree_path: PathBuf,
    pub remove_worktree: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_sha: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    pub thread_disposition: WorkspaceThreadDisposition,
    pub delete_branch: bool,
    pub tracked_changes: bool,
    pub untracked_file_count: usize,
    pub ignored_file_count: usize,
    pub detached_commits: bool,
    pub unretained_commit_count: usize,
    pub descendant_thread_count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blockers: Vec<String>,
}

impl WorkspaceRetirementPlan {
    pub const fn has_local_changes(&self) -> bool {
        self.tracked_changes || self.untracked_file_count > 0 || self.ignored_file_count > 0
    }

    pub(crate) fn sha256(&self) -> serde_json::Result<String> {
        Ok(hex::encode(Sha256::digest(serde_json::to_vec(self)?)))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum WorkspaceDeletionState {
    Unknown,
    /// Read-only checked plan; no durable claim or deletion has occurred.
    Preview,
    /// A single caller owns dispatch; cancellation never grants another try.
    Preparing,
    /// The existing recoverable retirement saga accepted this exact operation.
    Deleting,
    Deleted,
    Rejected,
    /// No confirmed complete result. Some external effects may have happened.
    Uncertain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspaceDeletionReceipt {
    pub operation_id: String,
    pub state: WorkspaceDeletionState,
    pub repository_id: String,
    pub repository_path: PathBuf,
    pub request_sha256: Option<String>,
    pub plan: Option<WorkspaceRetirementPlan>,
    pub plan_sha256: Option<String>,
    pub error_code: Option<String>,
    pub created_at_ms: Option<i64>,
    pub updated_at_ms: Option<i64>,
}

impl WorkspaceDeletionReceipt {
    pub(crate) fn unknown(operation_id: String, repository_id: String, path: PathBuf) -> Self {
        Self {
            operation_id,
            state: WorkspaceDeletionState::Unknown,
            repository_id,
            repository_path: path,
            request_sha256: None,
            plan: None,
            plan_sha256: None,
            error_code: None,
            created_at_ms: None,
            updated_at_ms: None,
        }
    }
}
