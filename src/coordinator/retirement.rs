use std::fs;
use std::path::Path;

use serde_json::{Value, json};
use tracing::warn;

use super::{Coordinator, CoordinatorError, NativeThread, WorkerError};
use crate::domain::hooks::{GuardAction, HookDispatch, HookEventKind};
use crate::domain::{
    CodexThreadStatus, Repository, Workspace, WorkspaceAvailability, WorkspaceLifecycle,
    WorktreeMode,
};
use crate::git::{GitError, GitRepository, WorktreeBinding};
use crate::protocol::{
    WorkspaceCloseParams, WorkspaceCloseResult, WorkspaceDeleteParams, WorkspaceDeleteResult,
    WorkspaceReopenParams, WorkspaceReopenResult, WorkspaceRetirementPlan,
    WorkspaceThreadDisposition,
};
use crate::store::WorkspaceDeletionIntent;

mod safety;

struct PreparedClose {
    workspace: Workspace,
    registered_repository: Repository,
    repository: GitRepository,
    binding: WorktreeBinding,
    plan: WorkspaceRetirementPlan,
}

struct PreparedDelete {
    workspace: Workspace,
    registered_repository: Repository,
    repository: GitRepository,
    plan: WorkspaceRetirementPlan,
}

impl Coordinator {
    pub(crate) async fn close_workspace(
        &self,
        params: WorkspaceCloseParams,
    ) -> Result<WorkspaceCloseResult, CoordinatorError> {
        let resolved = self.resolve_workspace(&params.scope, &params.workspace)?;
        let repository_lock = self.repository_lock(&resolved.repository_id).await;
        let _guard = repository_lock.lock().await;
        let prepared = self.prepare_close(&params).await?;
        if params.dry_run {
            return Ok(WorkspaceCloseResult {
                workspace: prepared.workspace,
                plan: prepared.plan,
                applied: false,
            });
        }
        reject_blocked_plan(&prepared.plan)?;
        validate_expected_plan(params.expected_plan.as_ref(), &prepared.plan)?;
        self.apply_close(prepared, params.discard_changes).await
    }

    async fn prepare_close(
        &self,
        params: &WorkspaceCloseParams,
    ) -> Result<PreparedClose, CoordinatorError> {
        let workspace = self.resolve_workspace(&params.scope, &params.workspace)?;
        require_availability(&workspace, WorkspaceAvailability::Open)?;
        if workspace.lifecycle != WorkspaceLifecycle::Ready {
            return Err(CoordinatorError::InvalidWorkspaceState {
                expected: "ready and open",
                actual: workspace.phase,
            });
        }
        let (registered_repository, repository) = self.git_repository_for_workspace(&workspace)?;
        let worktree_path = workspace
            .worktree_path
            .as_deref()
            .ok_or(CoordinatorError::IncompleteWorkspace("worktree"))?;
        self.validate_retirement_path(&workspace, &repository)?;
        let observation = self.git.observe_worktree_retirement(
            &repository,
            worktree_path,
            workspace.worktree_mode,
            workspace.branch_name.as_deref(),
        )?;
        let mut blockers = close_git_blockers(&workspace, &observation, params.discard_changes);
        let thread_disposition = close_thread_disposition(&workspace, params.archive_thread);
        self.collect_runtime_blockers(&workspace, thread_disposition, &mut blockers)
            .await;
        let descendant_thread_count = self
            .collect_close_descendant_blockers(&workspace, thread_disposition, &mut blockers)
            .await;
        let plan = close_plan(
            &workspace,
            &observation,
            thread_disposition,
            descendant_thread_count,
            blockers,
        );
        Ok(PreparedClose {
            workspace,
            registered_repository,
            repository,
            binding: observation.binding,
            plan,
        })
    }

