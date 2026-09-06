use std::path::Path;

use chrono::Utc;
use serde_json::{Value, json};
use tracing::warn;

use super::{Coordinator, CoordinatorError, validate_non_empty, validate_operation_id};
use crate::domain::{
    Audit, ContextMode, EventKind, EventSource, ProfileSnapshot, Repository, Workspace,
    WorkspaceLifecycle, WorkspacePhase,
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

struct CreationContext {
    mode: ContextMode,
    base_ref: String,
    base_sha: String,
    fork: Option<ForkContext>,
}

struct ForkContext {
    requested_reference: String,
    workspace_id: String,
    workspace_name: String,
    thread_id: String,
    compact: bool,
}

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
        if let Some(model) = params.model.as_deref() {
            validate_non_empty("model", model)?;
        }
        validate_operation_id(&params.operation_id)?;
        validate_context_request(&params)?;

        let (repository, git_repository) =
            self.registered_repository_for_path(&params.repository_path)?;
        let repository_lock = self.repository_lock(&repository.id).await;
        let guard = repository_lock.lock().await;

        if let Some(existing) = self
            .store
            .workspace_by_create_operation_id(&params.operation_id)?
        {
            ensure_create_replay_matches(&existing, &params, &repository.id)?;
            return self.workspace_response(existing);
        }

        let mut loaded_profile = load_profile(&params.profile, &self.codex_home)?;
        loaded_profile.snapshot.model_override = params.model.clone();
        let context = self.resolve_creation_context(&params, &repository, &git_repository)?;
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
            &context.base_sha,
        )?;

        let workspace = self.persist_prepared_workspace(
            &params,
            &repository,
            &context,
            &loaded_profile,
            &plan,
        )?;

        let binding = self.create_workspace_worktree(&git_repository, &plan, &workspace.id)?;

        let started_thread = match self
            .start_context_thread(
                &params.name,
                &context,
                &binding.path,
                loaded_profile.thread_config,
                params.model.as_deref(),
            )
            .await
        {
            Ok(thread) => thread,
            Err(source) => {
                let error = CoordinatorError::Worker(source);
                self.mark_workspace_failed(
                    &workspace.id,
                    if context.fork.is_some() {
                        "thread.fork"
                    } else {
                        "thread.start"
                    },
                    &error,
                    EventSource::Codex,
                );
                return Err(error);
            }
        };
        let compact = context.fork.as_ref().is_some_and(|fork| fork.compact);
        let workspace = self.bind_context_thread(
            &workspace.id,
            &context,
            loaded_profile.snapshot,
            &started_thread,
            compact,
        )?;
        drop(guard);
        self.finish_context_preparation(workspace, &started_thread.id, compact)
            .await
    }

    fn bind_context_thread(
        &self,
        workspace_id: &str,
        context: &CreationContext,
        profile: ProfileSnapshot,
        started_thread: &super::StartedThread,
        compact: bool,
    ) -> Result<Workspace, CoordinatorError> {
        let effective_profile = with_effective_thread_settings(profile, &started_thread.response);
        self.store
            .update_workspace_profile(workspace_id, &effective_profile)?;
        let next_lifecycle = if compact {
            WorkspaceLifecycle::Starting
        } else {
            WorkspaceLifecycle::Ready
        };
        Ok(self
            .store
            .bind_thread_with_event(
                workspace_id,
                WorkspaceLifecycle::Starting,
                next_lifecycle,
                NewThreadBinding {
                    thread_id: started_thread.id.clone(),
                    parent_thread_id: context.fork.as_ref().map(|fork| fork.thread_id.clone()),
                    status: started_thread.status.clone(),
                    runtime_generation: self.runtime_generation.clone(),
                },
                EventDraft::workspace(
                    EventKind::AgentStarted,
                    EventSource::Codex,
                    json!({
                        "threadId": started_thread.id,
                        "parentThreadId": context.fork.as_ref().map(|fork| &fork.thread_id),
                        "contextMode": context.mode,
                        "compactRequested": compact,
                    }),
                ),
            )?
            .0)
    }

    async fn finish_context_preparation(
        &self,
        workspace: Workspace,
        thread_id: &str,
        compact: bool,
    ) -> Result<WorkspaceResult, CoordinatorError> {
        if !compact {
            return self.workspace_response(workspace);
        }
        if let Err(error) = self.compact_thread(thread_id).await {
            self.mark_workspace_failed(&workspace.id, "thread.compact", &error, EventSource::Codex);
            return Err(error);
        }
        let (workspace, _) = self.store.transition_workspace_lifecycle_with_event(
            &workspace.id,
            WorkspaceLifecycle::Starting,
            WorkspaceLifecycle::Ready,
            None,
            EventDraft {
                workspace_id: Some(workspace.id.clone()),
                turn_id: None,
                kind: EventKind::ContextCompacted,
                source: EventSource::Codex,
                source_method: Some("thread/compact/start".to_owned()),
                occurred_at_ms: None,
                payload: json!({"threadId": thread_id}),
            },
        )?;
        self.workspace_response(workspace)
    }

    fn resolve_creation_context(
        &self,
        params: &WorkspaceCreateParams,
        repository: &Repository,
        git_repository: &GitRepository,
    ) -> Result<CreationContext, CoordinatorError> {
        if params.context_mode == ContextMode::Fresh {
            self.git.assert_clean(git_repository)?;
            return Ok(CreationContext {
                mode: ContextMode::Fresh,
                base_ref: params.base_ref.clone(),
                base_sha: self.git.resolve_commit(git_repository, &params.base_ref)?,
                fork: None,
            });
        }

        let requested_reference = params
            .fork_from
            .as_deref()
            .expect("fork requests were validated before resolution");
        let source = self.resolve_workspace(
            &RepositoryScope::repository(&repository.root_path),
            requested_reference,
        )?;
        if source.phase != WorkspacePhase::Idle {
            return Err(CoordinatorError::InvalidWorkspaceState {
                expected: "an idle source workspace",
                actual: source.phase,
            });
        }
        let source_worktree = source
            .worktree_path
            .as_deref()
            .ok_or(CoordinatorError::IncompleteWorkspace("source worktree"))?;
        let source_thread_id = source
            .codex_thread_id
            .as_deref()
            .ok_or(CoordinatorError::IncompleteWorkspace("source Codex thread"))?;
        let source_repository = self.git.discover(source_worktree)?;
        if source_repository.git_common_dir != git_repository.git_common_dir {
            return Err(CoordinatorError::InvalidParams(
                "fork source must belong to the destination repository".to_owned(),
            ));
        }
        self.git.assert_clean(&source_repository)?;
        let base_sha = self.git.resolve_commit(&source_repository, "HEAD")?;
        Ok(CreationContext {
            mode: ContextMode::Fork,
            base_ref: "HEAD".to_owned(),
            base_sha,
            fork: Some(ForkContext {
                requested_reference: requested_reference.to_owned(),
                workspace_id: source.id,
                workspace_name: source.name,
                thread_id: source_thread_id.to_owned(),
                compact: params.compact,
            }),
        })
    }

    async fn start_context_thread(
        &self,
        name: &str,
        context: &CreationContext,
        cwd: &Path,
        config: Value,
        model: Option<&str>,
    ) -> Result<super::StartedThread, super::WorkerError> {
        match &context.fork {
            Some(fork) => {
                self.worker
                    .fork_thread(name, &fork.thread_id, cwd, config, model)
                    .await
            }
            None => self.worker.start_thread(name, cwd, config, model).await,
        }
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
        context: &CreationContext,
        loaded_profile: &crate::profile::LoadedProfile,
        plan: &crate::git::WorktreePlan,
    ) -> Result<Workspace, CoordinatorError> {
        let context_descriptor = match &context.fork {
            Some(fork) => json!({
                "version": 2,
                "mode": context.mode,
                "baseRef": context.base_ref,
                "forkFrom": fork.requested_reference,
                "sourceWorkspaceId": fork.workspace_id,
                "sourceWorkspaceName": fork.workspace_name,
                "sourceThreadId": fork.thread_id,
                "sourceHeadSha": context.base_sha,
                "compact": fork.compact,
            }),
            None => json!({
                "version": 2,
                "mode": context.mode,
                "baseRef": context.base_ref,
                "compact": false,
            }),
        };
        let (workspace, _) = self.store.create_workspace_with_event(
            NewWorkspace {
                create_operation_id: Some(params.operation_id.clone()),
                repository_id: repository.id.clone(),
                name: params.name.clone(),
                context_mode: context.mode,
                context: context_descriptor,
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
        && existing.context_mode == params.context_mode
        && existing.context.get("baseRef").and_then(Value::as_str)
            == Some(params.base_ref.as_str())
        && existing.context.get("forkFrom").and_then(Value::as_str) == params.fork_from.as_deref()
        && existing
            .context
            .get("compact")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            == params.compact
        && existing.profile.name == params.profile
        && existing.profile.model_override == params.model;
    if matches {
        Ok(())
    } else {
        Err(CoordinatorError::IdempotencyConflict)
    }
}

fn validate_context_request(params: &WorkspaceCreateParams) -> Result<(), CoordinatorError> {
    match params.context_mode {
        ContextMode::Fresh if params.fork_from.is_some() || params.compact => {
            Err(CoordinatorError::InvalidParams(
                "fresh context cannot use forkFrom or compact".to_owned(),
            ))
        }
        ContextMode::Fresh => Ok(()),
        ContextMode::Fork => {
            let source = params.fork_from.as_deref().ok_or_else(|| {
                CoordinatorError::InvalidParams("fork context requires forkFrom".to_owned())
            })?;
            validate_non_empty("forkFrom", source)?;
            if params.base_ref != "HEAD" {
                return Err(CoordinatorError::InvalidParams(
                    "baseRef cannot be overridden when forking a workspace".to_owned(),
                ));
            }
            Ok(())
        }
        ContextMode::Handoff => Err(CoordinatorError::UnsupportedContext(
            ContextMode::Handoff.as_str().to_owned(),
        )),
    }
}

fn workspace_git_binding(workspace: &Workspace) -> Option<(&Path, &str, &str)> {
    Some((
        workspace.worktree_path.as_deref()?,
        workspace.branch_name.as_deref()?,
        workspace.base_sha.as_deref()?,
    ))
}
