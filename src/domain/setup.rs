use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Conservative alias check: native tool namespaces normalize punctuation.
/// Prefer rejection to accidentally selecting a different server's tools.
pub(crate) fn worker_slot_key(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

/// A local, immutable source reference. Never contains the executable recipe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkerMcpReference {
    pub path: PathBuf,
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum WorkspaceCreationState {
    /// No scoped receipt. This is not evidence that no side effect occurred.
    Unknown,
    /// Intent is durable; completion has not been confirmed. Never auto-replay.
    Preparing,
    Configured,
    Rejected,
    Failed,
    Deleted,
    /// An older/private creation has no public request evidence.
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CreationProfileEvidence {
    pub name: String,
    pub source_path: Option<PathBuf>,
    pub source_sha256: String,
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspaceCreationReceipt {
    pub operation_id: String,
    pub request_sha256: Option<String>,
    pub state: WorkspaceCreationState,
    pub repository_id: String,
    pub repository_path: PathBuf,
    pub workspace_id: Option<String>,
    pub workspace_name: Option<String>,
    pub base_sha: Option<String>,
    pub worktree_path: Option<PathBuf>,
    pub branch_name: Option<String>,
    pub profile: Option<CreationProfileEvidence>,
    pub worker_mcp: Option<WorkerMcpReference>,
    pub codex_thread_id: Option<String>,
    pub error_code: Option<String>,
    pub created_at_ms: Option<i64>,
    pub updated_at_ms: Option<i64>,
}

impl WorkspaceCreationReceipt {
    pub(crate) fn unknown(operation_id: String, repository_id: String, path: PathBuf) -> Self {
        Self {
            operation_id,
            request_sha256: None,
            state: WorkspaceCreationState::Unknown,
            repository_id,
            repository_path: path,
            workspace_id: None,
            workspace_name: None,
            base_sha: None,
            worktree_path: None,
            branch_name: None,
            profile: None,
            worker_mcp: None,
            codex_thread_id: None,
            error_code: None,
            created_at_ms: None,
            updated_at_ms: None,
        }
    }
}
