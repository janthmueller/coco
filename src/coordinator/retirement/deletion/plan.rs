use super::*;

impl Coordinator {
    pub(super) async fn prepare_delete(
        &self,
        params: &WorkspaceDeleteParams,
    ) -> Result<PreparedDelete, CoordinatorError> {
        let workspace = self.resolve_workspace(&params.scope, &params.workspace)?;
        if !matches!(
            workspace.availability,
            WorkspaceAvailability::Open | WorkspaceAvailability::Closed
        ) {
            return Err(CoordinatorError::InvalidWorkspaceState {
                expected: "open or closed",
                actual: workspace.phase,
            });
        }
        let (registered_repository, repository) = self.git_repository_for_workspace(&workspace)?;
        self.validate_retirement_path(&workspace, &repository)?;
        let path = required_worktree_path(&workspace)?.to_owned();
        let observation = if path_exists(&path)? {
            Some(self.git.observe_worktree_retirement(
                &repository,
                &path,
                workspace.worktree_mode,
                workspace.branch_name.as_deref(),
            )?)
        } else {
            ensure_unregistered_worktree(&self.git, &repository, &path)?;
            None
        };
        let mut plan = WorkspaceRetirementPlan {
            workspace_id: workspace.id.clone(),
            workspace_name: workspace.name.clone(),
            worktree_path: path,
            remove_worktree: observation.is_some(),
            head_sha: observation
                .as_ref()
                .map(|o| o.binding.head_sha.clone())
                .or_else(|| workspace.closed_head_sha.clone())
                .or_else(|| workspace.base_sha.clone()),
            branch_name: workspace.branch_name.clone(),
            thread_id: workspace.codex_thread_id.clone(),
            thread_disposition: delete_thread_disposition(&workspace, params.delete_thread),
            delete_branch: false,
            tracked_changes: false,
            untracked_file_count: 0,
            ignored_file_count: 0,
            detached_commits: false,
            unretained_commit_count: 0,
            descendant_thread_count: 0,
            blockers: Vec::new(),
        };
        if let Some(observation) = &observation {
            if workspace.availability == WorkspaceAvailability::Closed {
                plan.blockers.push(
                    "closed workspace has an unexpected worktree; inspect it before deleting"
                        .into(),
                );
            }
            plan.tracked_changes = observation.tracked_changes;
            plan.untracked_file_count = observation.untracked_file_count;
            plan.ignored_file_count = observation.ignored_file_count;
            plan.blockers
                .extend(close_git_blockers(observation, params.discard_changes));
        }
        self.plan_branch_deletion(&workspace, &repository, params, &mut plan)?;
        self.plan_commit_loss(&workspace, &repository, params, &mut plan)?;
        self.append_context_reference_blockers(
            &workspace,
            params.delete_thread,
            &mut plan.blockers,
        )?;
        plan.descendant_thread_count = self
            .collect_deletion_runtime_blockers(
                &workspace,
                plan.thread_disposition,
                plan.remove_worktree,
                &mut plan.blockers,
            )
            .await;
        Ok(PreparedDelete {
            workspace,
            registered_repository,
            repository,
            binding: observation.map(|o| o.binding),
            plan,
        })
    }

    fn plan_commit_loss(
        &self,
        workspace: &Workspace,
        repository: &GitRepository,
        params: &WorkspaceDeleteParams,
        plan: &mut WorkspaceRetirementPlan,
    ) -> Result<(), CoordinatorError> {
        if plan.delete_branch
            || (plan.remove_worktree && workspace.worktree_mode == WorktreeMode::Detached)
        {
            let head = plan
                .head_sha
                .as_deref()
                .ok_or(CoordinatorError::IncompleteWorkspace("worktree HEAD"))?;
            plan.unretained_commit_count = self.git.unretained_commit_count(
                repository,
                head,
                if plan.delete_branch {
                    plan.branch_name.as_deref()
                } else {
                    None
                },
            )?;
            plan.detached_commits = workspace.worktree_mode == WorktreeMode::Detached
                && plan.unretained_commit_count > 0;
            if plan.unretained_commit_count > 0 && !params.discard_unretained_commits {
                plan.blockers.push("commits are not retained by another branch or tag; keep a branch or pass --discard-unretained-commits".into());
            }
        }
        Ok(())
    }