    async fn apply_close(
        &self,
        prepared: PreparedClose,
        discard_changes: bool,
    ) -> Result<WorkspaceCloseResult, CoordinatorError> {
        self.hooks
            .check_guards(
                GuardAction::WorkspaceClose,
                &prepared.registered_repository,
                &prepared.workspace,
                json!({
                    "plan": &prepared.plan,
                    "archiveThread": prepared.plan.thread_disposition
                        == WorkspaceThreadDisposition::Archive,
                    "discardChanges": discard_changes,
                }),
            )
            .await?;
        let archive_thread =
            prepared.plan.thread_disposition == WorkspaceThreadDisposition::Archive;
        let closing = self.store.begin_workspace_close(
            &prepared.workspace.id,
            &prepared.binding.head_sha,
            archive_thread,
        )?;
        if let Err(source) = self.unsubscribe_for_close(&closing).await {
            self.rollback_close(&closing).await;
            return Err(source);
        }
        if archive_thread && let Err(source) = self.ensure_thread_archived(&closing).await {
            self.rollback_close(&closing).await;
            return Err(source);
        }
        if let Err(source) = self
            .ensure_worktree_quiescent(&closing, prepared.plan.thread_disposition)
            .await
        {
            self.rollback_close(&closing).await;
            return Err(source);
        }
        if let Err(source) =
            self.git
                .remove_worktree(&prepared.repository, &prepared.binding, discard_changes)
        {
            self.rollback_close(&closing).await;
            return Err(source.into());
        }
        let hook = self.plan_workspace_hook(
            HookEventKind::WorkspaceClosed,
            &closing,
            json!({
                "threadDisposition": prepared.plan.thread_disposition,
                "discardedChanges": discard_changes,
            }),
        )?;
        let notify_hook = hook.is_some();
        let workspace = self.store.transition_workspace_availability_with_hook(
            &closing.id,
            WorkspaceAvailability::Closing,
            WorkspaceAvailability::Closed,
            None,
            hook,
        )?;
        if notify_hook {
            self.hooks.notify();
        }
        Ok(WorkspaceCloseResult {
            workspace,
            plan: prepared.plan,
            applied: true,
        })
    }

    async fn unsubscribe_for_close(&self, workspace: &Workspace) -> Result<(), CoordinatorError> {
        let Some(thread_id) = workspace.codex_thread_id.as_deref() else {
            return Ok(());
        };
        if self.has_thread_subscription(thread_id) {
            self.worker.unsubscribe_thread(thread_id).await?;
            self.mark_thread_unsubscribed(thread_id);
        }
        Ok(())
    }

    pub(crate) async fn reopen_workspace(
        &self,
        params: WorkspaceReopenParams,
    ) -> Result<WorkspaceReopenResult, CoordinatorError> {
        let resolved = self.resolve_workspace(&params.scope, &params.workspace)?;
        let repository_lock = self.repository_lock(&resolved.repository_id).await;
        let _guard = repository_lock.lock().await;
        let workspace = self.resolve_workspace(&params.scope, &params.workspace)?;
        require_availability(&workspace, WorkspaceAvailability::Closed)?;
        let (_, repository) = self.git_repository_for_workspace(&workspace)?;
        let reopening = self.store.transition_workspace_availability(
            &workspace.id,
            WorkspaceAvailability::Closed,
            WorkspaceAvailability::Reopening,
            None,
        )?;
        let binding = match self.restore_closed_worktree(&repository, &reopening) {
            Ok(binding) => binding,
            Err(source) => {
                self.restore_closed_after_failed_reopen(&reopening, &repository, None)
                    .await;
                return Err(source);
            }
        };
        match self.finish_reopening(&reopening).await {
            Ok(workspace) => Ok(WorkspaceReopenResult { workspace }),
            Err(source) => {
                self.restore_closed_after_failed_reopen(&reopening, &repository, Some(&binding))
                    .await;
                Err(source)
            }
        }
    }

    fn restore_closed_worktree(
        &self,
        repository: &GitRepository,
        workspace: &Workspace,
    ) -> Result<WorktreeBinding, CoordinatorError> {
        let path = workspace
            .worktree_path
            .as_deref()
            .ok_or(CoordinatorError::IncompleteWorkspace("worktree"))?;
        self.validate_retirement_path(workspace, repository)?;
        let base_sha = workspace
            .base_sha
            .as_deref()
            .ok_or(CoordinatorError::IncompleteWorkspace("base SHA"))?;
        let closed_head =
            workspace
                .closed_head_sha
                .as_deref()
                .ok_or(CoordinatorError::IncompleteWorkspace(
                    "closed worktree HEAD",
                ))?;
        self.git
            .restore_worktree(
                repository,
                &self.worktrees_dir,
                &workspace.name,
                path,
                workspace.worktree_mode,
                workspace.branch_name.as_deref(),
                base_sha,
                closed_head,
            )
            .map_err(CoordinatorError::from)
    }

