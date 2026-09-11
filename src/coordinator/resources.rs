use crate::domain::runtime::{WorkspaceResourcePolicy, WorkspaceResourcePolicySnapshot};
use crate::domain::{Workspace, WorkspaceAvailability};
use crate::protocol::{
    ResourcePolicyUpdate, WorkspaceLimitsGetParams, WorkspaceLimitsResetParams,
    WorkspaceLimitsResult, WorkspaceLimitsSetParams, WorkspaceResourcePolicyPatch,
};

use super::{Coordinator, CoordinatorError};

impl Coordinator {
    pub(crate) async fn get_workspace_limits(
        &self,
        params: WorkspaceLimitsGetParams,
    ) -> Result<WorkspaceLimitsResult, CoordinatorError> {
        let workspace = self.resolve_workspace(&params.scope, &params.workspace)?;
        let policy = self.store.workspace_resource_policy(&workspace.id)?;
        let controller = self
            .worker
            .workspace_resource_policy_status(&workspace.id)
            .await?;
        Ok(WorkspaceLimitsResult {
            workspace,
            policy,
            controller,
        })
    }

    pub(crate) async fn set_workspace_limits(
        &self,
        params: WorkspaceLimitsSetParams,
    ) -> Result<WorkspaceLimitsResult, CoordinatorError> {
        if params.patch.is_empty() {
            return Err(CoordinatorError::InvalidParams(
                "at least one resource limit change is required".to_owned(),
            ));
        }
        let resolved = self.resolve_workspace(&params.scope, &params.workspace)?;
        let repository_lock = self.repository_lock(&resolved.repository_id).await;
        let _guard = repository_lock.lock().await;
        let workspace = self.resolve_workspace(&params.scope, &resolved.id)?;
        validate_resource_policy_target(&workspace)?;
        let current = self.store.workspace_resource_policy(&workspace.id)?;
        let policy = apply_patch(current.policy.clone(), params.patch);
        policy.validate()?;
        self.require_resource_policy_support(&policy)?;
        self.replace_resource_policy(&workspace.id, current, policy)
            .await?;
        self.get_workspace_limits(WorkspaceLimitsGetParams {
            scope: params.scope,
            workspace: workspace.id,
        })
        .await
    }

    pub(crate) async fn reset_workspace_limits(
        &self,
        params: WorkspaceLimitsResetParams,
    ) -> Result<WorkspaceLimitsResult, CoordinatorError> {
        let resolved = self.resolve_workspace(&params.scope, &params.workspace)?;
        let repository_lock = self.repository_lock(&resolved.repository_id).await;
        let _guard = repository_lock.lock().await;
        let workspace = self.resolve_workspace(&params.scope, &resolved.id)?;
        validate_resource_policy_target(&workspace)?;
        let current = self.store.workspace_resource_policy(&workspace.id)?;
        self.replace_resource_policy(&workspace.id, current, WorkspaceResourcePolicy::default())
            .await?;
        self.get_workspace_limits(WorkspaceLimitsGetParams {
            scope: params.scope,
            workspace: workspace.id,
        })
        .await
    }

    fn require_resource_policy_support(
        &self,
        policy: &WorkspaceResourcePolicy,
    ) -> Result<(), CoordinatorError> {
        let unsupported = self
            .worker
            .workspace_resource_capabilities()
            .unsupported_fields(policy);
        if unsupported.is_empty() {
            Ok(())
        } else {
            Err(CoordinatorError::ResourcePolicyUnsupported {
                fields: unsupported,
            })
        }
    }

    /// Prevents durable hard-limit intent from silently degrading when a
    /// workspace is activated under a less capable runtime backend.
    pub(super) fn require_workspace_resource_policy_support(
        &self,
        workspace_id: &str,
    ) -> Result<(), CoordinatorError> {
        let snapshot = self.store.workspace_resource_policy(workspace_id)?;
        self.require_resource_policy_support(&snapshot.policy)
    }

