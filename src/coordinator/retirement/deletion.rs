use super::*;

mod plan;

struct PreparedDelete {
    workspace: Workspace,
    registered_repository: Repository,
    repository: GitRepository,
    binding: Option<WorktreeBinding>,
    plan: WorkspaceRetirementPlan,
}

impl Coordinator {
    pub(crate) async fn delete_workspace(
        &self,
        params: WorkspaceDeleteParams,
    ) -> Result<WorkspaceDeleteResult, CoordinatorError> {
        let resolved = self.resolve_workspace(&params.scope, &params.workspace)?;
        let repository_lock = self.repository_lock(&resolved.repository_id).await;
        let _guard = repository_lock.lock().await;
        let _dependencies = self.context_dependencies.lock().await;
        let prepared = self.prepare_delete(&params).await?;
        if params.dry_run {
            return Ok(WorkspaceDeleteResult {
                plan: prepared.plan,
                applied: false,
            });
        }
        reject_blocked_plan(&prepared.plan)?;
        validate_expected_plan(params.expected_plan.as_ref(), &prepared.plan)?;
        self.check_delete_guards(&prepared, &params).await?;
        // Guards execute user programs. Recheck their possible effects before
        // accepting the durable intent, not only before the later Git command.
        let checked = self.prepare_delete(&params).await?;
        validate_expected_plan(Some(&prepared.plan), &checked.plan)?;
        self.apply_delete(checked, &params).await
    }

    async fn check_delete_guards(
        &self,
        prepared: &PreparedDelete,
        params: &WorkspaceDeleteParams,
    ) -> Result<(), CoordinatorError> {
        if prepared.plan.remove_worktree {
            self.hooks
                .check_guards(
                    GuardAction::WorkspaceClose,
                    &prepared.registered_repository,
                    &prepared.workspace,
                    json!({"plan": &prepared.plan, "archiveThread": false,
                    "discardChanges": params.discard_changes, "deleting": true}),
                )
                .await?;
        }
        self.hooks.check_guards(
            GuardAction::WorkspaceDelete,
            &prepared.registered_repository,
            &prepared.workspace,
            json!({"plan": &prepared.plan,
                "deleteThread": prepared.plan.thread_disposition == WorkspaceThreadDisposition::Delete,
                "deleteBranch": prepared.plan.delete_branch,
                "discardChanges": params.discard_changes, "discardUnretainedCommits": params.discard_unretained_commits}),
        ).await?;
        Ok(())
    }

    async fn apply_delete(
        &self,
        prepared: PreparedDelete,
        params: &WorkspaceDeleteParams,
    ) -> Result<WorkspaceDeleteResult, CoordinatorError> {
        let intent = WorkspaceDeletionIntent {
            delete_thread: prepared.plan.thread_disposition == WorkspaceThreadDisposition::Delete,
            delete_branch: prepared.plan.delete_branch,
            discard_unretained_commits: params.discard_unretained_commits,
            from_open: prepared.workspace.availability == WorkspaceAvailability::Open,
        };
        let deleting = self.store.begin_workspace_deletion(
            &prepared.workspace.id,
            intent,
            prepared.plan.head_sha.as_deref(),
        )?;
        if let Err(source) = self.remove_deleting_worktree(&prepared, params).await {
            // Removal may have succeeded despite an I/O error. Recovery uses
            // actual verified path/registration evidence to choose open/closed.
            let _ = self.recover_deletion_before_thread(&deleting, &prepared.repository, intent);
            return Err(source);
        }
        self.finish_workspace_deletion(deleting, &prepared.repository, intent, false)
            .await
            .map_err(|source| CoordinatorError::WorkspaceDeletionIncomplete {
                workspace_id: prepared.workspace.id.clone(),
                source: Box::new(source),
            })?;
        Ok(WorkspaceDeleteResult {
            plan: prepared.plan,
            applied: true,
        })
    }

