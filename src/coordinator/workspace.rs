use std::path::Path;

use chrono::Utc;
use serde_json::{Value, json};
use tracing::warn;

use super::{Coordinator, CoordinatorError, validate_non_empty, validate_operation_id};
use crate::domain::{
    Audit, ContextMode, EventKind, EventSource, Repository, Workspace, WorkspaceLifecycle,
    WorkspacePhase,
};
use crate::git::{GitRepository, WorktreeBinding, WorktreePlan};
use crate::profile::{load_profile, with_effective_thread_settings};
use crate::protocol::{
    AuditRecordParams, EventListParams, EventListResult, GitIncomplete, GitObservationError,
    GitUnavailable, RepositoryListParams, RepositoryRegisterParams, RepositoryScope,
    RepositorySummary, WorkspaceCreateParams, WorkspaceDiffParams, WorkspaceDiffResult,
    WorkspaceGetParams, WorkspaceGitStatus, WorkspaceListItem, WorkspaceListParams,
    WorkspaceResult, WorkspaceStatusResult,
};
use crate::store::{AuditDraft, EventDraft, NewThreadBinding, NewWorkspace};

const DEFAULT_DIFF_BYTES: usize = 4 * 1024 * 1024;
const MAX_DIFF_BYTES: usize = 16 * 1024 * 1024;

impl Coordinator {
    pub(crate) fn register_repository(
        &self,
        params: RepositoryRegisterParams,
    ) -> Result<Repository, CoordinatorError> {
        let discovered = self.git.discover(params.path)?;
        let now = Utc::now().timestamp_millis();
        let repository = self.store.register_repository(&Repository {
            id: discovered.id,
            root_path: discovered.root_path,
            git_common_dir: discovered.git_common_dir,
            display_name: discovered.display_name,
            is_linked_worktree: discovered.is_linked_worktree,
            created_at_ms: now,
            updated_at_ms: now,
        })?;
        Ok(repository)
    }

    pub(crate) fn list_repositories(
        &self,
        _params: RepositoryListParams,
    ) -> Result<Vec<RepositorySummary>, CoordinatorError> {
        Ok(self
            .store
            .list_repositories()?
            .iter()
            .map(RepositorySummary::from)
            .collect())
    }

    pub(crate) async fn create_workspace(
        &self,
        params: WorkspaceCreateParams,
    ) -> Result<WorkspaceResult, CoordinatorError> {
        validate_non_empty("baseRef", &params.base_ref)?;
        validate_operation_id(&params.operation_id)?;
        let context_mode = params.context_mode;
        if context_mode != ContextMode::Fresh {
            return Err(CoordinatorError::UnsupportedContext(
                context_mode.as_str().to_owned(),
            ));
        }

        let (repository, git_repository) =
            self.registered_repository_for_path(&params.repository_path)?;
        let repository_lock = self.repository_lock(&repository.id).await;
        let _guard = repository_lock.lock().await;

        if let Some(existing) = self
            .store
            .workspace_by_create_operation_id(&params.operation_id)?
        {
            ensure_create_replay_matches(&existing, &params, &repository.id)?;
            return self.workspace_response(existing);
        }

        let loaded_profile = load_profile(&params.profile, &self.codex_home)?;
        let base_sha = self.git.resolve_commit(&git_repository, &params.base_ref)?;
        self.git.assert_clean(&git_repository)?;
        if self
            .store
            .workspace_by_name(&repository.id, &params.name)?
            .is_some()
        {
            return Err(CoordinatorError::WorkspaceExists(params.name));
        }
        let plan = self.git.plan_worktree(
            &git_repository,
            &self.worktrees_dir,
            &params.name,
            &base_sha,
        )?;

        let workspace = self.persist_prepared_workspace(
            &params,
            &repository,
            context_mode,
            &loaded_profile,
            &plan,
        )?;

        let binding = self.create_workspace_worktree(&git_repository, &plan, &workspace.id)?;

        let started_thread = match self
            .worker
            .start_thread(&params.name, &binding.path, loaded_profile.thread_config)
            .await
        {
            Ok(thread) => thread,
            Err(source) => {
                let error = CoordinatorError::Worker(source);
                self.mark_workspace_failed(
                    &workspace.id,
                    "thread.start",
                    &error,
                    EventSource::Codex,
                );
                return Err(error);
            }
        };
        let effective_profile =
            with_effective_thread_settings(loaded_profile.snapshot, &started_thread.response);
        self.store
            .update_workspace_profile(&workspace.id, &effective_profile)?;
        let (workspace, _) = self.store.bind_thread_with_event(
            &workspace.id,
            WorkspaceLifecycle::Starting,
            NewThreadBinding {
                thread_id: started_thread.id.clone(),
                parent_thread_id: None,
                status: started_thread.status,
                runtime_generation: self.runtime_generation.clone(),
            },
            EventDraft::workspace(
                EventKind::AgentStarted,
                EventSource::Codex,
                json!({"threadId": started_thread.id}),
            ),
        )?;
        self.workspace_response(workspace)
    }

