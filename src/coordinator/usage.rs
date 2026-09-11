use std::time::{Duration, Instant};

use chrono::Utc;
use tracing::debug;

use super::{Coordinator, CoordinatorError};
use crate::domain::{Workspace, WorkspaceAvailability};
use crate::protocol::{
    RepositoryScope, RepositorySummary, WorkspaceCostEstimate, WorkspaceCostUnavailableReason,
    WorkspaceTokenUsageSnapshot, WorkspaceUsageGetParams, WorkspaceUsageItem,
    WorkspaceUsageListParams, WorkspaceUsageSource,
};

const COST_CACHE_TTL: Duration = Duration::from_secs(15);

impl Coordinator {
    pub(crate) async fn list_workspace_usage(
        &self,
        params: WorkspaceUsageListParams,
    ) -> Result<Vec<WorkspaceUsageItem>, CoordinatorError> {
        let repository_id = match &params.scope {
            RepositoryScope::Repository { path } => {
                Some(self.registered_repository_for_path(path)?.0.id)
            }
            RepositoryScope::AllRepositories => None,
        };
        let mut workspaces = self.store.list_workspaces(repository_id.as_deref())?;
        workspaces.retain(|workspace| workspace.availability != WorkspaceAvailability::Closed);
        let mut usage = Vec::with_capacity(workspaces.len());
        for workspace in workspaces {
            usage.push(self.workspace_usage_item(workspace).await?);
        }
        Ok(usage)
    }

    pub(crate) async fn get_workspace_usage(
        &self,
        params: WorkspaceUsageGetParams,
    ) -> Result<WorkspaceUsageItem, CoordinatorError> {
        let workspace = self.resolve_workspace(&params.scope, &params.workspace)?;
        self.workspace_usage_item(workspace).await
    }

    async fn workspace_usage_item(
        &self,
        workspace: Workspace,
    ) -> Result<WorkspaceUsageItem, CoordinatorError> {
        let repository = RepositorySummary::from(&self.repository_by_id(&workspace.repository_id)?);
        let tokens = self
            .store
            .workspace_token_usage(&workspace.id)?
            .map(|checkpoint| WorkspaceTokenUsageSnapshot {
                is_fresh: checkpoint.runtime_generation == self.runtime_generation,
                checkpoint,
                source: WorkspaceUsageSource::ThreadTokenUsageUpdated,
            });
        let cost = self.workspace_cost(&workspace).await;
        Ok(WorkspaceUsageItem {
            workspace,
            repository,
            tokens,
            cost,
        })
    }

    async fn workspace_cost(&self, workspace: &Workspace) -> WorkspaceCostEstimate {
        let Some(thread_id) = workspace.codex_thread_id.as_deref() else {
            return WorkspaceCostEstimate::Unavailable {
                reason: WorkspaceCostUnavailableReason::NoThread,
                checked_at_ms: None,
            };
        };
        let now = Instant::now();
        if let Some((checked_at, estimate)) = self
            .usage_costs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(thread_id)
            .cloned()
            && now.duration_since(checked_at) < COST_CACHE_TTL
        {
            return estimate;
        }

        let checked_at_ms = Utc::now().timestamp_millis();
        let estimate = match self.worker.read_thread_cost(thread_id).await {
            Ok(Some(native)) if native.thread_id == thread_id => WorkspaceCostEstimate::Available {
                estimated_usage_credits_micros: native.estimated_usage_credits_micros,
                estimated_usage_usd_micros: native.estimated_usage_usd_micros,
                groups: native.groups,
                observed_at_ms: checked_at_ms,
            },
            Ok(Some(native)) => {
                debug!(
                    workspace_id = %workspace.id,
                    expected_thread_id = thread_id,
                    actual_thread_id = native.thread_id,
                    "ignoring a billing estimate for another native thread"
                );
                WorkspaceCostEstimate::Unavailable {
                    reason: WorkspaceCostUnavailableReason::ReadFailed,
                    checked_at_ms: Some(checked_at_ms),
                }
            }
            Ok(None) => WorkspaceCostEstimate::Unavailable {
                reason: WorkspaceCostUnavailableReason::NotReported,
                checked_at_ms: Some(checked_at_ms),
            },
            Err(source) => {
                debug!(workspace_id = %workspace.id, %source, "native thread cost is unavailable");
                WorkspaceCostEstimate::Unavailable {
                    reason: WorkspaceCostUnavailableReason::ReadFailed,
                    checked_at_ms: Some(checked_at_ms),
                }
            }
        };
        self.usage_costs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(thread_id.to_owned(), (now, estimate.clone()));
        estimate
    }

    pub(super) fn invalidate_thread_cost(&self, thread_id: &str) {
        self.usage_costs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(thread_id);
    }
}