    async fn finish_reopening(&self, workspace: &Workspace) -> Result<Workspace, CoordinatorError> {
        if workspace.thread_archived {
            self.ensure_thread_unarchived(workspace).await?;
        }
        let hook =
            self.plan_workspace_hook(HookEventKind::WorkspaceReopened, workspace, json!({}))?;
        let notify_hook = hook.is_some();
        let reopened = self
            .store
            .transition_workspace_availability_with_hook(
                &workspace.id,
                WorkspaceAvailability::Reopening,
                WorkspaceAvailability::Open,
                None,
                hook,
            )
            .map_err(CoordinatorError::from)?;
        if notify_hook {
            self.hooks.notify();
        }
        Ok(reopened)
    }

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
        self.apply_delete(prepared, &params).await
    }

    async fn prepare_delete(
        &self,
        params: &WorkspaceDeleteParams,
    ) -> Result<PreparedDelete, CoordinatorError> {
        let workspace = self.resolve_workspace(&params.scope, &params.workspace)?;
        require_availability(&workspace, WorkspaceAvailability::Closed)?;
        let (registered_repository, repository) = self.git_repository_for_workspace(&workspace)?;
        let path = workspace
            .worktree_path
            .clone()
            .ok_or(CoordinatorError::IncompleteWorkspace("worktree"))?;
        self.validate_retirement_path(&workspace, &repository)?;
        let mut blockers = self.closed_workspace_blockers(&workspace, &repository, &path)?;
        if params.delete_branch {
            self.append_branch_delete_blockers(&workspace, &repository, &mut blockers)?;
        }
        let thread_disposition = delete_thread_disposition(&workspace, params.delete_thread);
        self.append_context_reference_blockers(&workspace, params.delete_thread, &mut blockers)?;
        if thread_disposition == WorkspaceThreadDisposition::Delete {
            self.collect_closed_thread_blockers(&workspace, &mut blockers)
                .await;
        }
        let descendant_thread_count = if thread_disposition == WorkspaceThreadDisposition::Delete {
            self.descendant_count_or_block(&workspace, &mut blockers)
                .await
        } else {
            0
        };
        let plan = delete_plan(
            &workspace,
            path,
            thread_disposition,
            params.delete_branch,
            descendant_thread_count,
            blockers,
        );
        Ok(PreparedDelete {
            workspace,
            registered_repository,
            repository,
            plan,
        })
    }

    async fn apply_delete(
        &self,
        prepared: PreparedDelete,
        params: &WorkspaceDeleteParams,
    ) -> Result<WorkspaceDeleteResult, CoordinatorError> {
        self.hooks
            .check_guards(
                GuardAction::WorkspaceDelete,
                &prepared.registered_repository,
                &prepared.workspace,
                json!({
                    "plan": &prepared.plan,
                    "deleteThread": params.delete_thread,
                    "deleteBranch": params.delete_branch,
                }),
            )
            .await?;
        let intent = WorkspaceDeletionIntent {
            delete_thread: params.delete_thread,
            delete_branch: params.delete_branch,
        };
        let hook = self.plan_workspace_hook(
            HookEventKind::WorkspaceDeleted,
            &prepared.workspace,
            json!({
                "threadDeleted": intent.delete_thread,
                "branchDeleted": intent.delete_branch,
            }),
        )?;
        let notify_hook = hook.is_some();
        let deleting = self
            .store
            .begin_workspace_deletion(&prepared.workspace.id, intent)?;
        let deleting = self
            .delete_native_thread_if_requested(deleting, intent)
            .await?;
        if let Err(source) =
            self.delete_branch_if_requested(&deleting, &prepared.repository, intent)
        {
            let _ = self.store.cancel_workspace_deletion(&deleting.id);
            return Err(source);
        }
        self.store
            .delete_workspace_record_with_hook(&deleting.id, hook)?;
        if notify_hook {
            self.hooks.notify();
        }
        Ok(WorkspaceDeleteResult {
            plan: prepared.plan,
            applied: true,
        })
    }

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
            let _ = self.store.cancel_workspace_deletion(&workspace.id);
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
                    let _ = self.store.cancel_workspace_deletion(&workspace.id);
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
        self.git
            .delete_created_branch_if_present(repository, branch, head)?;
        Ok(())
    }

    pub(crate) async fn recover_workspace_retirements(&self) -> usize {
        let workspaces = match self.store.transitional_workspaces() {
            Ok(workspaces) => workspaces,
            Err(source) => {
                warn!(%source, "could not inspect interrupted workspace retirement state");
                return 0;
            }
        };
        let mut recovered = 0;
        for workspace in workspaces {
            let repository_lock = self.repository_lock(&workspace.repository_id).await;
            let _guard = repository_lock.lock().await;
            match self.recover_workspace_retirement(&workspace).await {
                Ok(()) => recovered += 1,
                Err(source) => warn!(
                    workspace_id = %workspace.id,
                    %source,
                    "could not recover interrupted workspace retirement"
                ),
            }
        }
        recovered
    }

    async fn recover_workspace_retirement(
        &self,
        workspace: &Workspace,
    ) -> Result<(), CoordinatorError> {
        match workspace.availability {
            WorkspaceAvailability::Closing => self.recover_closing_workspace(workspace).await,
            WorkspaceAvailability::Reopening => self.recover_reopening_workspace(workspace).await,
            WorkspaceAvailability::Deleting => self.recover_deleting_workspace(workspace).await,
            WorkspaceAvailability::Open | WorkspaceAvailability::Closed => Ok(()),
        }
    }

    async fn collect_runtime_blockers(
        &self,
        workspace: &Workspace,
        thread_disposition: WorkspaceThreadDisposition,
        blockers: &mut Vec<String>,
    ) {
        if self.reject_workspace_attachment(&workspace.id).is_err() {
            blockers.push("workspace is attached to a Codex terminal UI".to_owned());
        }
        if workspace.active_turn_id.is_some()
            || self.runtime_turn_for_workspace(workspace).is_some()
        {
            blockers.push("workspace has an active or uncertain turn".to_owned());
        }
        if !self.open_decisions_for_workspace(&workspace.id).is_empty() {
            blockers.push("workspace has a pending approval or user question".to_owned());
        }
        let Some(thread_id) = workspace.codex_thread_id.as_deref() else {
            return;
        };
        match self.worker.locate_thread(thread_id).await {
            Ok(Some(located)) => {
                append_native_identity_blockers(workspace, &located.thread, blockers);
                if located.archived && thread_disposition != WorkspaceThreadDisposition::Archive {
                    blockers.push(
                        "Codex thread is already archived; retry with --archive-thread to restore it on reopen"
                            .to_owned(),
                    );
                }
                append_status_blocker(&located.thread.status, blockers);
                self.collect_background_terminal_blocker(
                    thread_id,
                    &located.thread.status,
                    blockers,
                )
                .await;
            }
            Ok(None) => blockers.push("Codex thread is unavailable".to_owned()),
            Err(_) => blockers.push("Codex thread status is unavailable".to_owned()),
        }
    }

    async fn collect_closed_thread_blockers(
        &self,
        workspace: &Workspace,
        blockers: &mut Vec<String>,
    ) {
        let Some(thread_id) = workspace.codex_thread_id.as_deref() else {
            return;
        };
        match self.worker.locate_thread(thread_id).await {
            Ok(Some(located)) => {
                append_native_identity_blockers(workspace, &located.thread, blockers);
                append_status_blocker(&located.thread.status, blockers);
                self.collect_background_terminal_blocker(
                    thread_id,
                    &located.thread.status,
                    blockers,
                )
                .await;
            }
            Ok(None) => {}
            Err(_) => blockers.push("Codex thread status is unavailable".to_owned()),
        }
    }

    async fn collect_background_terminal_blocker(
        &self,
        thread_id: &str,
        status: &CodexThreadStatus,
        blockers: &mut Vec<String>,
    ) {
        if matches!(status, CodexThreadStatus::NotLoaded) {
            return;
        }
        match self.worker.background_terminal_count(thread_id).await {
            Ok(0) => {}
            Ok(count) => blockers.push(format!(
                "Codex thread has {count} running background terminal(s)"
            )),
            Err(_) => blockers.push("Codex background-terminal state is unavailable".to_owned()),
        }
    }

    async fn descendant_count_or_block(
        &self,
        workspace: &Workspace,
        blockers: &mut Vec<String>,
    ) -> usize {
        let Some(thread_id) = workspace.codex_thread_id.as_deref() else {
            return 0;
        };
        match self.worker.list_thread_descendants(thread_id).await {
            Ok(descendants) => {
                if !descendants.is_empty() {
                    blockers.push(format!(
                        "native thread has {} descendant(s); Codex would apply this action recursively",
                        descendants.len()
                    ));
                }
                descendants.len()
            }
            Err(_) => {
                blockers.push("native thread descendants could not be verified".to_owned());
                0
            }
        }
    }

    async fn ensure_thread_archived(&self, workspace: &Workspace) -> Result<(), CoordinatorError> {
        let thread_id = required_thread_id(workspace)?;
        let located = self
            .worker
            .locate_thread(thread_id)
            .await?
            .ok_or_else(|| missing_native_thread(thread_id))?;
        validate_native_thread(workspace, &located.thread)?;
        if !located.archived {
            self.ensure_worktree_quiescent(workspace, WorkspaceThreadDisposition::Archive)
                .await?;
            self.worker.archive_thread(thread_id).await?;
        }
        self.mark_thread_unsubscribed(thread_id);
        Ok(())
    }

    async fn ensure_thread_unarchived(
        &self,
        workspace: &Workspace,
    ) -> Result<(), CoordinatorError> {
        let thread_id = required_thread_id(workspace)?;
        let located = self
            .worker
            .locate_thread(thread_id)
            .await?
            .ok_or_else(|| missing_native_thread(thread_id))?;
        validate_native_thread(workspace, &located.thread)?;
        if located.archived {
            let native = self.worker.unarchive_thread(thread_id).await?;
            validate_native_thread(workspace, &native)?;
        }
        self.mark_thread_unsubscribed(thread_id);
        Ok(())
    }

    async fn rollback_close(&self, workspace: &Workspace) {
        if let Err(source) = self.recover_closing_workspace(workspace).await {
            warn!(workspace_id = %workspace.id, %source, "close rollback awaits recovery");
        }
    }

    async fn restore_closed_after_failed_reopen(
        &self,
        workspace: &Workspace,
        repository: &GitRepository,
        binding: Option<&WorktreeBinding>,
    ) {
        if binding.is_none() {
            if let Err(source) = self.recover_reopening_workspace(workspace).await {
                warn!(workspace_id = %workspace.id, %source, "reopen recovery awaits startup");
            }
            return;
        }
        let disposition = close_thread_disposition(workspace, workspace.thread_archived);
        if let Err(source) = self.ensure_worktree_quiescent(workspace, disposition).await {
            warn!(workspace_id = %workspace.id, %source, "reopen rollback blocked by current worktree use");
            return;
        }
        if let Some(binding) = binding
            && let Err(source) = self.git.remove_worktree(repository, binding, false)
        {
            warn!(workspace_id = %workspace.id, %source, "reopen rollback awaits recovery");
            return;
        }
        if workspace.thread_archived
            && let Err(source) = self.ensure_thread_archived(workspace).await
        {
            warn!(workspace_id = %workspace.id, %source, "reopen rollback awaits recovery");
            return;
        }
        if let Err(source) = self.store.transition_workspace_availability(
            &workspace.id,
            WorkspaceAvailability::Reopening,
            WorkspaceAvailability::Closed,
            None,
        ) {
            warn!(workspace_id = %workspace.id, %source, "reopen rollback awaits recovery");
        }
    }

    async fn recover_closing_workspace(
        &self,
        workspace: &Workspace,
    ) -> Result<(), CoordinatorError> {
        let path = required_worktree_path(workspace)?;
        let (_, repository) = self.git_repository_for_workspace(workspace)?;
        self.validate_retirement_path(workspace, &repository)?;
        if path_exists(path)? {
            self.verify_recovery_worktree(workspace, &repository)?;
            if workspace.thread_archived {
                self.ensure_thread_unarchived(workspace).await?;
            }
            self.store.transition_workspace_availability(
                &workspace.id,
                WorkspaceAvailability::Closing,
                WorkspaceAvailability::Open,
                None,
            )?;
        } else {
            ensure_unregistered_worktree(&self.git, &repository, path)?;
            if workspace.thread_archived {
                self.ensure_thread_archived(workspace).await?;
            }
            let hook = self.plan_workspace_hook(
                HookEventKind::WorkspaceClosed,
                workspace,
                json!({"recovered": true}),
            )?;
            let notify_hook = hook.is_some();
            self.store.transition_workspace_availability_with_hook(
                &workspace.id,
                WorkspaceAvailability::Closing,
                WorkspaceAvailability::Closed,
                None,
                hook,
            )?;
            if notify_hook {
                self.hooks.notify();
            }
        }
        Ok(())
    }

    async fn recover_reopening_workspace(
        &self,
        workspace: &Workspace,
    ) -> Result<(), CoordinatorError> {
        let path = required_worktree_path(workspace)?;
        let (_, repository) = self.git_repository_for_workspace(workspace)?;
        self.validate_retirement_path(workspace, &repository)?;
        if !path_exists(path)? {
            ensure_unregistered_worktree(&self.git, &repository, path)?;
            if workspace.thread_archived {
                self.ensure_thread_archived(workspace).await?;
            }
            self.store.transition_workspace_availability(
                &workspace.id,
                WorkspaceAvailability::Reopening,
                WorkspaceAvailability::Closed,
                None,
            )?;
            return Ok(());
        }
        self.verify_recovery_worktree(workspace, &repository)?;
        self.finish_reopening(workspace).await?;
        Ok(())
    }

    async fn recover_deleting_workspace(
        &self,
        workspace: &Workspace,
    ) -> Result<(), CoordinatorError> {
        let _dependencies = self.context_dependencies.lock().await;
        let intent = self.store.workspace_deletion_intent(&workspace.id)?;
        let hook = self.plan_workspace_hook(
            HookEventKind::WorkspaceDeleted,
            workspace,
            json!({
                "threadDeleted": intent.delete_thread,
                "branchDeleted": intent.delete_branch,
                "recovered": true,
            }),
        )?;
        let notify_hook = hook.is_some();
        let (_, repository) = self.git_repository_for_workspace(workspace)?;
        self.validate_retirement_path(workspace, &repository)?;
        let mut blockers = self.closed_workspace_blockers(
            workspace,
            &repository,
            required_worktree_path(workspace)?,
        )?;
        self.append_context_reference_blockers(workspace, intent.delete_thread, &mut blockers)?;
        if let Err(source) = reject_blockers(blockers) {
            self.store.cancel_workspace_deletion(&workspace.id)?;
            return Err(source);
        }
        let deleting = self
            .delete_native_thread_if_requested(workspace.clone(), intent)
            .await?;
        if let Err(source) = self.delete_branch_if_requested(&deleting, &repository, intent) {
            let _ = self.store.cancel_workspace_deletion(&deleting.id);
            return Err(source);
        }
        self.store
            .delete_workspace_record_with_hook(&deleting.id, hook)?;
        if notify_hook {
            self.hooks.notify();
        }
        Ok(())
    }

    fn plan_workspace_hook(
        &self,
        kind: HookEventKind,
        workspace: &Workspace,
        data: Value,
    ) -> Result<Option<HookDispatch>, CoordinatorError> {
        let repository = self.repository_by_id(&workspace.repository_id)?;
        Ok(self.hooks.event(kind, &repository, workspace, data))
    }

    fn verify_recovery_worktree(
        &self,
        workspace: &Workspace,
        repository: &GitRepository,
    ) -> Result<(), CoordinatorError> {
        let binding = self.git.verify_worktree(
            repository,
            required_worktree_path(workspace)?,
            workspace.worktree_mode,
            workspace.branch_name.as_deref(),
        )?;
        let expected =
            workspace
                .closed_head_sha
                .as_deref()
                .ok_or(CoordinatorError::IncompleteWorkspace(
                    "closed worktree HEAD",
                ))?;
        if binding.head_sha != expected {
            return Err(GitError::BindingMismatch(format!(
                "worktree HEAD moved from {expected} to {} during recovery",
                binding.head_sha
            ))
            .into());
        }
        Ok(())
    }

    fn closed_workspace_blockers(
        &self,
        workspace: &Workspace,
        repository: &GitRepository,
        path: &Path,
    ) -> Result<Vec<String>, CoordinatorError> {
        let mut blockers = Vec::new();
        if path_exists(path)? || self.git.worktree_is_registered(repository, path)? {
            blockers.push("managed worktree still exists; reopen and close it first".to_owned());
        }
        if self.reject_workspace_attachment(&workspace.id).is_err() {
            blockers.push("workspace is attached to a Codex terminal UI".to_owned());
        }
        if !self.open_decisions_for_workspace(&workspace.id).is_empty() {
            blockers.push("workspace has a pending approval or user question".to_owned());
        }
        Ok(blockers)
    }

    fn append_branch_delete_blockers(
        &self,
        workspace: &Workspace,
        repository: &GitRepository,
        blockers: &mut Vec<String>,
    ) -> Result<(), CoordinatorError> {
        match (workspace.worktree_mode, workspace.branch_name.as_deref()) {
            (WorktreeMode::NewBranch, Some(branch)) => {
                let expected = workspace
                    .closed_head_sha
                    .as_deref()
                    .ok_or(CoordinatorError::IncompleteWorkspace("closed branch HEAD"))?;
                match self
                    .git
                    .validate_created_branch_deletion(repository, branch, expected)
                {
                    Ok(()) => {}
                    Err(GitError::BranchMoved { actual, .. }) => blockers.push(format!(
                        "branch {branch} moved from {expected} to {actual} after close"
                    )),
                    Err(GitError::BranchAlreadyCheckedOut { path, .. }) => blockers.push(format!(
                        "branch {branch} is checked out at {}",
                        path.display()
                    )),
                    Err(GitError::BranchNotFound(_)) => {
                        blockers.push(format!("branch {branch} is unavailable"));
                    }
                    Err(_) => blockers.push(format!("branch {branch} could not be verified")),
                }
            }
            (WorktreeMode::ExistingBranch, _) => blockers
                .push("--delete-branch cannot delete a branch CoCo did not create".to_owned()),
            _ => blockers.push("detached workspaces have no branch to delete".to_owned()),
        }
        Ok(())
    }
}

