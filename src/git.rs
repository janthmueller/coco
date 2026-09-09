use std::io;
use std::path::PathBuf;

use serde::Serialize;
use thiserror::Error;

use crate::domain::WorktreeMode;

const DEFAULT_CAPTURE_LIMIT: usize = 16 * 1024 * 1024;

mod command;
mod diff;
mod local_state;
mod repository;
mod retirement;
mod worktree;

#[cfg(test)]
mod tests;

#[derive(Debug, Error)]
pub enum GitError {
    #[error("filesystem operation failed for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("git command `{category}` failed with status {status:?}: {stderr}")]
    CommandFailed {
        category: &'static str,
        status: Option<i32>,
        stderr: String,
    },
    #[error("git command `{category}` produced more than {limit} bytes")]
    OutputTooLarge {
        category: &'static str,
        limit: usize,
    },
    #[error("git command `{category}` returned non-UTF-8 metadata")]
    NonUtf8Metadata { category: &'static str },
    #[error("not a usable Git worktree: {0}")]
    NotAWorktree(PathBuf),
    #[error("repository checkout is dirty: {0}")]
    DirtyRepository(PathBuf),
    #[error("managed worktree gained local changes after the close check: {0}")]
    DirtyWorktree(PathBuf),
    #[error("untracked files cannot be carried into a workspace: {0}")]
    UntrackedChanges(PathBuf),
    #[error("local changes are based on {actual}, but the workspace base is {expected}")]
    LocalChangesBaseMismatch { expected: String, actual: String },
    #[error("unsafe carried-file path: {0}")]
    UnsafeCarriedPath(PathBuf),
    #[error("local-state selection contains more than {limit} files")]
    CarriedFileLimit { limit: usize },
    #[error("local-state selection contains more than {limit} bytes")]
    CarriedByteLimit { limit: usize },
    #[error("carried-file destination already exists: {0}")]
    CarriedDestinationExists(PathBuf),
    #[error(
        "invalid workspace name `{0}`; use 1-63 bytes of lowercase letters, digits, hyphens, and single slashes between components"
    )]
    InvalidWorkspaceName(String),
    #[error("branch namespace collision: requested {requested}, existing {existing}")]
    BranchCollision { requested: String, existing: String },
    #[error("local branch does not exist: {0}")]
    BranchNotFound(String),
    #[error("branch {branch} is already checked out at {path}")]
    BranchAlreadyCheckedOut { branch: String, path: PathBuf },
    #[error("worktree destination already exists (including symlinks): {0}")]
    DestinationExists(PathBuf),
    #[error("base commit is not a complete Git object ID: {0}")]
    InvalidBaseSha(String),
    #[error("worktree binding mismatch: {0}")]
    BindingMismatch(String),
    #[error("worktree is locked{reason}")]
    WorktreeLocked { reason: String },
    #[error("local branch {branch} moved from {expected} to {actual}")]
    BranchMoved {
        branch: String,
        expected: String,
        actual: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRepository {
    pub id: String,
    pub root_path: PathBuf,
    pub git_common_dir: PathBuf,
    pub display_name: String,
    pub is_linked_worktree: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreePlan {
    pub path: PathBuf,
    pub mode: WorktreeMode,
    pub branch_name: Option<String>,
    pub base_sha: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeBinding {
    pub path: PathBuf,
    pub mode: WorktreeMode,
    pub branch_name: Option<String>,
    pub head_sha: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeRetirementObservation {
    pub binding: WorktreeBinding,
    pub tracked_changes: bool,
    pub untracked_file_count: usize,
    pub ignored_file_count: usize,
    pub lock_reason: Option<String>,
}

impl WorktreeRetirementObservation {
    pub const fn has_local_changes(&self) -> bool {
        self.tracked_changes || self.untracked_file_count > 0 || self.ignored_file_count > 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreeTarget {
    NewBranch { branch_name: String },
    ExistingBranch { branch_name: String },
    Detached,
}

impl WorktreeTarget {
    pub const fn mode(&self) -> WorktreeMode {
        match self {
            Self::NewBranch { .. } => WorktreeMode::NewBranch,
            Self::ExistingBranch { .. } => WorktreeMode::ExistingBranch,
            Self::Detached => WorktreeMode::Detached,
        }
    }

    pub fn branch_name(&self) -> Option<&str> {
        match self {
            Self::NewBranch { branch_name } | Self::ExistingBranch { branch_name } => {
                Some(branch_name)
            }
            Self::Detached => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffResult {
    pub tracked_patch: Vec<u8>,
    pub tracked_patch_truncated: bool,
    pub untracked_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalStateManifest {
    pub source_path: PathBuf,
    pub source_head: String,
    pub carried_tracked_changes: bool,
    pub staged_patch_bytes: usize,
    pub unstaged_patch_bytes: usize,
    pub untracked_file_count: usize,
    pub untracked_file_bytes: usize,
    pub included_file_count: usize,
    pub included_file_bytes: usize,
    pub snapshot_hash: String,
}

#[derive(Debug, Clone)]
pub struct LocalStateSnapshot {
    staged_patch: Vec<u8>,
    unstaged_patch: Vec<u8>,
    untracked_files: Vec<IncludedFile>,
    included_files: Vec<IncludedFile>,
    manifest: LocalStateManifest,
}

#[derive(Debug, Clone)]
struct IncludedFile {
    path: PathBuf,
    contents: Vec<u8>,
}

impl LocalStateSnapshot {
    pub fn manifest(&self) -> &LocalStateManifest {
        &self.manifest
    }
}

#[derive(Debug, Clone)]
pub struct Git {
    executable: PathBuf,
    capture_limit: usize,
}

impl Default for Git {
    fn default() -> Self {
        Self::new("git")
    }
}

impl Git {
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            capture_limit: DEFAULT_CAPTURE_LIMIT,
        }
    }
}
