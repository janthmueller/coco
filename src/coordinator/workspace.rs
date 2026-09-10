use std::path::Path;

use chrono::Utc;
use serde_json::{Value, json};
use tracing::warn;

use super::{Coordinator, CoordinatorError, validate_non_empty, validate_operation_id};
use crate::domain::hooks::{HookDispatch, HookEventKind};
use crate::domain::{
    Audit, ContextMode, EventKind, EventSource, ProfileSnapshot, Repository, ThreadRuntimeSnapshot,
    Workspace, WorkspaceAvailability, WorkspaceLifecycle, WorkspacePhase, WorktreeMode,
    derive_workspace_runtime,
};
use crate::git::{GitRepository, LocalStateSnapshot, WorktreePlan, WorktreeTarget};
use crate::profile::{load_profile, with_effective_thread_settings};
use crate::protocol::{
    AuditRecordParams, EventListParams, EventListResult, GitIncomplete, GitObservationError,
    GitUnavailable, RepositoryListParams, RepositoryRegisterParams, RepositoryResolveParams,
    RepositoryScope, RepositorySummary, WorkspaceBaseRequest, WorkspaceContextRequest,
    WorkspaceContextSource, WorkspaceCreateParams, WorkspaceDiffParams, WorkspaceDiffResult,
    WorkspaceGetParams, WorkspaceGitStatus, WorkspaceListItem, WorkspaceListParams,
    WorkspaceResult, WorkspaceStatusResult, WorkspaceWorktreeRequest,
};
use crate::store::{
    AuditDraft, EventDraft, NewThreadBinding, NewWorkspace, Operation, ThreadOperationAcceptance,
};

const DEFAULT_DIFF_BYTES: usize = 4 * 1024 * 1024;
const MAX_DIFF_BYTES: usize = 16 * 1024 * 1024;

struct CreationContext {
    mode: ContextMode,
    fork: Option<ForkContext>,
}

pub(super) struct PendingFreshThread {
    pub(super) started: super::StartedThread,
    profile: ProfileSnapshot,
}

pub(super) struct PendingContextDependency {
    pub(super) thread_id: String,
    pub(super) workspace_id: Option<String>,
}

struct CreationBase {
    requested: WorkspaceBaseRequest,
    base_ref: String,
    base_sha: String,
    source_workspace: Option<BaseWorkspace>,
}

struct ResolvedCreation {
    base: CreationBase,
    context: CreationContext,
    worktree: WorktreePlan,
    local_state: LocalStateSnapshot,
}

struct BaseWorkspace {
    requested_reference: String,
    workspace_id: String,
    workspace_name: String,
}

struct ForkContext {
    source: ResolvedContextSource,
    thread_id: String,
    compact: bool,
}

enum ResolvedContextSource {
    Workspace {
        requested_reference: String,
        workspace_id: String,
        workspace_name: String,
        source_cwd: std::path::PathBuf,
    },
    Thread {
        source_cwd: std::path::PathBuf,
    },
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

    pub(crate) fn resolve_repository(
        &self,
        params: RepositoryResolveParams,
    ) -> Result<RepositorySummary, CoordinatorError> {
        let (repository, _) = self.registered_repository_for_path(&params.path)?;
        Ok(RepositorySummary::from(&repository))
    }

    pub(crate) async fn create_workspace(
        &self,
        params: WorkspaceCreateParams,
    ) -> Result<WorkspaceResult, CoordinatorError> {
        if let Some(model) = params.model.as_deref() {
            validate_non_empty("model", model)?;
        }
        validate_operation_id(&params.operation_id)?;
        validate_create_request(&params)?;

        let (repository, git_repository) =
            self.registered_repository_for_path(&params.repository_path)?;
        let repository_lock = self.repository_lock(&repository.id).await;
        let guard = repository_lock.lock().await;

        if let Some(existing) = self
            .store
            .workspace_by_create_operation_id(&params.operation_id)?
        {
            ensure_create_replay_matches(&existing, &params, &repository.id)?;
            let existing = self.hydrate_native_thread_runtime(existing).await;
            return self.workspace_response(existing);
        }

        let mut loaded_profile = load_profile(&params.profile, &self.codex_home)?;
        loaded_profile.snapshot.model_override = params.model.clone();
        let (base, target) =
            self.resolve_creation_worktree(&params, &repository, &git_repository)?;
        let local_state = self.git.snapshot_local_state(
            &git_repository,
            &base.base_sha,
            params.changes.carries_tracked(),
            params.changes.carries_untracked(),
        )?;
        // Cross-repository thread context can race source deletion. Hold the
        // dependency guard until the resolved source is durably recorded.
        let dependencies = self.context_dependencies.lock().await;
        let context = self.resolve_creation_context(&params, &repository).await?;
        if self
            .store
            .workspace_by_name(&repository.id, &params.name)?
            .is_some()
        {
            return Err(CoordinatorError::WorkspaceExists(params.name));
        }
        let worktree = self.git.plan_worktree(
            &git_repository,
            &self.worktrees_dir,
            &params.name,
            target,
            &base.base_sha,
        )?;
        let creation = ResolvedCreation {
            base,
            context,
            worktree,
            local_state,
        };

        let workspace =
            self.persist_prepared_workspace(&params, &repository, &loaded_profile, &creation)?;
        drop(dependencies);

        let hook = self.hooks.event(
            HookEventKind::WorkspaceCreated,
            &repository,
            &workspace,
            json!({
                "worktreeMode": creation.worktree.mode,
                "baseSha": creation.worktree.base_sha,
            }),
        );
        let notify_hook = hook.is_some();
        let workspace = self.create_workspace_worktree(
            &git_repository,
            &creation.worktree,
            &creation.local_state,
            &workspace.id,
            hook,
        )?;
        if notify_hook {
            self.hooks.notify();
        }
        drop(guard);
        self.workspace_response(workspace)
    }

