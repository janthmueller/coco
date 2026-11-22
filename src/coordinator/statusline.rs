use std::time::Duration;

use super::{Coordinator, CoordinatorError, validate_non_empty};
use crate::domain::{WorkspaceAvailability, WorkspaceLifecycle};
use crate::protocol::{
    WorkspaceStatuslineParams, WorkspaceStatuslineResult, WorkspaceStatuslineSnapshot,
};

impl Coordinator {
    /// Passive bound-thread metadata only: no Git inspection, native history,
    /// runtime activation, status hydration, repository registration or audit.
    pub(crate) async fn workspace_statusline(
        &self,
        params: WorkspaceStatuslineParams,
    ) -> Result<WorkspaceStatuslineResult, CoordinatorError> {
        for (field, value) in [
            ("repositoryId", &params.repository_id),
            ("threadId", &params.thread_id),
        ] {
            validate_non_empty(field, value)?;
            if value.len() > 256 || value.chars().any(char::is_control) {
                return Err(CoordinatorError::InvalidParams(format!("invalid {field}")));
            }
        }
        if self
            .store
            .repository_by_id(&params.repository_id)?
            .is_none()
        {
            return Ok(WorkspaceStatuslineResult::default());
        }
        let Some(workspace) = self.store.workspace_by_thread_id(&params.thread_id)? else {
            return Ok(WorkspaceStatuslineResult::default());
        };
        if workspace.repository_id != params.repository_id
            || workspace.availability != WorkspaceAvailability::Open
            || workspace.lifecycle != WorkspaceLifecycle::Ready
        {
            return Ok(WorkspaceStatuslineResult::default());
        }
        let resources = tokio::time::timeout(
            Duration::from_millis(200),
            self.worker.workspace_resources(&workspace.id),
        )
        .await
        .ok()
        .and_then(Result::ok)
        .flatten();
        Ok(WorkspaceStatuslineResult {
            workspace: Some(WorkspaceStatuslineSnapshot {
                workspace_id: workspace.id,
                name: workspace.name,
                resources,
            }),
        })
    }
}