fn close_git_blockers(
    workspace: &Workspace,
    observation: &crate::git::WorktreeRetirementObservation,
    discard_changes: bool,
) -> Vec<String> {
    let mut blockers = Vec::new();
    if let Some(reason) = &observation.lock_reason {
        blockers.push(format!("Git worktree is locked: {reason}"));
    }
    if detached_commits(workspace, &observation.binding) {
        blockers
            .push("detached worktree contains commits that are not retained by a branch".into());
    }
    if observation.has_local_changes() && !discard_changes {
        blockers.push(
            "worktree contains local changes; pass --discard-changes after reviewing the plan"
                .into(),
        );
    }
    blockers
}

fn close_plan(
    workspace: &Workspace,
    observation: &crate::git::WorktreeRetirementObservation,
    thread_disposition: WorkspaceThreadDisposition,
    descendant_thread_count: usize,
    blockers: Vec<String>,
) -> WorkspaceRetirementPlan {
    WorkspaceRetirementPlan {
        workspace_id: workspace.id.clone(),
        workspace_name: workspace.name.clone(),
        worktree_path: observation.binding.path.clone(),
        head_sha: Some(observation.binding.head_sha.clone()),
        branch_name: workspace.branch_name.clone(),
        thread_id: workspace.codex_thread_id.clone(),
        thread_disposition,
        delete_branch: false,
        tracked_changes: observation.tracked_changes,
        untracked_file_count: observation.untracked_file_count,
        ignored_file_count: observation.ignored_file_count,
        detached_commits: detached_commits(workspace, &observation.binding),
        descendant_thread_count,
        blockers,
    }
}

