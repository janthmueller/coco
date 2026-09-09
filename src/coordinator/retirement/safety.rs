use super::*;

impl Coordinator {
    pub(super) fn validate_retirement_path(
        &self,
        workspace: &Workspace,
        repository: &GitRepository,
    ) -> Result<(), CoordinatorError> {
        self.git.validate_managed_worktree_path(
            repository,
            &self.worktrees_dir,
            &workspace.name,
            required_worktree_path(workspace)?,
        )?;
        Ok(())
    }

    pub(super) async fn ensure_worktree_quiescent(
        &self,
        workspace: &Workspace,
        disposition: WorkspaceThreadDisposition,
    ) -> Result<(), CoordinatorError> {
        let mut blockers = Vec::new();
        self.collect_runtime_blockers(workspace, disposition, &mut blockers)
            .await;
        self.collect_close_descendant_blockers(workspace, disposition, &mut blockers)
            .await;
        reject_blockers(blockers)
    }

    pub(super) async fn collect_close_descendant_blockers(
        &self,
        workspace: &Workspace,
        disposition: WorkspaceThreadDisposition,
        blockers: &mut Vec<String>,
    ) -> usize {
        if disposition == WorkspaceThreadDisposition::Archive {
            return self.descendant_count_or_block(workspace, blockers).await;
        }
        let Some(thread_id) = workspace.codex_thread_id.as_deref() else {
            return 0;
        };
        let descendants = match self.worker.list_thread_descendants(thread_id).await {
            Ok(descendants) => descendants,
            Err(_) => {
                blockers.push("native thread descendants could not be verified".to_owned());
                return 0;
            }
        };
        for descendant in &descendants {
            self.collect_descendant_worktree_blockers(workspace, descendant, blockers)
                .await;
        }
        descendants.len()
    }

    async fn collect_descendant_worktree_blockers(
        &self,
        workspace: &Workspace,
        thread_id: &str,
        blockers: &mut Vec<String>,
    ) {
        let native = match self.worker.locate_thread(thread_id).await {
            Ok(Some(located)) if located.thread.id == thread_id => located.thread,
            _ => {
                blockers.push(format!("descendant {thread_id} could not be verified"));
                return;
            }
        };
        if native.status == CodexThreadStatus::NotLoaded {
            return;
        }
        let Some(worktree) = workspace.worktree_path.as_deref() else {
            blockers.push("workspace has no worktree path".to_owned());
            return;
        };
        if !native.cwd.starts_with(worktree) {
            match fs::canonicalize(&native.cwd) {
                Ok(cwd) if !cwd.starts_with(worktree) => return,
                Ok(_) => {}
                Err(_) => {
                    blockers.push(format!(
                        "descendant {thread_id} working directory could not be verified"
                    ));
                    return;
                }
            }
        }
        let first = blockers.len();
        append_status_blocker(&native.status, blockers);
        self.collect_background_terminal_blocker(thread_id, &native.status, blockers)
            .await;
        for blocker in &mut blockers[first..] {
            *blocker = format!("descendant {thread_id} uses this worktree: {blocker}");
        }
    }

    pub(super) fn append_context_reference_blockers(
        &self,
        workspace: &Workspace,
        delete_thread: bool,
        blockers: &mut Vec<String>,
    ) -> Result<(), CoordinatorError> {
        let mut prepared = Vec::new();
        let mut materialized = Vec::new();
        for candidate in self.store.list_workspaces(None)? {
            if candidate.id == workspace.id {
                continue;
            }
            if let Some(dependency) =
                super::super::workspace::pending_context_dependency(&candidate)?
            {
                if dependency.workspace_id.as_deref() == Some(&workspace.id)
                    || (delete_thread
                        && workspace.codex_thread_id.as_deref() == Some(&dependency.thread_id))
                {
                    prepared.push(candidate.name);
                }
            } else if delete_thread
                && workspace.codex_thread_id.is_some()
                && candidate.codex_thread_id.is_some()
                && candidate.parent_thread_id == workspace.codex_thread_id
            {
                materialized.push(candidate.name);
            }
        }
        prepared.sort_unstable();
        materialized.sort_unstable();
        if !prepared.is_empty() {
            blockers.push(format!(
                "workspace supplies context for prepared workspace(s) {}; start or delete those workspaces first",
                prepared.join(", ")
            ));
        }
        if !materialized.is_empty() {
            blockers.push(format!(
                "Codex thread provides context for workspace(s) {}; delete those dependent threads first",
                materialized.join(", ")
            ));
        }
        Ok(())
    }
}
