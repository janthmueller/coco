use std::path::PathBuf;

use thiserror::Error;

use super::WorkerError;
use crate::domain::TaskPhase;
use crate::git::GitError;
use crate::profile::ProfileError;
use crate::store::StoreError;

#[derive(Debug, Error)]
pub(crate) enum CoordinatorError {
    #[error("invalid request parameters: {0}")]
    InvalidParams(String),
    #[error("unsupported context mode {0:?}")]
    UnsupportedContext(String),
    #[error("repository is not registered: {0}")]
    RepositoryNotRegistered(PathBuf),
    #[error("task already exists: {0}")]
    TaskExists(String),
    #[error("task not found: {0}")]
    TaskNotFound(String),
    #[error("operation ID was already used with different parameters")]
    IdempotencyConflict,
    #[error("task must be {expected}, but is {actual:?}")]
    InvalidTaskState {
        expected: &'static str,
        actual: TaskPhase,
    },
    #[error("task has no bound {0}")]
    IncompleteTask(&'static str),
    #[error("profile {0:?} changed since this task was created")]
    ProfileChanged(String),
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Profile(#[from] ProfileError),
    #[error(transparent)]
    Worker(#[from] WorkerError),
}

impl CoordinatorError {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::InvalidParams(_) => "INVALID_PARAMS",
            Self::UnsupportedContext(_) => "UNSUPPORTED_CONTEXT",
            Self::RepositoryNotRegistered(_) => "REPOSITORY_NOT_REGISTERED",
            Self::TaskExists(_) => "TASK_EXISTS",
            Self::TaskNotFound(_) => "TASK_NOT_FOUND",
            Self::IdempotencyConflict => "IDEMPOTENCY_CONFLICT",
            Self::InvalidTaskState { .. } => "INVALID_TASK_STATE",
            Self::IncompleteTask(_) => "INCOMPLETE_TASK",
            Self::ProfileChanged(_) => "PROFILE_CHANGED",
            Self::Git(GitError::DirtyRepository(_)) => "DIRTY_SOURCE",
            Self::Git(GitError::InvalidTaskName(_)) => "INVALID_TASK_NAME",
            Self::Git(GitError::BranchExists(_) | GitError::DestinationExists(_)) => {
                "TASK_COLLISION"
            }
            Self::Git(GitError::NotAWorktree(_)) => "NOT_A_GIT_REPOSITORY",
            Self::Git(_) => "GIT_ERROR",
            Self::Store(StoreError::NotFound { .. }) => "NOT_FOUND",
            Self::Store(StoreError::InvalidTaskTransition { .. }) => "INVALID_TASK_STATE",
            Self::Store(_) => "INTERNAL",
            Self::Profile(ProfileError::NotFound { .. }) => "PROFILE_NOT_FOUND",
            Self::Profile(_) => "INVALID_PROFILE",
            Self::Worker(_) => "CODEX_ERROR",
        }
    }
}