fn delete_plan(
    workspace: &Workspace,
    worktree_path: std::path::PathBuf,
    thread_disposition: WorkspaceThreadDisposition,
    delete_branch: bool,
    descendant_thread_count: usize,
    blockers: Vec<String>,
) -> WorkspaceRetirementPlan {
    WorkspaceRetirementPlan {
        workspace_id: workspace.id.clone(),
        workspace_name: workspace.name.clone(),
        worktree_path,
        head_sha: workspace.closed_head_sha.clone(),
        branch_name: workspace.branch_name.clone(),
        thread_id: workspace.codex_thread_id.clone(),
        thread_disposition,
        delete_branch,
        tracked_changes: false,
        untracked_file_count: 0,
        ignored_file_count: 0,
        detached_commits: false,
        descendant_thread_count,
        blockers,
    }
}

fn close_thread_disposition(
    workspace: &Workspace,
    archive_thread: bool,
) -> WorkspaceThreadDisposition {
    if archive_thread && workspace.codex_thread_id.is_some() {
        WorkspaceThreadDisposition::Archive
    } else {
        WorkspaceThreadDisposition::Retain
    }
}

fn delete_thread_disposition(
    workspace: &Workspace,
    delete_thread: bool,
) -> WorkspaceThreadDisposition {
    if delete_thread && workspace.codex_thread_id.is_some() {
        WorkspaceThreadDisposition::Delete
    } else {
        WorkspaceThreadDisposition::Retain
    }
}

