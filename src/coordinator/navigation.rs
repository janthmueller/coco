use std::path::PathBuf;
use std::time::Duration;

use futures_util::{StreamExt, stream};

use super::{Coordinator, CoordinatorError};
use crate::domain::{Workspace, WorkspaceAvailability, WorkspaceLifecycle};
use crate::protocol::{
    RepositoryScope, RepositorySummary, WorkspacePathEntry, WorkspacePathGetParams,
    WorkspacePathListParams,
};

const RECENCY_BUDGET: Duration = Duration::from_secs(1);
const MAX_RECENCY_READS: usize = 4;

impl Coordinator {
    pub(crate) fn workspace_path(
        &self,
        params: WorkspacePathGetParams,
    ) -> Result<PathBuf, CoordinatorError> {
        let workspace = self.resolve_workspace(&params.scope, &params.workspace)?;
        available_path(&workspace)
    }

    pub(crate) async fn list_workspace_paths(
        &self,
        params: WorkspacePathListParams,
    ) -> Result<Vec<WorkspacePathEntry>, CoordinatorError> {
        let repository_id = match params.scope {
            RepositoryScope::Repository { path } => {
                Some(self.registered_repository_for_path(&path)?.0.id)
            }
            RepositoryScope::AllRepositories => None,
        };
        let workspaces = self.store.list_workspaces(repository_id.as_deref())?;
        let mut entries = Vec::new();
        let mut reads = Vec::new();
        for workspace in &workspaces {
            let Ok(path) = available_path(workspace) else {
                continue;
            };
            let repository = self.repository_by_id(&workspace.repository_id)?;
            entries.push(WorkspacePathEntry {
                workspace_id: workspace.id.clone(),
                name: workspace.name.clone(),
                repository: RepositorySummary::from(&repository),
                path,
                branch_name: workspace.branch_name.clone(),
                worktree_mode: workspace.worktree_mode,
                recency_at_ms: None,
            });
            if params.include_recency && workspace.codex_thread_id.is_some() {
                let index = entries.len() - 1;
                reads.push(async move {
                    let time = self
                        .read_bound_thread(workspace)
                        .await
                        .ok()
                        .and_then(|native| native.recency_at_ms)
                        .filter(|millis| *millis >= 0)
                        .filter(|millis| {
                            chrono::DateTime::from_timestamp_millis(*millis).is_some()
                        });
                    (index, time)
                });
            }
        }
        // One budget for the whole picker, not one timeout per workspace.
        // Cancelling these passive reads never changes the saved candidates.
        let mut reads = stream::iter(reads).buffer_unordered(MAX_RECENCY_READS);
        let _ = tokio::time::timeout(RECENCY_BUDGET, async {
            while let Some((index, time)) = reads.next().await {
                entries[index].recency_at_ms = time;
            }
        })
        .await;
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
