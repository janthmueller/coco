use std::io;
use std::path::PathBuf;

use thiserror::Error;

const DEFAULT_CAPTURE_LIMIT: usize = 16 * 1024 * 1024;

mod command;
mod diff;
mod repository;
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
    #[error(
        "invalid workspace name `{0}`; use 1-63 bytes of lowercase letters, digits, hyphens, and single slashes between components"
    )]
    InvalidWorkspaceName(String),
    #[error("branch namespace collision: requested {requested}, existing {existing}")]
    BranchCollision { requested: String, existing: String },
    #[error("worktree destination already exists (including symlinks): {0}")]
    DestinationExists(PathBuf),
    #[error("base commit is not a complete Git object ID: {0}")]
    InvalidBaseSha(String),
    #[error("worktree binding mismatch: {0}")]
    BindingMismatch(String),
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
    pub branch_name: String,
    pub base_sha: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeBinding {
    pub path: PathBuf,
    pub branch_name: String,
    pub head_sha: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffResult {
    pub tracked_patch: Vec<u8>,
    pub tracked_patch_truncated: bool,
    pub untracked_paths: Vec<PathBuf>,
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