    async fn remove_deleting_worktree(
        &self,
        prepared: &PreparedDelete,
        params: &WorkspaceDeleteParams,
    ) -> Result<(), CoordinatorError> {
        self.validate_retirement_path(&prepared.workspace, &prepared.repository)?;
        let mut blockers = Vec::new();
        self.collect_deletion_runtime_blockers(
            &prepared.workspace,
            prepared.plan.thread_disposition,
            prepared.plan.remove_worktree,
            &mut blockers,
        )
        .await;
        self.check_delete_commits(
            &prepared.repository,
            &prepared.plan,
            params.discard_unretained_commits,
            &mut blockers,
        )?;
        reject_blockers(blockers)?;
        self.worker
            .stop_workspace_execution(&prepared.workspace.id)
            .await?;
        if let Some(binding) = &prepared.binding {
            self.git
                .remove_worktree(&prepared.repository, binding, params.discard_changes)?;
        } else {
            ensure_absent_worktree(
                &self.git,
                &prepared.repository,
                &prepared.plan.worktree_path,
            )?;
        }
        Ok(())
    }

    async fn finish_workspace_deletion(
        &self,
        workspace: Workspace,
        repository: &GitRepository,
        intent: WorkspaceDeletionIntent,
        recovered: bool,
    ) -> Result<(), CoordinatorError> {
        // Recheck branch loss before deleting native history as well as at the
        // final compare-and-delete; unrelated Git clients do not hold our lock.
        if let Err(source) = self.check_deleting_branch(&workspace, repository, intent) {
            let _ = self.store.cancel_workspace_deletion(&workspace.id, false);
            return Err(source);
        }
        let hook = self.plan_workspace_hook(
            HookEventKind::WorkspaceDeleted,
            &workspace,
            json!({"threadDeleted": intent.delete_thread, "branchDeleted": intent.delete_branch,
                "recovered": recovered}),
        )?;
        let notify = hook.is_some();
        let deleting = self
            .delete_native_thread_if_requested(workspace, intent)
            .await?;
        if let Err(source) = self.delete_branch_if_requested(&deleting, repository, intent) {
            let _ = self.store.cancel_workspace_deletion(&deleting.id, false);
            return Err(source);
        }
        self.store
            .delete_workspace_record_with_hook(&deleting.id, hook)?;
        if notify {
            self.hooks.notify();
        }
        Ok(())
    }

    pub(super) async fn recover_deleting_workspace(
        &self,
        workspace: &Workspace,
    ) -> Result<(), CoordinatorError> {
        let _dependencies = self.context_dependencies.lock().await;
        let intent = self.store.workspace_deletion_intent(&workspace.id)?;
        let (_, repository) = self.git_repository_for_workspace(workspace)?;
        self.validate_retirement_path(workspace, &repository)?;
        if self.recover_deletion_before_thread(workspace, &repository, intent)? {
            return Ok(());
        }
        let mut blockers = Vec::new();
        self.append_context_reference_blockers(workspace, intent.delete_thread, &mut blockers)?;
        self.collect_deletion_runtime_blockers(
            workspace,
            delete_thread_disposition(workspace, intent.delete_thread),
            false,
            &mut blockers,
        )
        .await;
        if let Err(source) = reject_blockers(blockers) {
            self.store.cancel_workspace_deletion(&workspace.id, false)?;
            return Err(source);
        }
        self.worker.stop_workspace_execution(&workspace.id).await?;
        self.finish_workspace_deletion(workspace.clone(), &repository, intent, true)
            .await
    }

    fn check_deleting_branch(
        &self,
        workspace: &Workspace,
        repository: &GitRepository,
        intent: WorkspaceDeletionIntent,
    ) -> Result<(), CoordinatorError> {
        if !intent.delete_branch {
            return Ok(());
        }
        let branch = workspace
            .branch_name
            .as_deref()
            .ok_or(CoordinatorError::IncompleteWorkspace("branch"))?;
        let head = workspace
            .closed_head_sha
            .as_deref()
            .ok_or(CoordinatorError::IncompleteWorkspace("closed branch HEAD"))?;
        match self
            .git
            .validate_created_branch_deletion(repository, branch, head, None)
        {
            Ok(()) => {}
            Err(GitError::BranchNotFound(_)) => return Ok(()),
            Err(source) => return Err(source.into()),
        }
        if !intent.discard_unretained_commits
            && self
                .git
                .unretained_commit_count(repository, head, Some(branch))?
                > 0
        {
            return Err(CoordinatorError::WorkspaceRetirementBlocked(vec![
                "branch commits are no longer retained elsewhere; review deletion again".into(),
            ]));
        }
        Ok(())
    }

