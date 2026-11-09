use chrono::Utc;
use sha2::{Digest, Sha256};

use super::{Coordinator, CoordinatorError, validate_operation_id};
use crate::domain::Workspace;
use crate::domain::setup::{WorkerMcpReference, WorkspaceCreationReceipt, WorkspaceCreationState};
use crate::profile::{LoadedProfile, load_profile};
use crate::protocol::{
    WorkspaceContextRequest, WorkspaceCreateParams, WorkspaceCreationGetParams,
    WorkspaceSetupParams,
};
use crate::worker_mcp::{self, WorkerMcpError};

impl Coordinator {
    pub(super) async fn load_creation_profile(
        &self,
        params: &WorkspaceCreateParams,
        binding: Option<&WorkerMcpReference>,
    ) -> Result<LoadedProfile, CoordinatorError> {
        let mut profile = load_profile(&params.profile, &self.codex_home)?;
        profile.snapshot.model_override = params.model.clone();
        if let Some(reference) = binding {
            let recipe = worker_mcp::load(reference)?;
            recipe.apply(&mut profile.thread_config)?;
            self.worker
                .verify_worker_mcp_slots(&params.repository_path, &recipe.server_names())
                .await
                .map_err(|_| {
                    WorkerMcpError("native base configuration cannot accept these server positions")
                })?;
        }
        Ok(profile)
    }

    /// A native loader may quote rejected configuration values. Bound recipes
    /// never propagate those details into public errors or saved diagnostics.
    pub(super) fn worker_load_error(
        &self,
        workspace: &Workspace,
        error: CoordinatorError,
    ) -> CoordinatorError {
        if self.has_worker_mcp(workspace).unwrap_or(true) {
            WorkerMcpError("native loading did not confirm the bound worker configuration").into()
        } else {
            error
        }
    }

    pub(super) fn worker_turn_error(
        &self,
        workspace: &Workspace,
        error: super::WorkerError,
    ) -> super::WorkerError {
        if !self.has_worker_mcp(workspace).unwrap_or(true) {
            return error;
        }
        match error {
            super::WorkerError::TurnRejected(_) => super::WorkerError::TurnRejected(Box::new(
                WorkerMcpError("native turn was rejected before starting"),
            )),
            _ => super::WorkerError::runtime(WorkerMcpError(
                "native turn acceptance was not confirmed",
            )),
        }
    }

    pub(crate) fn get_workspace_creation(
        &self,
        params: WorkspaceCreationGetParams,
    ) -> Result<WorkspaceCreationReceipt, CoordinatorError> {
        validate_operation_id(&params.operation_id)?;
        // Discovery is read-only and works even before registration or after
        // repository removal. A foreign key never reveals its receipt.
        let repository = self.git.discover(params.repository_path)?;
        let receipt = match self.store.workspace_creation(&params.operation_id)? {
            Some(receipt) if receipt.repository_id == repository.id => receipt,
            _ => {
                let mut receipt = WorkspaceCreationReceipt::unknown(
                    params.operation_id.clone(),
                    repository.id.clone(),
                    repository.root_path,
                );
                if let Some(workspace) = self
                    .store
                    .workspace_by_create_operation_id(&params.operation_id)?
                    .filter(|w| w.repository_id == repository.id)
                {
                    receipt.state = WorkspaceCreationState::Unavailable;
                    receipt.workspace_id = Some(workspace.id);
                    receipt.workspace_name = Some(workspace.name);
                    receipt.error_code = Some("CREATION_RECEIPT_MISSING".into());
                }
                receipt
            }
        };
        self.observe_creation_target(receipt)
    }

