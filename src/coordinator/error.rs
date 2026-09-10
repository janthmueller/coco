use std::path::PathBuf;

use serde::Serialize;
use serde_json::{Value, json};
use thiserror::Error;

use super::WorkerError;
use crate::domain::WorkspacePhase;
use crate::domain::signals::SignalError;
use crate::git::GitError;
use crate::hooks::{GuardRejection, HookConfigError};
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
    #[error(transparent)]
    Signal(#[from] SignalError),
    #[error("invalid request parameters: {0}")]
    InvalidParams(String),
    #[error(transparent)]
    HookConfig(#[from] HookConfigError),
    #[error(transparent)]
    Guard(#[from] GuardRejection),
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
    #[error(
        "context reference {reference:?} did not match a workspace in the destination repository and could not be read as a native Codex thread: {source}"
    )]
    ContextReferenceUnresolved {
        reference: String,
        #[source]
        source: WorkerError,
    },
    #[error("operation ID was already used with different parameters")]
    IdempotencyConflict,
    #[error(
        "operation {operation_id} may already have reached Codex and will not be retried automatically"
    )]
    OperationUncertain { operation_id: String },
    #[error("workspace must be {expected}, but is {actual:?}")]
    InvalidWorkspaceState {
        expected: &'static str,
        actual: WorkspacePhase,
    },
    #[error("workspace has no bound {0}")]
    IncompleteWorkspace(&'static str),
    #[error("workspace is already being opened in the Codex terminal UI")]
    WorkspaceAttachInProgress,
    #[error("workspace cannot be retired safely: {blockers}", blockers = .0.join("; "))]
    WorkspaceRetirementBlocked(Vec<String>),
    #[error("the temporary workspace attach lease is missing, expired, or does not match")]
    InvalidWorkspaceAttachLease,
    #[error("profile {0:?} changed since this workspace was created")]
    ProfileChanged(String),
    #[error("Codex thread compaction failed: {0}")]
    CompactionFailed(String),
    #[error("Codex thread compaction did not finish within 15 minutes")]
    CompactionTimedOut,
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
            Self::Signal(error) | Self::Store(StoreError::Signal(error)) => error.code(),
            Self::InvalidParams(_) => "INVALID_PARAMS",
            Self::HookConfig(_) => "HOOK_CONFIG_INVALID",
            Self::Guard(error) => error.code(),
            Self::RepositoryNotRegistered(_) => "REPOSITORY_NOT_REGISTERED",
            Self::WorkspaceExists(_) => "WORKSPACE_EXISTS",
            Self::WorkspaceNotFound { .. } => "WORKSPACE_NOT_FOUND",
            Self::WorkspaceReferenceAmbiguous { .. } => "WORKSPACE_REFERENCE_AMBIGUOUS",
            Self::ContextReferenceUnresolved { .. } => "CONTEXT_REFERENCE_UNRESOLVED",
            Self::IdempotencyConflict => "IDEMPOTENCY_CONFLICT",
            Self::OperationUncertain { .. } => "OPERATION_UNCERTAIN",
            Self::InvalidWorkspaceState { .. } => "INVALID_WORKSPACE_STATE",
            Self::IncompleteWorkspace(_) => "INCOMPLETE_WORKSPACE",
            Self::WorkspaceAttachInProgress => "WORKSPACE_BUSY",
            Self::WorkspaceRetirementBlocked(_) => "WORKSPACE_RETIREMENT_BLOCKED",
            Self::InvalidWorkspaceAttachLease => "ATTACH_LEASE_INVALID",
            Self::ProfileChanged(_) => "PROFILE_CHANGED",
            Self::CompactionFailed(_) => "CODEX_COMPACTION_FAILED",
            Self::CompactionTimedOut => "CODEX_COMPACTION_TIMEOUT",
            Self::Git(GitError::DirtyRepository(_)) => "DIRTY_SOURCE",
            Self::Git(GitError::InvalidWorkspaceName(_)) => "INVALID_WORKSPACE_NAME",
            Self::Git(GitError::BranchCollision { .. } | GitError::DestinationExists(_)) => {
                "WORKSPACE_COLLISION"
            }
            Self::Git(GitError::NotAWorktree(_)) => "NOT_A_GIT_REPOSITORY",
            Self::Git(_) => "GIT_ERROR",
            Self::Store(StoreError::NotFound { .. }) => "NOT_FOUND",
            Self::Store(StoreError::InvalidWorkspaceTransition { .. }) => "INVALID_WORKSPACE_STATE",
            Self::Store(StoreError::InvalidDecisionState { .. }) => "INVALID_DECISION_STATE",
            Self::Store(StoreError::DecisionGenerationMismatch { .. }) => "STALE_DECISION",
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
            Self::OperationUncertain { operation_id } => Some(json!({"operationId": operation_id})),
            Self::WorkspaceRetirementBlocked(blockers) => Some(json!({"blockers": blockers})),
            Self::Guard(error) => {
                let (guard_id, action, reason) = error.details();
                Some(json!({
                    "guardId": guard_id,
                    "action": action.as_str(),
                    "reason": reason,
                }))
            }
            _ => None,
        }
    }
}
