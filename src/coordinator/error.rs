use std::path::PathBuf;

use serde::Serialize;
use serde_json::{Value, json};
use thiserror::Error;

use super::WorkerError;
use crate::domain::WorkspacePhase;
use crate::git::GitError;
use crate::profile::ProfileError;
use crate::store::StoreError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkspaceReferenceCandidate {
    pub(crate) workspace_id: String,
    pub(crate) workspace_name: String,
    pub(crate) repository_path: PathBuf,
}

#[derive(Debug, Error)]
pub(crate) enum CoordinatorError {
    #[error("invalid request parameters: {0}")]
    InvalidParams(String),
    #[error("unsupported context mode {0:?}")]
    UnsupportedContext(String),
    #[error("repository is not registered: {0}")]
    RepositoryNotRegistered(PathBuf),
    #[error("workspace already exists: {0}")]
    WorkspaceExists(String),
    #[error("workspace not found: {reference}")]
    WorkspaceNotFound {
        reference: String,
        candidates: Vec<WorkspaceReferenceCandidate>,
    },
    #[error("workspace reference is ambiguous across repositories: {reference}")]
    WorkspaceReferenceAmbiguous {
        reference: String,
        candidates: Vec<WorkspaceReferenceCandidate>,
    },
    #[error("operation ID was already used with different parameters")]
    IdempotencyConflict,
    #[error("workspace must be {expected}, but is {actual:?}")]
    InvalidWorkspaceState {
        expected: &'static str,
        actual: WorkspacePhase,
    },
    #[error("workspace has no bound {0}")]
    IncompleteWorkspace(&'static str),
    #[error("profile {0:?} changed since this workspace was created")]
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
            Self::WorkspaceExists(_) => "WORKSPACE_EXISTS",
            Self::WorkspaceNotFound { .. } => "WORKSPACE_NOT_FOUND",
            Self::WorkspaceReferenceAmbiguous { .. } => "WORKSPACE_REFERENCE_AMBIGUOUS",
            Self::IdempotencyConflict => "IDEMPOTENCY_CONFLICT",
            Self::InvalidWorkspaceState { .. } => "INVALID_WORKSPACE_STATE",
            Self::IncompleteWorkspace(_) => "INCOMPLETE_WORKSPACE",
            Self::ProfileChanged(_) => "PROFILE_CHANGED",
            Self::Git(GitError::DirtyRepository(_)) => "DIRTY_SOURCE",
            Self::Git(GitError::InvalidWorkspaceName(_)) => "INVALID_WORKSPACE_NAME",
            Self::Git(GitError::BranchCollision { .. } | GitError::DestinationExists(_)) => {
                "WORKSPACE_COLLISION"
            }
            Self::Git(GitError::NotAWorktree(_)) => "NOT_A_GIT_REPOSITORY",
            Self::Git(_) => "GIT_ERROR",
            Self::Store(StoreError::NotFound { .. }) => "NOT_FOUND",
            Self::Store(StoreError::InvalidWorkspaceTransition { .. }) => "INVALID_WORKSPACE_STATE",
            Self::Store(_) => "INTERNAL",
            Self::Profile(ProfileError::NotFound { .. }) => "PROFILE_NOT_FOUND",
            Self::Profile(_) => "INVALID_PROFILE",
            Self::Worker(_) => "CODEX_ERROR",
        }
    }

    pub(crate) fn data(&self) -> Option<Value> {
        match self {
            Self::WorkspaceNotFound { candidates, .. } if !candidates.is_empty() => {
                Some(json!({"matches": candidates}))
            }
            Self::WorkspaceReferenceAmbiguous { candidates, .. } => {
                Some(json!({"matches": candidates}))
            }
            _ => None,
        }
    }
}