    pub(crate) async fn setup_workspace(
        &self,
        params: WorkspaceSetupParams,
    ) -> Result<WorkspaceCreationReceipt, CoordinatorError> {
        validate_operation_id(&params.request.operation_id)?;
        let repository = self.git.discover(&params.repository_path)?;
        let request_sha256 = hex::encode(Sha256::digest(
            serde_json::to_vec(&params.request)
                .map_err(|_| CoordinatorError::InvalidParams("invalid setup request".into()))?,
        ));
        let mut intent = WorkspaceCreationReceipt::unknown(
            params.request.operation_id.clone(),
            repository.id,
            repository.root_path,
        );
        intent.request_sha256 = Some(request_sha256);
        intent.state = WorkspaceCreationState::Preparing;
        intent.workspace_name = Some(params.request.name.clone());
        intent.worker_mcp = params.request.worker_mcp.clone();
        let now = Utc::now().timestamp_millis();
        intent.created_at_ms = Some(now);
        intent.updated_at_ms = Some(now);
        if let Some(existing) = self.store.workspace_creation(&intent.operation_id)? {
            return self.observe_creation_target(check_replay(existing, &intent)?);
        }
        if let Some(existing) = self
            .store
            .workspace_by_create_operation_id(&intent.operation_id)?
        {
            return Err(if existing.repository_id == intent.repository_id {
                CoordinatorError::CreationEvidenceUnavailable
            } else {
                CoordinatorError::IdempotencyConflict
            });
        }
        // Read untrusted external bytes before taking a SQLite writer lock.
        // Keep any rejection durable too, without storing parser diagnostics.
        let validation = params
            .request
            .worker_mcp
            .as_ref()
            .map(worker_mcp::load)
            .transpose();
        let (claimed, execute) = self.store.claim_workspace_creation(&intent)?;
        if !execute {
            return self.observe_creation_target(check_replay(claimed, &intent)?);
        }
        let result = match validation {
            Err(error) => Err(CoordinatorError::from(error)),
            Ok(_) => {
                self.create_workspace_for_setup(
                    WorkspaceCreateParams {
                        repository_path: params.repository_path.clone(),
                        temporary: None,
                        name: params.request.name,
                        context: WorkspaceContextRequest::Fresh,
                        worktree: params.request.worktree,
                        changes: params.request.changes,
                        profile: params.request.profile,
                        model: params.request.model,
                        operation_id: params.request.operation_id.clone(),
                    },
                    Some(&intent),
                )
                .await
            }
        };
        if let Err(error) = result {
            self.store
                .reject_workspace_creation(&intent.operation_id, error.code())?;
        }
        self.get_workspace_creation(WorkspaceCreationGetParams {
            repository_path: params.repository_path,
            operation_id: intent.operation_id,
        })
    }

    pub(super) async fn load_workspace_profile(
        &self,
        workspace: &Workspace,
    ) -> Result<LoadedProfile, CoordinatorError> {
        let binding = self.store.worker_mcp_reference(&workspace.id)?;
        let mut profile =
            load_profile(&workspace.profile.name, &self.codex_home).map_err(|error| {
                if binding.is_some() {
                    CoordinatorError::from(WorkerMcpError("execution profile is unavailable"))
                } else {
                    CoordinatorError::from(error)
                }
            })?;
        if !super::recovery::same_profile_source(&profile.snapshot, &workspace.profile) {
            return Err(CoordinatorError::ProfileChanged(
                workspace.profile.name.clone(),
            ));
        }
        profile.snapshot.model_override = workspace.profile.model_override.clone();
        if let Some(reference) = binding {
            let recipe = worker_mcp::load(&reference)?;
            recipe.apply(&mut profile.thread_config)?;
            let cwd = workspace
                .worktree_path
                .as_deref()
                .ok_or(CoordinatorError::IncompleteWorkspace("worktree"))?;
            self.worker
                .verify_worker_mcp_slots(cwd, &recipe.server_names())
                .await
                .map_err(|_| {
                    WorkerMcpError("native base configuration cannot accept these server positions")
                })?;
        }
        Ok(profile)
    }

    pub(super) fn has_worker_mcp(&self, workspace: &Workspace) -> Result<bool, CoordinatorError> {
        Ok(self.store.worker_mcp_reference(&workspace.id)?.is_some())
    }

    fn observe_creation_target(
        &self,
        mut receipt: WorkspaceCreationReceipt,
    ) -> Result<WorkspaceCreationReceipt, CoordinatorError> {
        if receipt.state != WorkspaceCreationState::Deleted
            && let Some(id) = receipt.workspace_id.as_deref()
            && self.store.workspace_by_id(id)?.is_none()
        {
            receipt.state = WorkspaceCreationState::Unavailable;
            receipt.error_code = Some("CREATION_TARGET_UNAVAILABLE".into());
        }
        Ok(receipt)
    }
}

fn check_replay(
    receipt: WorkspaceCreationReceipt,
    intent: &WorkspaceCreationReceipt,
) -> Result<WorkspaceCreationReceipt, CoordinatorError> {
    if receipt.repository_id != intent.repository_id
        || receipt.request_sha256 != intent.request_sha256
    {
        return Err(CoordinatorError::IdempotencyConflict);
    }
    // Includes preparing, failed and deleted: never recapture or recreate.
    Ok(receipt)
}
