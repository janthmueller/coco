use std::path::PathBuf;

use super::{Coordinator, CoordinatorError};
use crate::domain::{Workspace, WorkspaceAvailability, WorkspaceLifecycle};
use crate::protocol::{
    RepositoryScope, RepositorySummary, WorkspacePathEntry, WorkspacePathGetParams,
    WorkspacePathListParams,
};

impl Coordinator {
    pub(crate) fn workspace_path(
        &self,
        params: WorkspacePathGetParams,
    ) -> Result<PathBuf, CoordinatorError> {
        let workspace = self.resolve_workspace(&params.scope, &params.workspace)?;
        available_path(&workspace)
    }

    pub(crate) fn list_workspace_paths(
        &self,
        params: WorkspacePathListParams,
    ) -> Result<Vec<WorkspacePathEntry>, CoordinatorError> {
        let repository_id = match params.scope {
            RepositoryScope::Repository { path } => {
                Some(self.registered_repository_for_path(&path)?.0.id)
            }
            RepositoryScope::AllRepositories => None,
        };
        let mut entries = Vec::new();
        for workspace in self.store.list_workspaces(repository_id.as_deref())? {
            let Ok(path) = available_path(&workspace) else {
                continue;
            };
            let repository = self.repository_by_id(&workspace.repository_id)?;
            entries.push(WorkspacePathEntry {
                workspace_id: workspace.id,
                name: workspace.name,
                repository: RepositorySummary::from(&repository),
                path,
                branch_name: workspace.branch_name,
                worktree_mode: workspace.worktree_mode,
            });
        }
        Ok(entries)
    }
}

fn available_path(workspace: &Workspace) -> Result<PathBuf, CoordinatorError> {
    if workspace.availability != WorkspaceAvailability::Open
        || workspace.lifecycle != WorkspaceLifecycle::Ready
    {
        return Err(CoordinatorError::InvalidWorkspaceState {
            expected: "open with a prepared worktree (use `coco reopen` for a closed workspace)",
            actual: workspace.phase,
        });
    }
    let path = workspace
        .worktree_path
        .as_ref()
        .ok_or(CoordinatorError::IncompleteWorkspace("worktree"))?;
    if !path.is_absolute() || !path.is_dir() {
        return Err(CoordinatorError::WorktreeUnavailable(path.clone()));
    }
    Ok(path.clone())
}