    /// Never repeat a destructive file discard on restart. If the original
    /// verified worktree remains, restore open availability for a fresh plan.
    /// Once it is absent, only the pinned thread/branch cleanup can resume.
    fn recover_deletion_before_thread(
        &self,
        workspace: &Workspace,
        repository: &GitRepository,
        intent: WorkspaceDeletionIntent,
    ) -> Result<bool, CoordinatorError> {
        self.validate_retirement_path(workspace, repository)?;
        let path = required_worktree_path(workspace)?;
        if path_exists(path)? {
            if !intent.from_open {
                return Err(CoordinatorError::WorkspaceRetirementBlocked(vec![
                    "a worktree appeared after deletion was planned; inspect it before retrying"
                        .into(),
                ]));
            }
            self.verify_recovery_worktree(workspace, repository)?;
            self.store.cancel_workspace_deletion(&workspace.id, true)?;
            return Ok(true);
        }
        ensure_unregistered_worktree(&self.git, repository, path)?;
        Ok(false)
    }
}

impl Coordinator {
    async fn delete_native_thread_if_requested(
        &self,
        workspace: Workspace,
        intent: WorkspaceDeletionIntent,
    ) -> Result<Workspace, CoordinatorError> {
        if !intent.delete_thread || workspace.codex_thread_id.is_none() {
            return Ok(workspace);
        }
        let thread_id = required_thread_id(&workspace)?;
        let Some(located) = self.worker.locate_thread(thread_id).await? else {
            self.mark_thread_unsubscribed(thread_id);
            return self
                .store
                .clear_workspace_thread_binding(&workspace.id)
                .map_err(CoordinatorError::from);
        };
        let mut blockers = Vec::new();
        append_native_identity_blockers(&workspace, &located.thread, &mut blockers);
        append_status_blocker(&located.thread.status, &mut blockers);
        self.collect_background_terminal_blocker(thread_id, &located.thread.status, &mut blockers)
            .await;
        self.descendant_count_or_block(&workspace, &mut blockers)
            .await;
        if let Err(source) = reject_blockers(blockers) {
            let _ = self.store.cancel_workspace_deletion(&workspace.id, false);
            return Err(source);
        }
        if let Err(source) = self.worker.delete_thread(thread_id).await {
            match self.worker.locate_thread(thread_id).await {
                Ok(None) => {
                    self.mark_thread_unsubscribed(thread_id);
                    return self
                        .store
                        .clear_workspace_thread_binding(&workspace.id)
                        .map_err(CoordinatorError::from);
                }
                Ok(Some(_)) => {
                    let _ = self.store.cancel_workspace_deletion(&workspace.id, false);
                }
                Err(verification) => {
                    warn!(
                        workspace_id = %workspace.id,
                        %verification,
                        "native thread deletion result is ambiguous; recovery remains pending"
                    );
                }
            }
            return Err(source.into());
        }
        self.mark_thread_unsubscribed(thread_id);
        self.store
            .clear_workspace_thread_binding(&workspace.id)
            .map_err(CoordinatorError::from)
    }

    fn delete_branch_if_requested(
        &self,
        workspace: &Workspace,
        repository: &GitRepository,
        intent: WorkspaceDeletionIntent,
    ) -> Result<(), CoordinatorError> {
        if !intent.delete_branch {
            return Ok(());
        }
        let branch = workspace
            .branch_name
            .as_deref()
            .ok_or(CoordinatorError::IncompleteWorkspace("branch"))?;
        let head = workspace
            .closed_head_sha
            .as_deref()
            .ok_or(CoordinatorError::IncompleteWorkspace("closed branch HEAD"))?;
        self.git.delete_created_branch_if_present(
            repository,
            branch,
            head,
            intent.discard_unretained_commits,
        )?;
        Ok(())
    }
}

fn ensure_absent_worktree(
    git: &crate::git::Git,
    repository: &GitRepository,
    path: &Path,
) -> Result<(), CoordinatorError> {
    if path_exists(path)? {
        return Err(CoordinatorError::WorkspaceRetirementBlocked(vec![
            "managed worktree appeared after the deletion preview; review the plan again".into(),
        ]));
    }
    ensure_unregistered_worktree(git, repository, path)
}
