use chrono::Utc;
use sha2::{Digest, Sha256};

use super::*;
use crate::coordinator::validate_operation_id;
use crate::domain::retirement::{WorkspaceDeletionReceipt, WorkspaceDeletionState};
use crate::protocol::{
    RepositoryScope, WorkspaceDeleteOperationParams, WorkspaceDeletionGetParams,
};

impl Coordinator {
    pub(crate) fn get_workspace_deletion(
        &self,
        params: WorkspaceDeletionGetParams,
    ) -> Result<WorkspaceDeletionReceipt, CoordinatorError> {
        validate_operation_id(&params.operation_id)?;
        let repository = self.git.discover(params.repository_path)?;
        Ok(self
            .store
            .workspace_deletion(&params.operation_id)?
            .filter(|receipt| receipt.repository_id == repository.id)
            .unwrap_or_else(|| {
                WorkspaceDeletionReceipt::unknown(
                    params.operation_id,
                    repository.id,
                    repository.root_path,
                )
            }))
    }

    pub(crate) async fn delete_workspace_operation(
        &self,
        params: WorkspaceDeleteOperationParams,
    ) -> Result<WorkspaceDeletionReceipt, CoordinatorError> {
        validate_operation_id(&params.request.operation_id)?;
        let repository = self.git.discover(&params.repository_path)?;
        let request_sha256 = hex::encode(Sha256::digest(
            serde_json::to_vec(&params.request).map_err(|_| invalid_plan())?,
        ));
        // Scope discovery is required; replay precedes target resolution and
        // native/worktree inspection, including for a reused workspace name.
        if let Some(receipt) = self
            .store
            .workspace_deletion(&params.request.operation_id)?
        {
            return check_replay(receipt, &repository.id, &request_sha256);
        }
        let (mut core, mut receipt) = match self
            .prepare_public_deletion(&params, &repository, &request_sha256)
            .await
        {
            Ok(prepared) => prepared,
            Err(error) => {
                // A matching dispatch may have completed while this caller
                // waited for planning. Its receipt wins over missing resources.
                return match self
                    .store
                    .workspace_deletion(&params.request.operation_id)?
                {
                    Some(receipt) => check_replay(receipt, &repository.id, &request_sha256),
                    None => Err(error),
                };
            }
        };
        if params.dry_run {
            return Ok(receipt);
        }
        let now = Utc::now().timestamp_millis();
        receipt.state = WorkspaceDeletionState::Preparing;
        receipt.created_at_ms = Some(now);
        receipt.updated_at_ms = Some(now);
        let (claimed, execute) = self.store.claim_workspace_deletion(&receipt)?;
        if !execute {
            return check_replay(claimed, &receipt.repository_id, &request_sha256);
        }
        core.dry_run = false;
        if let Err(error) = self
            .delete_workspace_with_operation(core, Some(&receipt.operation_id))
            .await
        {
            self.store
                .fail_workspace_deletion(&receipt.operation_id, error.code())?;
        }
        // The scope was authenticated before dispatch. Its original path may
        // itself have been the now-removed worktree; do not rediscover it to
        // return the already-scoped durable result.
        match self.store.workspace_deletion(&receipt.operation_id)? {
            Some(observed) => check_replay(observed, &repository.id, &request_sha256),
            None => Ok(WorkspaceDeletionReceipt::unknown(
                receipt.operation_id,
                repository.id,
                repository.root_path,
            )),
        }
    }

    async fn prepare_public_deletion(
        &self,
        params: &WorkspaceDeleteOperationParams,
        repository: &GitRepository,
        request_sha256: &str,
    ) -> Result<(WorkspaceDeleteParams, WorkspaceDeletionReceipt), CoordinatorError> {
        let candidate = match self.store.workspace_by_id(&params.request.workspace)? {
            Some(workspace) => (workspace.repository_id == repository.id).then_some(workspace),
            None => self
                .store
                .workspace_by_name(&repository.id, &params.request.workspace)?,
        };
        let workspace = candidate.ok_or_else(|| CoordinatorError::WorkspaceNotFound {
            reference: params.request.workspace.clone(),
            candidates: Vec::new(),
        })?;
        let mut core = WorkspaceDeleteParams {
            scope: RepositoryScope::repository(params.repository_path.clone()),
            workspace: workspace.id,
            delete_thread: !params.request.keep_thread,
            delete_branch: !params.request.keep_branch,
            discard_changes: params.request.discard_changes,
            discard_unretained_commits: params.request.discard_unretained_commits,
            dry_run: true,
            expected_plan: None,
        };
        let preview = self.delete_workspace(core.clone()).await?;
        let plan_sha256 = preview.plan.sha256().map_err(|_| invalid_plan())?;
        if params
            .expected_plan_sha256
            .as_ref()
            .is_some_and(|expected| expected != &plan_sha256)
        {
            return Err(CoordinatorError::WorkspaceRetirementBlocked(vec![
                "workspace retirement plan changed; review it again before applying".into(),
            ]));
        }
        let mut receipt = WorkspaceDeletionReceipt::unknown(
            params.request.operation_id.clone(),
            repository.id.clone(),
            repository.root_path.clone(),
        );
        receipt.state = WorkspaceDeletionState::Preview;
        receipt.request_sha256 = Some(request_sha256.to_owned());
        receipt.plan = Some(preview.plan.clone());
        receipt.plan_sha256 = Some(plan_sha256);
        core.expected_plan = Some(preview.plan);
        Ok((core, receipt))
    }
}

fn check_replay(
    receipt: WorkspaceDeletionReceipt,
    repository_id: &str,
    request_sha256: &str,
) -> Result<WorkspaceDeletionReceipt, CoordinatorError> {
    if receipt.repository_id != repository_id
        || receipt.request_sha256.as_deref() != Some(request_sha256)
    {
        return Err(CoordinatorError::IdempotencyConflict);
    }
    Ok(receipt)
}

fn invalid_plan() -> CoordinatorError {
    CoordinatorError::InvalidParams("could not encode retirement parameters".into())
}