    fn plan_branch_deletion(
        &self,
        workspace: &Workspace,
        repository: &GitRepository,
        params: &WorkspaceDeleteParams,
        plan: &mut WorkspaceRetirementPlan,
    ) -> Result<(), CoordinatorError> {
        let Some(branch) = plan.branch_name.as_deref() else {
            return Ok(());
        };
        match self.git.resolve_local_branch(repository, branch) {
            Err(GitError::BranchNotFound(_)) => {
                plan.branch_name = None;
                return Ok(());
            }
            Err(source) => return Err(source.into()),
            Ok(_) => {}
        }
        // A planned name alone does not prove ownership after a failed create.
        // With no verified worktree or completed provisioning, leave that ref.
        let owned = workspace.worktree_mode == WorktreeMode::NewBranch
            && (plan.remove_worktree
                || self.store.has_worktree_creation_event(&workspace.id)?
                || matches!(
                    workspace.lifecycle,
                    WorkspaceLifecycle::Ready | WorkspaceLifecycle::Completed
                ));
        if !params.delete_branch || !owned {
            return Ok(());
        }
        plan.delete_branch = true;
        let head = plan
            .head_sha
            .as_deref()
            .ok_or(CoordinatorError::IncompleteWorkspace("branch HEAD"))?;
        let removing_path = plan.remove_worktree.then_some(plan.worktree_path.as_path());
        if let Err(source) =
            self.git
                .validate_created_branch_deletion(repository, branch, head, removing_path)
        {
            plan.blockers.push(source.to_string());
        }
        Ok(())
    }

    pub(super) fn check_delete_commits(
        &self,
        repository: &GitRepository,
        plan: &WorkspaceRetirementPlan,
        discard_unretained_commits: bool,
        blockers: &mut Vec<String>,
    ) -> Result<(), CoordinatorError> {
        if plan.delete_branch || (plan.remove_worktree && plan.branch_name.is_none()) {
            let head = plan
                .head_sha
                .as_deref()
                .ok_or(CoordinatorError::IncompleteWorkspace("worktree HEAD"))?;
            let excluding = if plan.delete_branch {
                plan.branch_name.as_deref()
            } else {
                None
            };
            if !discard_unretained_commits
                && self
                    .git
                    .unretained_commit_count(repository, head, excluding)?
                    > 0
            {
                blockers
                    .push("commits are no longer retained elsewhere; review deletion again".into());
            }
        }
        Ok(())
    }

    pub(super) async fn collect_deletion_runtime_blockers(
        &self,
        workspace: &Workspace,
        disposition: WorkspaceThreadDisposition,
        remove_worktree: bool,
        blockers: &mut Vec<String>,
    ) -> usize {
        if self.reject_workspace_attachment(&workspace.id).is_err() {
            blockers.push("workspace is attached to a Codex terminal UI".into());
        }
        if workspace.active_turn_id.is_some()
            || self.runtime_turn_for_workspace(workspace).is_some()
        {
            blockers.push("workspace has an active or uncertain turn".into());
        }
        if !self.open_decisions_for_workspace(&workspace.id).is_empty() {
            blockers.push("workspace has a pending approval or user question".into());
        }
        if !remove_worktree && disposition != WorkspaceThreadDisposition::Delete {
            return 0;
        }
        let Some(thread_id) = workspace.codex_thread_id.as_deref() else {
            return 0;
        };
        let native = match self.worker.locate_thread(thread_id).await {
            Ok(Some(located)) => located.thread,
            Ok(None) => return 0, // Already deleted externally; no runtime can use the worktree.
            Err(_) => {
                blockers.push("Codex thread status is unavailable".into());
                return 0;
            }
        };
        append_native_identity_blockers(workspace, &native, blockers);
        append_status_blocker(&native.status, blockers);
        self.collect_background_terminal_blocker(thread_id, &native.status, blockers)
            .await;
        if disposition == WorkspaceThreadDisposition::Delete {
            self.descendant_count_or_block(workspace, blockers).await
        } else {
            self.collect_close_descendant_blockers(workspace, disposition, blockers)
                .await
        }
    }
}