    fn create_workspace_worktree(
        &self,
        repository: &GitRepository,
        plan: &WorktreePlan,
        workspace_id: &str,
    ) -> Result<WorktreeBinding, CoordinatorError> {
        let binding = self
            .git
            .create_worktree(repository, plan)
            .map_err(|source| {
                let error = CoordinatorError::Git(source);
                self.mark_workspace_failed(
                    workspace_id,
                    "worktree.create",
                    &error,
                    EventSource::Git,
                );
                error
            })?;
        self.store.transition_workspace_lifecycle_with_event(
            workspace_id,
            WorkspaceLifecycle::Provisioning,
            WorkspaceLifecycle::Starting,
            None,
            EventDraft::workspace(
                EventKind::WorktreeCreated,
                EventSource::Git,
                json!({
                    "path": binding.path,
                    "branchName": binding.branch_name,
                    "headSha": binding.head_sha,
                }),
            ),
        )?;
        Ok(binding)
    }

    fn persist_prepared_workspace(
        &self,
        params: &WorkspaceCreateParams,
        repository: &Repository,
        context_mode: ContextMode,
        loaded_profile: &crate::profile::LoadedProfile,
        plan: &crate::git::WorktreePlan,
    ) -> Result<Workspace, CoordinatorError> {
        let (workspace, _) = self.store.create_workspace_with_event(
            NewWorkspace {
                create_operation_id: Some(params.operation_id.clone()),
                repository_id: repository.id.clone(),
                name: params.name.clone(),
                context_mode,
                context: json!({
                    "version": 1,
                    "mode": context_mode,
                    "baseRef": params.base_ref,
                }),
                profile: loaded_profile.snapshot.clone(),
                branch_name: Some(plan.branch_name.clone()),
                base_sha: Some(plan.base_sha.clone()),
                worktree_path: Some(plan.path.clone()),
            },
            EventDraft::workspace(
                EventKind::WorkspaceCreated,
                EventSource::Coco,
                json!({
                    "operationId": params.operation_id,
                    "name": params.name,
                    "baseSha": plan.base_sha,
                }),
            ),
        )?;
        Ok(workspace)
    }