fn detached_commits(workspace: &Workspace, binding: &WorktreeBinding) -> bool {
    workspace.worktree_mode == WorktreeMode::Detached
        && workspace.base_sha.as_deref() != Some(&binding.head_sha)
}

fn require_availability(
    workspace: &Workspace,
    expected: WorkspaceAvailability,
) -> Result<(), CoordinatorError> {
    if workspace.availability == expected {
        Ok(())
    } else {
        Err(CoordinatorError::InvalidWorkspaceState {
            expected: match expected {
                WorkspaceAvailability::Open => "open",
                WorkspaceAvailability::Closed => "closed",
                WorkspaceAvailability::Closing
                | WorkspaceAvailability::Reopening
                | WorkspaceAvailability::Deleting => "a stable state",
            },
            actual: workspace.phase,
        })
    }
}

fn required_thread_id(workspace: &Workspace) -> Result<&str, CoordinatorError> {
    workspace
        .codex_thread_id
        .as_deref()
        .ok_or(CoordinatorError::IncompleteWorkspace("Codex thread"))
}

fn required_worktree_path(workspace: &Workspace) -> Result<&Path, CoordinatorError> {
    workspace
        .worktree_path
        .as_deref()
        .ok_or(CoordinatorError::IncompleteWorkspace("worktree"))
}

