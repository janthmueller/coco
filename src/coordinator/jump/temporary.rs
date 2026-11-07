use super::*;
use crate::protocol::{
    RepositoryScope, WorkspaceAttachDisarmCleanupParams, WorkspaceAttachDisarmCleanupResult,
    WorkspaceAttachFinishParams, WorkspaceAttachFinishResult, WorkspaceCleanupResult,
    WorkspaceDeleteParams,
};

impl JumpLeaseRegistry {
    fn live_session(
        &mut self,
        workspace: &str,
        lease: &str,
    ) -> Result<&JumpLease, CoordinatorError> {
        self.prune();
        // Unlike ordinary idempotent release, completion needs a currently
        // valid exact capability. An expired/replayed completion cannot delete.
        self.leases
            .get(lease)
            .filter(|entry| entry.workspace_id == workspace && !entry.release_requested)
            .ok_or(CoordinatorError::InvalidWorkspaceAttachLease)
    }

    fn finish_session(&mut self, workspace: &str, lease: &str) -> Result<bool, CoordinatorError> {
        let adoption_pending = self.live_session(workspace, lease)?.adoption_in_flight;
        self.release(workspace, lease)?;
        Ok(!adoption_pending && self.reject_active(workspace).is_ok())
    }
}

impl Coordinator {
    pub(crate) fn disarm_workspace_cleanup(
        &self,
        params: WorkspaceAttachDisarmCleanupParams,
    ) -> Result<WorkspaceAttachDisarmCleanupResult, CoordinatorError> {
        validate_non_empty("workspaceId", &params.workspace_id)?;
        validate_non_empty("leaseId", &params.lease_id)?;
        let mut leases = self
            .jump_leases
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        leases.live_session(&params.workspace_id, &params.lease_id)?;
        // Keep authority pinned by the registry mutex until the durable claim
        // commits. Another session cannot finish/claim cleanup in this gap.
        self.store.claim_temporary_cleanup(&params.workspace_id)?;
        Ok(WorkspaceAttachDisarmCleanupResult {})
    }

    pub(crate) async fn finish_workspace_attach(
        &self,
        params: WorkspaceAttachFinishParams,
    ) -> Result<WorkspaceAttachFinishResult, CoordinatorError> {
        validate_non_empty("workspaceId", &params.workspace_id)?;
        validate_non_empty("leaseId", &params.lease_id)?;
        let last = self
            .jump_leases
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .finish_session(&params.workspace_id, &params.lease_id)?;
        if !last {
            return Ok(WorkspaceAttachFinishResult { cleanup: None });
        }
        let workspace =
            self.resolve_workspace(&RepositoryScope::AllRepositories, &params.workspace_id)?;
        let repository = self.repository_by_id(&workspace.repository_id)?;
        let Some(policy) = self.store.claim_temporary_cleanup(&workspace.id)? else {
            return Ok(WorkspaceAttachFinishResult { cleanup: None });
        };
        // Permission is consumed durably, even when safety checks fail. No
        // idle notification, expired lease or restart may retry this deletion.
        let result = self
            .delete_workspace(WorkspaceDeleteParams {
                scope: RepositoryScope::repository(repository.root_path),
                workspace: workspace.id.clone(),
                delete_thread: !policy.keep_thread,
                delete_branch: true,
                discard_changes: false,
                discard_unretained_commits: false,
                dry_run: false,
                expected_plan: None,
            })
            .await;
        let cleanup = match result {
            Ok(_) | Err(CoordinatorError::WorkspaceNotFound { .. }) => {
                WorkspaceCleanupResult::Deleted
            }
            Err(CoordinatorError::WorkspaceDeletionIncomplete { .. }) => {
                WorkspaceCleanupResult::Incomplete
            }
            Err(error) => cleanup_failure(
                error,
                self.store
                    .workspace_by_id(&workspace.id)?
                    .map(|current| current.availability),
            ),
        };
        Ok(WorkspaceAttachFinishResult {
            cleanup: Some(cleanup),
        })
    }
}

fn cleanup_failure(
    error: CoordinatorError,
    remaining: Option<WorkspaceAvailability>,
) -> WorkspaceCleanupResult {
    match remaining {
        None => return WorkspaceCleanupResult::Deleted,
        Some(WorkspaceAvailability::Open) => {}
        // A worktree-removal I/O error can have partial effects even without
        // the later-stage DeletionIncomplete wrapper. Trust recovery evidence,
        // not the error variant alone, before claiming that resources remain.
        Some(_) => return WorkspaceCleanupResult::Incomplete,
    }
    WorkspaceCleanupResult::Retained {
        reason: cleanup_reason(error),
    }
}

fn cleanup_reason(error: CoordinatorError) -> String {
    match error {
        CoordinatorError::WorkspaceRetirementBlocked(blockers) => blockers.join("; "),
        CoordinatorError::WorkspaceAttachInProgress => "another TUI is open".to_owned(),
        CoordinatorError::Guard(_) => "a guard blocked cleanup".to_owned(),
        _ => "cleanup could not be completed; inspect status before deleting".to_owned(),
    }
}

#[cfg(test)]
mod tests;