    /// Materializes the native Codex conversation on the first activating
    /// operation. A prepared Git workspace deliberately has no empty native
    /// thread: current Codex versions do not persist one until its first turn.
    pub(super) async fn materialize_workspace_thread(
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
        if workspace.codex_thread_id.is_some() {
            return Ok(workspace);
        }
        let worktree = workspace
            .worktree_path
            .as_deref()
            .ok_or(CoordinatorError::IncompleteWorkspace("worktree"))?;
        let context = stored_creation_context(&workspace)?;
        self.validate_materialization_context(&context).await?;

        let mut loaded_profile = load_profile(&workspace.profile.name, &self.codex_home)?;
        if !super::recovery::same_profile_source(&loaded_profile.snapshot, &workspace.profile) {
            return Err(CoordinatorError::ProfileChanged(
                workspace.profile.name.clone(),
            ));
        }
        loaded_profile.snapshot.model_override = workspace.profile.model_override.clone();
        let started_thread = self
            .start_context_thread(
                &workspace.id,
                &workspace.name,
                &context,
                worktree,
                loaded_profile.thread_config,
                workspace.profile.model_override.as_deref(),
            )
            .await?;
        let compact = context.fork.as_ref().is_some_and(|fork| fork.compact);
        let workspace = self.bind_context_thread(
            &workspace.id,
            &context,
            loaded_profile.snapshot,
            &started_thread,
            compact,
        )?;
        self.finish_context_materialization(workspace, &started_thread.id, compact)
            .await
    }

    /// Starts a fresh native thread for an immediately following first turn,
    /// but deliberately leaves SQLite unbound until that turn is accepted.
    pub(super) async fn start_fresh_workspace_thread(
        &self,
        workspace: &Workspace,
    ) -> Result<PendingFreshThread, CoordinatorError> {
        if workspace.lifecycle != WorkspaceLifecycle::Ready
            || workspace.availability != WorkspaceAvailability::Open
        {
            return Err(CoordinatorError::InvalidWorkspaceState {
                expected: "ready and open",
                actual: workspace.phase,
            });
        }
        if workspace.codex_thread_id.is_some() || workspace.context_mode != ContextMode::Fresh {
            return Err(CoordinatorError::InvalidWorkspaceState {
                expected: "an unbound fresh workspace",
                actual: workspace.phase,
            });
        }
        let worktree = workspace
            .worktree_path
            .as_deref()
            .ok_or(CoordinatorError::IncompleteWorkspace("worktree"))?;
        let context = stored_creation_context(workspace)?;
        if context.mode != ContextMode::Fresh || context.fork.is_some() {
            return Err(CoordinatorError::InvalidParams(
                "stored fresh workspace context unexpectedly contains a fork source".to_owned(),
            ));
        }
        let mut loaded_profile = load_profile(&workspace.profile.name, &self.codex_home)?;
        if !super::recovery::same_profile_source(&loaded_profile.snapshot, &workspace.profile) {
            return Err(CoordinatorError::ProfileChanged(
                workspace.profile.name.clone(),
            ));
        }
        loaded_profile.snapshot.model_override = workspace.profile.model_override.clone();
        let started = self
            .worker
            .start_thread(
                &workspace.id,
                &workspace.name,
                worktree,
                loaded_profile.thread_config,
                workspace.profile.model_override.as_deref(),
            )
            .await?;
        Ok(PendingFreshThread {
            started,
            profile: loaded_profile.snapshot,
        })
    }