    pub(crate) fn list_workspaces(
        &self,
        params: WorkspaceListParams,
    ) -> Result<Vec<WorkspaceListItem>, CoordinatorError> {
        let repository_id = match &params.scope {
            RepositoryScope::Repository { path } => {
                Some(self.registered_repository_for_path(path)?.0.id)
            }
            RepositoryScope::AllRepositories => None,
        };
        let mut workspaces = self.store.list_workspaces(repository_id.as_deref())?;
        if let Some(phases) = params.phases {
            let phases = phases
                .iter()
                .map(|phase| {
                    WorkspacePhase::parse(phase).ok_or_else(|| {
                        CoordinatorError::InvalidParams(format!(
                            "unknown workspace phase {phase:?}"
                        ))
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            workspaces.retain(|workspace| phases.contains(&workspace.phase));
        }
        workspaces
            .into_iter()
            .map(|workspace| self.workspace_list_item(workspace))
            .collect()
    }

    pub(crate) fn get_workspace(
        &self,
        params: WorkspaceGetParams,
    ) -> Result<WorkspaceStatusResult, CoordinatorError> {
        let workspace = self.resolve_workspace(&params.scope, &params.workspace)?;
        let (_, git_repository) = self.git_repository_for_workspace(&workspace)?;
        let git = match workspace_git_binding(&workspace) {
            Some((worktree, branch, base)) => {
                match self.git.observe(&git_repository, worktree, branch, base) {
                    Ok(observation) => WorkspaceGitStatus::Observed(observation),
                    Err(source) => {
                        warn!(workspace_id = %workspace.id, %source, "could not refresh workspace Git state");
                        WorkspaceGitStatus::Unavailable(GitUnavailable {
                            observed: false,
                            error: GitObservationError {
                                code: "GIT_OBSERVATION_FAILED".to_owned(),
                                message: "Git state could not be refreshed".to_owned(),
                            },
                        })
                    }
                }
            }
            None => WorkspaceGitStatus::Incomplete(GitIncomplete {
                observed: false,
                reason: "workspace has no complete Git binding".to_owned(),
            }),
        };
        let events = self.store.events_after(Some(&workspace.id), 0)?;
        let next_sequence = events.last().map_or(0, |event| event.sequence);
        let open_decisions = self
            .store
            .open_decisions_for_workspace(&workspace.id)?
            .into_iter()
            .map(|stored| stored.decision)
            .collect();
        Ok(WorkspaceStatusResult {
            workspace,
            git,
            open_decisions,
            next_sequence,
        })
    }

    pub(crate) fn list_events(
        &self,
        params: EventListParams,
    ) -> Result<EventListResult, CoordinatorError> {
        let workspace = self.resolve_workspace(&params.scope, &params.workspace)?;
        let events = self
            .store
            .events_after(Some(&workspace.id), params.after_sequence)?;
        let next_sequence = events
            .last()
            .map_or(params.after_sequence, |event| event.sequence);
        let open_decisions = self
            .store
            .open_decisions_for_workspace(&workspace.id)?
            .into_iter()
            .map(|stored| stored.decision)
            .collect();
        Ok(EventListResult {
            workspace,
            events,
            open_decisions,
            next_sequence,
        })
    }

    pub(crate) fn workspace_diff(
        &self,
        params: WorkspaceDiffParams,
    ) -> Result<WorkspaceDiffResult, CoordinatorError> {
        let workspace = self.resolve_workspace(&params.scope, &params.workspace)?;
        let worktree = workspace
            .worktree_path
            .as_deref()
            .ok_or(CoordinatorError::IncompleteWorkspace("worktree"))?;
        let base_sha = workspace
            .base_sha
            .as_deref()
            .ok_or(CoordinatorError::IncompleteWorkspace("base SHA"))?;
        let diff = self.git.diff(worktree, base_sha)?;
        let requested = params
            .max_bytes
            .map(|value| usize::try_from(value).unwrap_or(usize::MAX))
            .unwrap_or(DEFAULT_DIFF_BYTES)
            .min(MAX_DIFF_BYTES);
        let retained = diff.tracked_patch.len().min(requested);
        let patch = String::from_utf8_lossy(&diff.tracked_patch[..retained]);
        Ok(WorkspaceDiffResult {
            patch: patch.into_owned(),
            patch_truncated: diff.tracked_patch_truncated || retained < diff.tracked_patch.len(),
            untracked_paths: diff.untracked_paths,
        })
    }

    pub(crate) fn record_audit(
        &self,
        params: AuditRecordParams,
    ) -> Result<Audit, CoordinatorError> {
        let workspace_id = params.workspace_id.as_deref().and_then(|candidate| {
            if let Ok(Some(workspace)) = self.store.workspace_by_id(candidate) {
                return Some(workspace.id);
            }
            let repository_path = params.details.get("repositoryPath")?.as_str()?;
            let discovered = self.git.discover(repository_path).ok()?;
            let repository = self
                .store
                .repository_by_common_dir(&discovered.git_common_dir)
                .ok()??;
            let workspace = self
                .store
                .workspace_by_name(&repository.id, candidate)
                .ok()??;
            Some(workspace.id)
        });
        let audit = self.store.append_audit(AuditDraft {
            source: params.source,
            action: params.action,
            workspace_id,
            operation_id: params.operation_id,
            outcome: params.outcome,
            details: params.details,
            occurred_at_ms: None,
        })?;
        Ok(audit)
    }
}

fn ensure_create_replay_matches(
    existing: &Workspace,
    params: &WorkspaceCreateParams,
    repository_id: &str,
) -> Result<(), CoordinatorError> {
    let matches = existing.repository_id == repository_id
        && existing.name == params.name
        && existing.context_mode == ContextMode::Fresh
        && existing.context.get("baseRef").and_then(Value::as_str)
            == Some(params.base_ref.as_str())
        && existing.profile.name == params.profile;
    if matches {
        Ok(())
    } else {
        Err(CoordinatorError::IdempotencyConflict)
    }
}

fn workspace_git_binding(workspace: &Workspace) -> Option<(&Path, &str, &str)> {
    Some((
        workspace.worktree_path.as_deref()?,
        workspace.branch_name.as_deref()?,
        workspace.base_sha.as_deref()?,
    ))
}