fn append_status_blocker(status: &CodexThreadStatus, blockers: &mut Vec<String>) {
    match status {
        CodexThreadStatus::Idle | CodexThreadStatus::NotLoaded => {}
        CodexThreadStatus::Active { active_flags } if active_flags.is_empty() => {
            blockers.push("Codex thread is active".to_owned());
        }
        CodexThreadStatus::Active { active_flags } => blockers.push(format!(
            "Codex thread is active ({})",
            active_flags.join(", ")
        )),
        CodexThreadStatus::SystemError => {
            blockers.push("Codex thread is in a system-error state".to_owned());
        }
    }
}

fn append_native_identity_blockers(
    workspace: &Workspace,
    native: &NativeThread,
    blockers: &mut Vec<String>,
) {
    if workspace.codex_thread_id.as_deref() != Some(&native.id) {
        blockers.push("Codex returned a different thread ID".to_owned());
    }
    if workspace.worktree_path.as_deref() != Some(&native.cwd) {
        blockers.push("Codex thread cwd no longer matches the workspace".to_owned());
    }
}

fn reject_blocked_plan(plan: &WorkspaceRetirementPlan) -> Result<(), CoordinatorError> {
    reject_blockers(plan.blockers.clone())
}

fn validate_expected_plan(
    expected: Option<&WorkspaceRetirementPlan>,
    current: &WorkspaceRetirementPlan,
) -> Result<(), CoordinatorError> {
    if expected.is_some_and(|expected| expected != current) {
        return Err(CoordinatorError::WorkspaceRetirementBlocked(vec![
            "workspace retirement plan changed; review it again before applying".to_owned(),
        ]));
    }
    Ok(())
}