    async fn replace_resource_policy(
        &self,
        workspace_id: &str,
        current: WorkspaceResourcePolicySnapshot,
        policy: WorkspaceResourcePolicy,
    ) -> Result<(), CoordinatorError> {
        if current.policy == policy {
            return Ok(());
        }
        let desired = self.store.replace_workspace_resource_policy(
            workspace_id,
            current.revision,
            &policy,
        )?;
        match self
            .worker
            .configure_workspace_resource_policy(workspace_id, desired.clone())
            .await
        {
            Ok(_) => Ok(()),
            Err(source) => {
                let rollback = match self.store.replace_workspace_resource_policy(
                    workspace_id,
                    desired.revision,
                    &current.policy,
                ) {
                    Ok(rollback) => rollback,
                    Err(rollback_store_error) => {
                        tracing::error!(
                            workspace_id,
                            %rollback_store_error,
                            "workspace resource policy persistence rollback failed"
                        );
                        self.stop_runtime_after_policy_recovery_failure(workspace_id)
                            .await;
                        // The desired policy is still durable. Refresh the
                        // inactive cache so a later activation cannot use the
                        // previous policy in this daemon generation.
                        if let Err(cache_error) = self
                            .worker
                            .configure_workspace_resource_policy(workspace_id, desired)
                            .await
                        {
                            tracing::error!(
                                workspace_id,
                                %cache_error,
                                "workspace resource policy cache could not be reconciled after persistence rollback failed"
                            );
                        }
                        return Err(CoordinatorError::ResourcePolicyRecoveryIncomplete {
                            workspace_id: workspace_id.to_owned(),
                            source: rollback_store_error,
                        });
                    }
                };
                if let Err(rollback_error) = self
                    .worker
                    .configure_workspace_resource_policy(workspace_id, rollback.clone())
                    .await
                {
                    tracing::error!(
                        workspace_id,
                        %rollback_error,
                        "workspace resource policy rollback failed"
                    );
                    self.stop_runtime_after_policy_recovery_failure(workspace_id)
                        .await;
                    if let Err(cache_error) = self
                        .worker
                        .configure_workspace_resource_policy(workspace_id, rollback)
                        .await
                    {
                        tracing::error!(
                            workspace_id,
                            %cache_error,
                            "inactive workspace resource policy cache could not be reconciled"
                        );
                    }
                }
                Err(CoordinatorError::ResourcePolicyApplication(source))
            }
        }
    }

    async fn stop_runtime_after_policy_recovery_failure(&self, workspace_id: &str) {
        if let Err(stop_error) = self.worker.stop_workspace_execution(workspace_id).await {
            tracing::error!(
                workspace_id,
                %stop_error,
                "workspace runtime could not be stopped after a policy recovery failure"
            );
        }
    }
}

fn validate_resource_policy_target(workspace: &Workspace) -> Result<(), CoordinatorError> {
    if matches!(
        workspace.availability,
        WorkspaceAvailability::Open | WorkspaceAvailability::Closed
    ) && !matches!(
        workspace.lifecycle,
        crate::domain::WorkspaceLifecycle::Provisioning
            | crate::domain::WorkspaceLifecycle::Starting
    ) {
        Ok(())
    } else {
        Err(CoordinatorError::InvalidParams(format!(
            "workspace resource limits cannot change while the workspace is {}",
            workspace.phase.as_str()
        )))
    }
}

fn apply_patch(
    mut policy: WorkspaceResourcePolicy,
    patch: WorkspaceResourcePolicyPatch,
) -> WorkspaceResourcePolicy {
    apply_update(&mut policy.memory_high_bytes, patch.memory_high_bytes);
    apply_update(&mut policy.memory_max_bytes, patch.memory_max_bytes);
    apply_update(&mut policy.cpu_max_millicores, patch.cpu_max_millicores);
    apply_update(&mut policy.cpu_weight, patch.cpu_weight);
    apply_update(&mut policy.tasks_max, patch.tasks_max);
    policy
}

fn apply_update<T>(target: &mut Option<T>, update: Option<ResourcePolicyUpdate<T>>) {
    match update {
        Some(ResourcePolicyUpdate::Set(value)) => *target = Some(value),
        Some(ResourcePolicyUpdate::Clear) => *target = None,
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_patch_changes_only_selected_fields() {
        let policy = WorkspaceResourcePolicy {
            memory_high_bytes: Some(128),
            memory_max_bytes: Some(256),
            cpu_weight: Some(100),
            ..WorkspaceResourcePolicy::default()
        };
        let patched = apply_patch(
            policy,
            WorkspaceResourcePolicyPatch {
                memory_high_bytes: Some(ResourcePolicyUpdate::Clear),
                cpu_max_millicores: Some(ResourcePolicyUpdate::Set(1_500)),
                ..WorkspaceResourcePolicyPatch::default()
            },
        );
        assert_eq!(patched.memory_high_bytes, None);
        assert_eq!(patched.memory_max_bytes, Some(256));
        assert_eq!(patched.cpu_max_millicores, Some(1_500));
        assert_eq!(patched.cpu_weight, Some(100));
    }
}