    pub(super) fn accept_fresh_thread_turn(
        &self,
        workspace: &Workspace,
        pending: &PendingFreshThread,
        operation_id: &str,
        native_turn_id: &str,
    ) -> Result<(Workspace, Operation), CoordinatorError> {
        let profile =
            with_effective_thread_settings(pending.profile.clone(), &pending.started.response);
        let (workspace, operation, _) =
            self.store
                .bind_thread_and_accept_operation(ThreadOperationAcceptance {
                    workspace_id: workspace.id.clone(),
                    profile,
                    binding: NewThreadBinding {
                        thread_id: pending.started.id.clone(),
                        parent_thread_id: None,
                    },
                    event: thread_started_event(
                        &pending.started.id,
                        None,
                        ContextMode::Fresh,
                        false,
                    ),
                    operation_id: operation_id.to_owned(),
                    native_result_id: native_turn_id.to_owned(),
                })?;
        self.mark_thread_subscribed(&pending.started.id);
        Ok((
            self.project_native_thread_runtime(workspace, pending.started.status.clone()),
            operation,
        ))
    }

    pub(super) fn bind_materialized_fresh_thread(
        &self,
        workspace: &Workspace,
        pending: &PendingFreshThread,
    ) -> Result<Workspace, CoordinatorError> {
        let context = CreationContext {
            mode: ContextMode::Fresh,
            fork: None,
        };
        self.bind_context_thread(
            &workspace.id,
            &context,
            pending.profile.clone(),
            &pending.started,
            false,
        )
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
        let workspace = self
            .store
            .bind_thread_with_event(
                workspace_id,
                WorkspaceLifecycle::Ready,
                next_lifecycle,
                NewThreadBinding {
                    thread_id: started_thread.id.clone(),
                    parent_thread_id: context.fork.as_ref().map(|fork| fork.thread_id.clone()),
                },
                thread_started_event(
                    &started_thread.id,
                    context.fork.as_ref().map(|fork| fork.thread_id.as_str()),
                    context.mode,
                    compact,
                ),
            )?
            .0;
        let workspace =
            self.project_native_thread_runtime(workspace, started_thread.status.clone());
        self.mark_thread_subscribed(&started_thread.id);
        Ok(workspace)
    }

