use super::{Coordinator, CoordinatorError, NativeThread, StartedThread, WorkerError};
use crate::domain::{
    CodexThreadStatus, ProfileSnapshot, Workspace, WorkspaceAvailability, WorkspaceLifecycle,
};
use crate::profile::load_profile;

impl Coordinator {
    /// Returns the bound native thread without loading it into the App Server.
    ///
    /// Binding validation lives here so passive projections and activating
    /// operations apply the same thread-ID and cwd invariants.
    pub(super) async fn read_bound_thread(
        &self,
        workspace: &Workspace,
    ) -> Result<NativeThread, CoordinatorError> {
        let thread_id = workspace
            .codex_thread_id
            .as_deref()
            .ok_or(CoordinatorError::IncompleteWorkspace("Codex thread"))?;
        let worktree = workspace
            .worktree_path
            .as_deref()
            .ok_or(CoordinatorError::IncompleteWorkspace("worktree"))?;
        let native = self.worker.read_thread(thread_id).await?;
        if native.id != thread_id {
            return Err(CoordinatorError::Worker(WorkerError::ThreadIdMismatch {
                expected: thread_id.to_owned(),
                actual: native.id,
            }));
        }
        if native.cwd != worktree {
            return Err(CoordinatorError::Worker(WorkerError::CwdMismatch {
                expected: worktree.to_owned(),
                actual: native.cwd,
            }));
        }
        Ok(native)
    }

    /// Ensures the daemon connection owns a live subscription for a workspace
    /// thread. Passive status reads deliberately never call this method.
    pub(super) async fn ensure_workspace_thread_loaded(
        &self,
        workspace: Workspace,
    ) -> Result<Workspace, CoordinatorError> {
        if workspace.lifecycle != WorkspaceLifecycle::Ready
            || workspace.availability != WorkspaceAvailability::Open
        {
            return Err(CoordinatorError::InvalidWorkspaceState {
                expected: "ready and open",
                actual: workspace.phase,
            });
        }

        if workspace.codex_thread_id.is_none() {
            return self.materialize_workspace_thread(workspace).await;
        }

        let native = self.read_bound_thread(&workspace).await?;
        let thread_id = native.id.clone();
        let needs_resume = native.status == CodexThreadStatus::NotLoaded
            || !self.has_thread_subscription(&thread_id);
        let status = if needs_resume {
            let resumed = self.resume_workspace_thread(&workspace).await?;
            if resumed.status == CodexThreadStatus::NotLoaded {
                return Err(CoordinatorError::Worker(WorkerError::InvalidThreadRead(
                    "thread remained notLoaded after thread/resume".to_owned(),
                )));
            }
            self.mark_thread_subscribed(&thread_id);
            resumed.status
        } else {
            native.status
        };
        Ok(self.project_native_thread_runtime(workspace, status))
    }

    async fn resume_workspace_thread(
        &self,
        workspace: &Workspace,
    ) -> Result<StartedThread, CoordinatorError> {
        let thread_id = workspace
            .codex_thread_id
            .as_deref()
            .ok_or(CoordinatorError::IncompleteWorkspace("Codex thread"))?;
        let worktree = workspace
            .worktree_path
            .as_deref()
            .ok_or(CoordinatorError::IncompleteWorkspace("worktree"))?;
        let profile = load_profile(&workspace.profile.name, &self.codex_home)?;
        if !same_profile_source(&profile.snapshot, &workspace.profile) {
            return Err(CoordinatorError::ProfileChanged(
                workspace.profile.name.clone(),
            ));
        }
        let resumed = self
            .worker
            .resume_thread(
                thread_id,
                worktree,
                profile.thread_config,
                workspace.profile.model_override.as_deref(),
            )
            .await?;
        if resumed.id != thread_id {
            return Err(CoordinatorError::Worker(WorkerError::ThreadIdMismatch {
                expected: thread_id.to_owned(),
                actual: resumed.id,
            }));
        }
        if resumed.cwd != worktree {
            return Err(CoordinatorError::Worker(WorkerError::CwdMismatch {
                expected: worktree.to_owned(),
                actual: resumed.cwd,
            }));
        }
        Ok(resumed)
    }
}

pub(super) fn same_profile_source(current: &ProfileSnapshot, stored: &ProfileSnapshot) -> bool {
    current.name == stored.name
        && current.source_path == stored.source_path
        && current.source_hash == stored.source_hash
}