fn reject_blockers(blockers: Vec<String>) -> Result<(), CoordinatorError> {
    if blockers.is_empty() {
        Ok(())
    } else {
        Err(CoordinatorError::WorkspaceRetirementBlocked(blockers))
    }
}

fn validate_native_thread(workspace: &Workspace, native: &NativeThread) -> Result<(), WorkerError> {
    let expected_id = workspace
        .codex_thread_id
        .as_deref()
        .ok_or(WorkerError::InvalidResponse("thread.id"))?;
    if native.id != expected_id {
        return Err(WorkerError::ThreadIdMismatch {
            expected: expected_id.to_owned(),
            actual: native.id.clone(),
        });
    }
    let expected_cwd = workspace
        .worktree_path
        .as_deref()
        .ok_or(WorkerError::InvalidResponse("thread.cwd"))?;
    if native.cwd != expected_cwd {
        return Err(WorkerError::CwdMismatch {
            expected: expected_cwd.to_owned(),
            actual: native.cwd.clone(),
        });
    }
    Ok(())
}

fn missing_native_thread(thread_id: &str) -> CoordinatorError {
    WorkerError::InvalidThreadRead(format!("Codex thread {thread_id} does not exist")).into()
}

fn path_exists(path: &Path) -> Result<bool, CoordinatorError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(GitError::Io {
            path: path.to_owned(),
            source,
        }
        .into()),
    }
}

fn ensure_unregistered_worktree(
    git: &crate::git::Git,
    repository: &GitRepository,
    path: &Path,
) -> Result<(), CoordinatorError> {
    if git.worktree_is_registered(repository, path)? {
        return Err(GitError::BindingMismatch(format!(
            "worktree {} is still registered after its directory disappeared",
            path.display()
        ))
        .into());
    }
    Ok(())
}