    async fn finish_context_materialization(
        &self,
        workspace: Workspace,
        thread_id: &str,
        compact: bool,
    ) -> Result<Workspace, CoordinatorError> {
        if !compact {
            return Ok(workspace);
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
        Ok(self.hydrate_native_thread_runtime(workspace).await)
    }

    fn resolve_creation_worktree(
        &self,
        params: &WorkspaceCreateParams,
        repository: &Repository,
        git_repository: &GitRepository,
    ) -> Result<(CreationBase, WorktreeTarget), CoordinatorError> {
        match &params.worktree {
            WorkspaceWorktreeRequest::NewBranch { branch, base } => {
                let base = self.resolve_creation_base(base, repository, git_repository)?;
                let branch_name = branch
                    .clone()
                    .unwrap_or_else(|| format!("coco/{}", params.name));
                Ok((base, WorktreeTarget::NewBranch { branch_name }))
            }
            WorkspaceWorktreeRequest::ExistingBranch { branch } => {
                let reference = format!("refs/heads/{branch}");
                let base_sha = self.git.resolve_local_branch(git_repository, branch)?;
                Ok((
                    CreationBase {
                        requested: WorkspaceBaseRequest::Revision {
                            revision: reference.clone(),
                        },
                        base_ref: reference,
                        base_sha,
                        source_workspace: None,
                    },
                    WorktreeTarget::ExistingBranch {
                        branch_name: branch.clone(),
                    },
                ))
            }
            WorkspaceWorktreeRequest::Detached { base } => Ok((
                self.resolve_creation_base(base, repository, git_repository)?,
                WorktreeTarget::Detached,
            )),
        }
    }

    fn resolve_creation_base(
        &self,
        requested: &WorkspaceBaseRequest,
        repository: &Repository,
        git_repository: &GitRepository,
    ) -> Result<CreationBase, CoordinatorError> {
        match requested {
            WorkspaceBaseRequest::Revision { revision } => Ok(CreationBase {
                requested: requested.clone(),
                base_ref: revision.clone(),
                base_sha: self.git.resolve_commit(git_repository, revision)?,
                source_workspace: None,
            }),
            WorkspaceBaseRequest::Workspace { workspace } => {
                let source = self.resolve_workspace_in_repository(repository, workspace)?;
                let source_worktree = source.worktree_path.as_deref().ok_or(
                    CoordinatorError::IncompleteWorkspace("base workspace worktree"),
                )?;
                let source_repository = self.git.discover(source_worktree)?;
                if source_repository.git_common_dir != git_repository.git_common_dir {
                    return Err(CoordinatorError::InvalidParams(
                        "base workspace must belong to the destination repository".to_owned(),
                    ));
                }
                Ok(CreationBase {
                    requested: requested.clone(),
                    base_ref: "HEAD".to_owned(),
                    base_sha: self.git.resolve_commit(&source_repository, "HEAD")?,
                    source_workspace: Some(BaseWorkspace {
                        requested_reference: workspace.clone(),
                        workspace_id: source.id,
                        workspace_name: source.name,
                    }),
                })
            }
        }
    }

    async fn resolve_creation_context(
        &self,
        params: &WorkspaceCreateParams,
        repository: &Repository,
    ) -> Result<CreationContext, CoordinatorError> {
        let WorkspaceContextRequest::Fork { source, compact } = &params.context else {
            return Ok(CreationContext {
                mode: ContextMode::Fresh,
                fork: None,
            });
        };

        let (native, source) = self.resolve_context_source(source, repository).await?;
        if !matches!(
            native.status,
            crate::domain::CodexThreadStatus::Idle | crate::domain::CodexThreadStatus::NotLoaded
        ) {
            return Err(CoordinatorError::InvalidParams(
                "context source thread must be idle or not loaded".to_owned(),
            ));
        }
        Ok(CreationContext {
            mode: ContextMode::Fork,
            fork: Some(ForkContext {
                source,
                thread_id: native.id,
                compact: *compact,
            }),
        })
    }

    async fn resolve_context_source(
        &self,
        source: &WorkspaceContextSource,
        repository: &Repository,
    ) -> Result<(super::NativeThread, ResolvedContextSource), CoordinatorError> {
        match source {
            WorkspaceContextSource::Reference { reference } => {
                self.resolve_automatic_context_reference(repository, reference)
                    .await
            }
            WorkspaceContextSource::Workspace { workspace } => {
                self.resolve_workspace_context_source(repository, workspace)
                    .await
            }
            WorkspaceContextSource::Thread { thread_id } => {
                self.resolve_thread_context_source(thread_id).await
            }
        }
    }

    async fn resolve_automatic_context_reference(
        &self,
        repository: &Repository,
        reference: &str,
    ) -> Result<(super::NativeThread, ResolvedContextSource), CoordinatorError> {
        if let Some(workspace) = reference.strip_prefix("workspace:") {
            validate_non_empty("context.reference", workspace)?;
            return self
                .resolve_workspace_context_source(repository, workspace)
                .await;
        }
        if let Some(thread_id) = reference.strip_prefix("thread:") {
            validate_non_empty("context.reference", thread_id)?;
            return self.resolve_thread_context_source(thread_id).await;
        }
        match self.resolve_workspace_in_repository(repository, reference) {
            Ok(workspace) => {
                self.resolve_resolved_workspace_context_source(workspace, reference)
                    .await
            }
            Err(error)
                if matches!(
                    &error,
                    CoordinatorError::WorkspaceNotFound { candidates, .. }
                        if !candidates.is_empty()
                ) =>
            {
                Err(error)
            }
            Err(CoordinatorError::WorkspaceNotFound { .. }) => self
                .resolve_thread_context_source(reference)
                .await
                .map_err(|error| match error {
                    CoordinatorError::Worker(source) => {
                        CoordinatorError::ContextReferenceUnresolved {
                            reference: reference.to_owned(),
                            source,
                        }
                    }
                    error => error,
                }),
            Err(error) => Err(error),
        }
    }

    async fn resolve_workspace_context_source(
        &self,
        repository: &Repository,
        reference: &str,
    ) -> Result<(super::NativeThread, ResolvedContextSource), CoordinatorError> {
        let source = self.resolve_workspace_in_repository(repository, reference)?;
        self.resolve_resolved_workspace_context_source(source, reference)
            .await
    }

    async fn resolve_resolved_workspace_context_source(
        &self,
        source: Workspace,
        requested_reference: &str,
    ) -> Result<(super::NativeThread, ResolvedContextSource), CoordinatorError> {
        let source = self.project_current_runtime_turn(source);
        if source.active_turn_id.is_some() {
            return Err(CoordinatorError::InvalidWorkspaceState {
                expected: "an idle or unloaded source workspace",
                actual: source.phase,
            });
        }
        let native = self.read_bound_thread(&source).await?;
        let source = ResolvedContextSource::Workspace {
            requested_reference: requested_reference.to_owned(),
            workspace_id: source.id,
            workspace_name: source.name,
            source_cwd: native.cwd.clone(),
        };
        Ok((native, source))
    }

    async fn resolve_thread_context_source(
        &self,
        thread_id: &str,
    ) -> Result<(super::NativeThread, ResolvedContextSource), CoordinatorError> {
        let native = self.worker.read_thread(thread_id).await?;
        if native.id != thread_id {
            return Err(CoordinatorError::Worker(
                super::WorkerError::ThreadIdMismatch {
                    expected: thread_id.to_owned(),
                    actual: native.id,
                },
            ));
        }
        let source = ResolvedContextSource::Thread {
            source_cwd: native.cwd.clone(),
        };
        Ok((native, source))
    }

    async fn start_context_thread(
        &self,
        workspace_id: &str,
        name: &str,
        context: &CreationContext,
        cwd: &Path,
        config: Value,
        model: Option<&str>,
    ) -> Result<super::StartedThread, super::WorkerError> {
        match &context.fork {
            Some(fork) => {
                self.worker
                    .fork_thread(workspace_id, name, &fork.thread_id, cwd, config, model)
                    .await
            }
            None => {
                self.worker
                    .start_thread(workspace_id, name, cwd, config, model)
                    .await
            }
        }
    }

    async fn validate_materialization_context(
        &self,
        context: &CreationContext,
    ) -> Result<(), CoordinatorError> {
        let Some(fork) = &context.fork else {
            return Ok(());
        };
        let native = match &fork.source {
            ResolvedContextSource::Workspace {
                workspace_id,
                source_cwd,
                ..
            } => {
                let source = self.store.workspace_by_id(workspace_id)?.ok_or_else(|| {
                    CoordinatorError::WorkspaceNotFound {
                        reference: workspace_id.clone(),
                        candidates: Vec::new(),
                    }
                })?;
                let source = self.project_current_runtime_turn(source);
                if source.active_turn_id.is_some() {
                    return Err(CoordinatorError::InvalidWorkspaceState {
                        expected: "an idle or unloaded source workspace",
                        actual: source.phase,
                    });
                }
                if source.codex_thread_id.as_deref() != Some(&fork.thread_id) {
                    return Err(CoordinatorError::InvalidParams(
                        "the context source workspace no longer has its recorded Codex thread"
                            .to_owned(),
                    ));
                }
                let native = self.read_bound_thread(&source).await?;
                if &native.cwd != source_cwd {
                    return Err(CoordinatorError::Worker(super::WorkerError::CwdMismatch {
                        expected: source_cwd.clone(),
                        actual: native.cwd,
                    }));
                }
                native
            }
            ResolvedContextSource::Thread { source_cwd } => {
                let native = self.worker.read_thread(&fork.thread_id).await?;
                if native.id != fork.thread_id {
                    return Err(CoordinatorError::Worker(
                        super::WorkerError::ThreadIdMismatch {
                            expected: fork.thread_id.clone(),
                            actual: native.id,
                        },
                    ));
                }
                if &native.cwd != source_cwd {
                    return Err(CoordinatorError::Worker(super::WorkerError::CwdMismatch {
                        expected: source_cwd.clone(),
                        actual: native.cwd,
                    }));
                }
                native
            }
        };
        if !matches!(
            native.status,
            crate::domain::CodexThreadStatus::Idle | crate::domain::CodexThreadStatus::NotLoaded
        ) {
            return Err(CoordinatorError::InvalidParams(
                "context source thread must be idle or not loaded".to_owned(),
            ));
        }
        Ok(())
    }

    fn create_workspace_worktree(
        &self,
        repository: &GitRepository,
        plan: &WorktreePlan,
        local_state: &crate::git::LocalStateSnapshot,
        workspace_id: &str,
        hook: Option<HookDispatch>,
    ) -> Result<Workspace, CoordinatorError> {
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
        self.git
            .apply_local_state(&binding.path, local_state)
            .map_err(|source| {
                let error = CoordinatorError::Git(source);
                self.mark_workspace_failed(
                    workspace_id,
                    "worktree.local_state",
                    &error,
                    EventSource::Git,
                );
                error
            })?;
        let (workspace, _) = self
            .store
            .transition_workspace_lifecycle_with_event_and_hook(
                workspace_id,
                WorkspaceLifecycle::Provisioning,
                WorkspaceLifecycle::Ready,
                None,
                EventDraft::workspace(
                    EventKind::WorktreeCreated,
                    EventSource::Git,
                    json!({
                        "path": binding.path,
                        "branchName": binding.branch_name,
                        "worktreeMode": binding.mode,
                        "headSha": binding.head_sha,
                        "localState": local_state.manifest(),
                    }),
                ),
                hook,
            )?;
        Ok(workspace)
    }

    fn persist_prepared_workspace(
        &self,
        params: &WorkspaceCreateParams,
        repository: &Repository,
        loaded_profile: &crate::profile::LoadedProfile,
        creation: &ResolvedCreation,
    ) -> Result<Workspace, CoordinatorError> {
        let base = &creation.base;
        let context = &creation.context;
        let plan = &creation.worktree;
        let local_state = &creation.local_state;
        let base_workspace = base.source_workspace.as_ref().map(|source| {
            json!({
                "requestedReference": source.requested_reference,
                "workspaceId": source.workspace_id,
                "workspaceName": source.workspace_name,
            })
        });
        let context_source = context.fork.as_ref().map(|fork| match &fork.source {
            ResolvedContextSource::Workspace {
                requested_reference,
                workspace_id,
                workspace_name,
                source_cwd,
            } => json!({
                "kind": "workspace",
                "requestedReference": requested_reference,
                "workspaceId": workspace_id,
                "workspaceName": workspace_name,
                "threadId": fork.thread_id,
                "cwd": source_cwd,
            }),
            ResolvedContextSource::Thread { source_cwd } => json!({
                "kind": "thread",
                "threadId": fork.thread_id,
                "cwd": source_cwd,
            }),
        });
        let context_descriptor = json!({
            "version": 3,
            "request": {
                "context": params.context,
                "worktree": params.worktree,
                "changes": params.changes,
            },
            "resolved": {
                "base": {
                    "requested": base.requested,
                    "baseRef": base.base_ref,
                    "baseSha": base.base_sha,
                    "sourceWorkspace": base_workspace,
                },
                "context": {
                    "mode": context.mode,
                    "source": context_source,
                    "compact": context.fork.as_ref().is_some_and(|fork| fork.compact),
                },
                "localState": local_state.manifest(),
            },
        });
        let context_mode = context.mode;
        let branch_name = plan.branch_name.clone();
        let worktree_mode = plan.mode;
        let base_sha = plan.base_sha.clone();
        let worktree_path = plan.path.clone();
        let (workspace, _) = self.store.create_workspace_with_event(
            NewWorkspace {
                create_operation_id: Some(params.operation_id.clone()),
                repository_id: repository.id.clone(),
                name: params.name.clone(),
                context_mode,
                context: context_descriptor,
                profile: loaded_profile.snapshot.clone(),
                worktree_mode,
                branch_name,
                base_sha: Some(base_sha),
                worktree_path: Some(worktree_path),
            },
            EventDraft::workspace(
                EventKind::WorkspaceCreated,
                EventSource::Coco,
                json!({
                    "operationId": params.operation_id,
                    "name": params.name,
                    "baseSha": plan.base_sha,
                    "worktreeMode": plan.mode,
                }),
            ),
        )?;
        Ok(workspace)
    }

    pub(crate) async fn list_workspaces(
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
        let phases = params
            .phases
            .map(|phases| {
                phases
                    .iter()
                    .map(|phase| {
                        WorkspacePhase::parse(phase).ok_or_else(|| {
                            CoordinatorError::InvalidParams(format!(
                                "unknown workspace phase {phase:?}"
                            ))
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;
        if phases.is_none() {
            workspaces.retain(|workspace| workspace.availability != WorkspaceAvailability::Closed);
        }
        let mut hydrated = Vec::with_capacity(workspaces.len());
        for workspace in workspaces {
            hydrated.push(self.hydrate_native_thread_runtime(workspace).await);
        }
        if let Some(phases) = phases {
            hydrated.retain(|workspace| phases.contains(&workspace.phase));
        }
        hydrated
            .into_iter()
            .map(|workspace| self.workspace_list_item(workspace))
            .collect()
    }

    pub(crate) async fn get_workspace(
        &self,
        params: WorkspaceGetParams,
    ) -> Result<WorkspaceStatusResult, CoordinatorError> {
        let workspace = self.resolve_workspace(&params.scope, &params.workspace)?;
        let workspace = self.hydrate_native_thread_runtime(workspace).await;
        let (_, git_repository) = self.git_repository_for_workspace(&workspace)?;
        let git = if workspace.availability == WorkspaceAvailability::Closed {
            WorkspaceGitStatus::Incomplete(GitIncomplete {
                observed: false,
                reason: "workspace is closed".to_owned(),
            })
        } else {
            match workspace_git_binding(&workspace) {
                Some((worktree, mode, branch, base)) => {
                    match self
                        .git
                        .observe(&git_repository, worktree, mode, branch, base)
                    {
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
            }
        };
        let events = self.store.events_after(Some(&workspace.id), 0)?;
        let next_sequence = events.last().map_or(0, |event| event.sequence);
        let open_decisions = self.open_decisions_for_workspace(&workspace.id);
        let runtime_resources = match self.worker.workspace_resources(&workspace.id).await {
            Ok(resources) => resources,
            Err(source) => {
                warn!(workspace_id = %workspace.id, %source, "workspace resources are unavailable");
                None
            }
        };
        Ok(WorkspaceStatusResult {
            workspace,
            git,
            runtime_resources,
            open_decisions,
            next_sequence,
        })
    }

    pub(crate) async fn list_events(
        &self,
        params: EventListParams,
    ) -> Result<EventListResult, CoordinatorError> {
        let workspace = self.resolve_workspace(&params.scope, &params.workspace)?;
        let workspace = self.hydrate_native_thread_runtime(workspace).await;
        let events = self
            .store
            .events_after(Some(&workspace.id), params.after_sequence)?;
        let next_sequence = events
            .last()
            .map_or(params.after_sequence, |event| event.sequence);
        let open_decisions = self.open_decisions_for_workspace(&workspace.id);
        Ok(EventListResult {
            workspace,
            events,
            open_decisions,
            next_sequence,
        })
    }

    pub(super) async fn hydrate_native_thread_runtime(
        &self,
        mut workspace: Workspace,
    ) -> Workspace {
        if workspace.lifecycle != WorkspaceLifecycle::Ready
            || workspace.availability != WorkspaceAvailability::Open
        {
            return workspace;
        }

        if workspace.codex_thread_id.is_none() {
            clear_native_thread_projection(&mut workspace);
            return self.project_current_runtime_turn(workspace);
        }

        let thread_id = workspace.codex_thread_id.clone();
        let native = match self.read_bound_thread(&workspace).await {
            Ok(native) => native,
            Err(source) => {
                warn!(
                    workspace_id = %workspace.id,
                    thread_id = thread_id.as_deref().unwrap_or("(missing)"),
                    %source,
                    "Codex thread state is unavailable"
                );
                clear_native_thread_projection(&mut workspace);
                return self.project_current_runtime_turn(workspace);
            }
        };
        self.project_native_thread_runtime(workspace, native.status)
    }

    pub(super) fn project_native_thread_runtime(
        &self,
        mut workspace: Workspace,
        status: crate::domain::CodexThreadStatus,
    ) -> Workspace {
        let status = status.canonicalized();
        let runtime = self.runtime_turn_for_workspace(&workspace);
        let has_accepted_turn = runtime.is_some() || workspace.active_turn_id.is_some();
        if let Some(runtime) = &runtime {
            workspace.active_turn_id = Some(runtime.id.clone());
        } else if !matches!(
            &status,
            crate::domain::CodexThreadStatus::Active { .. }
                | crate::domain::CodexThreadStatus::Idle
        ) {
            // No current-generation operation can remain correlated to an
            // unloaded or errored native thread.
            workspace.active_turn_id = None;
        }
        workspace.thread_runtime = Some(ThreadRuntimeSnapshot {
            status,
            runtime_generation: self.runtime_generation.clone(),
            observed_at_ms: Utc::now().timestamp_millis(),
            is_fresh: true,
        });
        // A locally accepted current-generation turn closes the short race in
        // which thread/read still says idle before native turn/status events
        // arrive. The same guard is used by send and fork mutations.
        (workspace.phase, workspace.wait_reasons) = derive_workspace_runtime(
            workspace.lifecycle,
            workspace.availability,
            workspace.thread_runtime.as_ref(),
            has_accepted_turn,
            workspace.codex_thread_id.is_some(),
        );
        if runtime.is_some_and(|runtime| runtime.uncertain) {
            workspace.phase = WorkspacePhase::Unavailable;
            workspace.wait_reasons.clear();
        }
        workspace
    }

    pub(crate) fn workspace_diff(
        &self,
        params: WorkspaceDiffParams,
    ) -> Result<WorkspaceDiffResult, CoordinatorError> {
        let workspace = self.resolve_workspace(&params.scope, &params.workspace)?;
        if workspace.availability != WorkspaceAvailability::Open {
            return Err(CoordinatorError::InvalidWorkspaceState {
                expected: "open",
                actual: workspace.phase,
            });
        }
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

fn thread_started_event(
    thread_id: &str,
    parent_thread_id: Option<&str>,
    context_mode: ContextMode,
    compact: bool,
) -> EventDraft {
    EventDraft::workspace(
        EventKind::AgentStarted,
        EventSource::Codex,
        json!({
            "threadId": thread_id,
            "parentThreadId": parent_thread_id,
            "contextMode": context_mode,
            "compactRequested": compact,
        }),
    )
}

fn clear_native_thread_projection(workspace: &mut Workspace) {
    workspace.thread_runtime = None;
    (workspace.phase, workspace.wait_reasons) = derive_workspace_runtime(
        workspace.lifecycle,
        workspace.availability,
        None,
        false,
        workspace.codex_thread_id.is_some(),
    );
}

pub(super) fn pending_context_dependency(
    workspace: &Workspace,
) -> Result<Option<PendingContextDependency>, CoordinatorError> {
    if workspace.codex_thread_id.is_some() || workspace.context_mode != ContextMode::Fork {
        return Ok(None);
    }
    let context = stored_creation_context(workspace)?;
    Ok(context.fork.map(|fork| PendingContextDependency {
        thread_id: fork.thread_id,
        workspace_id: match fork.source {
            ResolvedContextSource::Workspace { workspace_id, .. } => Some(workspace_id),
            ResolvedContextSource::Thread { .. } => None,
        },
    }))
}

fn stored_creation_context(workspace: &Workspace) -> Result<CreationContext, CoordinatorError> {
    match workspace.context_mode {
        ContextMode::Fresh => Ok(CreationContext {
            mode: ContextMode::Fresh,
            fork: None,
        }),
        ContextMode::Fork => {
            let source = workspace
                .context
                .pointer("/resolved/context/source")
                .and_then(Value::as_object)
                .ok_or_else(|| invalid_stored_context("missing resolved context source"))?;
            let string = |field: &str| {
                source
                    .get(field)
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                    .ok_or_else(|| {
                        invalid_stored_context(&format!("missing context source {field}"))
                    })
            };
            let thread_id = string("threadId")?;
            let source_cwd = std::path::PathBuf::from(string("cwd")?);
            let source = match string("kind")?.as_str() {
                "workspace" => ResolvedContextSource::Workspace {
                    requested_reference: string("requestedReference")?,
                    workspace_id: string("workspaceId")?,
                    workspace_name: string("workspaceName")?,
                    source_cwd,
                },
                "thread" => ResolvedContextSource::Thread { source_cwd },
                kind => {
                    return Err(invalid_stored_context(&format!(
                        "unknown context source kind {kind:?}"
                    )));
                }
            };
            let compact = workspace
                .context
                .pointer("/resolved/context/compact")
                .and_then(Value::as_bool)
                .ok_or_else(|| invalid_stored_context("missing context compact flag"))?;
            Ok(CreationContext {
                mode: ContextMode::Fork,
                fork: Some(ForkContext {
                    source,
                    thread_id,
                    compact,
                }),
            })
        }
        ContextMode::Handoff => Err(invalid_stored_context(
            "handoff materialization is not implemented",
        )),
    }
}

fn invalid_stored_context(message: &str) -> CoordinatorError {
    CoordinatorError::InvalidParams(format!(
        "workspace contains invalid stored context: {message}"
    ))
}

fn ensure_create_replay_matches(
    existing: &Workspace,
    params: &WorkspaceCreateParams,
    repository_id: &str,
) -> Result<(), CoordinatorError> {
    let request = json!({
        "context": params.context,
        "worktree": params.worktree,
        "changes": params.changes,
    });
    let matches = existing.repository_id == repository_id
        && existing.name == params.name
        && existing.context_mode == params.context.mode()
        && existing.context.get("request") == Some(&request)
        && existing.profile.name == params.profile
        && existing.profile.model_override == params.model;
    if matches {
        Ok(())
    } else {
        Err(CoordinatorError::IdempotencyConflict)
    }
}

fn validate_create_request(params: &WorkspaceCreateParams) -> Result<(), CoordinatorError> {
    match &params.worktree {
        WorkspaceWorktreeRequest::NewBranch { branch, base } => {
            if let Some(branch) = branch {
                validate_non_empty("worktree.branch", branch)?;
            }
            validate_base_request(base)?;
        }
        WorkspaceWorktreeRequest::ExistingBranch { branch } => {
            validate_non_empty("worktree.branch", branch)?;
        }
        WorkspaceWorktreeRequest::Detached { base } => validate_base_request(base)?,
    }
    match &params.context {
        WorkspaceContextRequest::Fresh => Ok(()),
        WorkspaceContextRequest::Fork { source, .. } => match source {
            WorkspaceContextSource::Reference { reference } => {
                validate_non_empty("context.source.reference", reference)
            }
            WorkspaceContextSource::Workspace { workspace } => {
                validate_non_empty("context.source.workspace", workspace)
            }
            WorkspaceContextSource::Thread { thread_id } => {
                validate_non_empty("context.source.threadId", thread_id)
            }
        },
    }
}

fn validate_base_request(base: &WorkspaceBaseRequest) -> Result<(), CoordinatorError> {
    match base {
        WorkspaceBaseRequest::Revision { revision } => {
            validate_non_empty("worktree.base.revision", revision)
        }
        WorkspaceBaseRequest::Workspace { workspace } => {
            validate_non_empty("worktree.base.workspace", workspace)
        }
    }
}

fn workspace_git_binding(
    workspace: &Workspace,
) -> Option<(&Path, WorktreeMode, Option<&str>, &str)> {
    let branch = workspace.branch_name.as_deref();
    let binding_is_valid = match workspace.worktree_mode {
        WorktreeMode::NewBranch | WorktreeMode::ExistingBranch => branch.is_some(),
        WorktreeMode::Detached => branch.is_none(),
    };
    binding_is_valid.then_some((
        workspace.worktree_path.as_deref()?,
        workspace.worktree_mode,
        branch,
        workspace.base_sha.as_deref()?,
    ))
}
