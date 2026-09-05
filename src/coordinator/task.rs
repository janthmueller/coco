use std::path::Path;

use chrono::Utc;
use serde_json::{Value, json};
use tracing::warn;

use super::{Coordinator, CoordinatorError, validate_non_empty, validate_operation_id};
use crate::domain::{Audit, ContextMode, EventKind, EventSource, Repository, Task, TaskPhase};
use crate::profile::{load_profile, with_effective_thread_settings};
use crate::protocol::{
    AuditRecordParams, EventListParams, EventListResult, GitIncomplete, GitObservationError,
    GitUnavailable, RepositoryRegisterParams, TaskCreateParams, TaskDiffParams, TaskDiffResult,
    TaskGetParams, TaskGitStatus, TaskListParams, TaskResult, TaskStatusResult,
};
use crate::store::{AuditDraft, EventDraft, NewTask};

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

    pub(crate) async fn create_task(
        &self,
        params: TaskCreateParams,
    ) -> Result<TaskResult, CoordinatorError> {
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
            .task_by_create_operation_id(&params.operation_id)?
        {
            ensure_create_replay_matches(&existing, &params, &repository.id)?;
            return self.task_response(existing);
        }

        let loaded_profile = load_profile(&params.profile, &self.codex_home)?;
        let base_sha = self.git.resolve_commit(&git_repository, &params.base_ref)?;
        self.git.assert_clean(&git_repository)?;
        if self
            .store
            .task_by_name(&repository.id, &params.name)?
            .is_some()
        {
            return Err(CoordinatorError::TaskExists(params.name));
        }
        let plan = self.git.plan_worktree(
            &git_repository,
            &self.worktrees_dir,
            &params.name,
            &base_sha,
        )?;

        let (task, _) = self.store.create_task_with_event(
            NewTask {
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
            EventDraft::task(
                EventKind::TaskCreated,
                EventSource::Coco,
                json!({
                    "operationId": params.operation_id,
                    "name": params.name,
                    "baseSha": plan.base_sha,
                }),
            ),
        )?;

        let binding = match self.git.create_worktree(&git_repository, &plan) {
            Ok(binding) => binding,
            Err(source) => {
                let error = CoordinatorError::Git(source);
                self.mark_task_failed(&task.id, "worktree.create", &error, EventSource::Git);
                return Err(error);
            }
        };
        self.store.transition_task_with_event(
            &task.id,
            TaskPhase::Provisioning,
            TaskPhase::Starting,
            None,
            EventDraft::task(
                EventKind::WorktreeCreated,
                EventSource::Git,
                json!({
                    "path": binding.path,
                    "branchName": binding.branch_name,
                    "headSha": binding.head_sha,
                }),
            ),
        )?;

        let started_thread = match self
            .worker
            .start_thread(&binding.path, loaded_profile.thread_config)
            .await
        {
            Ok(thread) => thread,
            Err(source) => {
                let error = CoordinatorError::Worker(source);
                self.mark_task_failed(&task.id, "thread.start", &error, EventSource::Codex);
                return Err(error);
            }
        };
        let effective_profile =
            with_effective_thread_settings(loaded_profile.snapshot, &started_thread.response);
        self.store
            .update_task_profile(&task.id, &effective_profile)?;
        let (task, _) = self.store.bind_thread_with_event(
            &task.id,
            TaskPhase::Starting,
            TaskPhase::Idle,
            &started_thread.id,
            None,
            EventDraft::task(
                EventKind::AgentStarted,
                EventSource::Codex,
                json!({"threadId": started_thread.id}),
            ),
        )?;
        self.task_response(task)
    }

    pub(crate) fn list_tasks(&self, params: TaskListParams) -> Result<Vec<Task>, CoordinatorError> {
        let (repository, _) = self.registered_repository_for_path(&params.repository_path)?;
        let mut tasks = self.store.list_tasks(Some(&repository.id))?;
        if let Some(phases) = params.phases {
            let phases = phases
                .iter()
                .map(|phase| {
                    TaskPhase::parse(phase).ok_or_else(|| {
                        CoordinatorError::InvalidParams(format!("unknown task phase {phase:?}"))
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            tasks.retain(|task| phases.contains(&task.phase));
        }
        Ok(tasks)
    }

    pub(crate) fn get_task(
        &self,
        params: TaskGetParams,
    ) -> Result<TaskStatusResult, CoordinatorError> {
        let (repository, git_repository) =
            self.registered_repository_for_path(&params.repository_path)?;
        let task = self.resolve_task(&repository, &params.task)?;
        let git = match task_git_binding(&task) {
            Some((worktree, branch, base)) => {
                match self.git.observe(&git_repository, worktree, branch, base) {
                    Ok(observation) => TaskGitStatus::Observed(observation),
                    Err(source) => {
                        warn!(task_id = %task.id, %source, "could not refresh task Git state");
                        TaskGitStatus::Unavailable(GitUnavailable {
                            observed: false,
                            error: GitObservationError {
                                code: "GIT_OBSERVATION_FAILED".to_owned(),
                                message: "Git state could not be refreshed".to_owned(),
                            },
                        })
                    }
                }
            }
            None => TaskGitStatus::Incomplete(GitIncomplete {
                observed: false,
                reason: "task has no complete Git binding".to_owned(),
            }),
        };
        let events = self.store.events_after(Some(&task.id), 0)?;
        let next_sequence = events.last().map_or(0, |event| event.sequence);
        Ok(TaskStatusResult {
            task,
            git,
            next_sequence,
        })
    }

    pub(crate) fn list_events(
        &self,
        params: EventListParams,
    ) -> Result<EventListResult, CoordinatorError> {
        let (repository, _) = self.registered_repository_for_path(&params.repository_path)?;
        let task = self.resolve_task(&repository, &params.task)?;
        let events = self
            .store
            .events_after(Some(&task.id), params.after_sequence)?;
        let next_sequence = events
            .last()
            .map_or(params.after_sequence, |event| event.sequence);
        Ok(EventListResult {
            task,
            events,
            next_sequence,
        })
    }

    pub(crate) fn task_diff(
        &self,
        params: TaskDiffParams,
    ) -> Result<TaskDiffResult, CoordinatorError> {
        let (repository, _) = self.registered_repository_for_path(&params.repository_path)?;
        let task = self.resolve_task(&repository, &params.task)?;
        let worktree = task
            .worktree_path
            .as_deref()
            .ok_or(CoordinatorError::IncompleteTask("worktree"))?;
        let base_sha = task
            .base_sha
            .as_deref()
            .ok_or(CoordinatorError::IncompleteTask("base SHA"))?;
        let diff = self.git.diff(worktree, base_sha)?;
        let requested = params
            .max_bytes
            .map(|value| usize::try_from(value).unwrap_or(usize::MAX))
            .unwrap_or(DEFAULT_DIFF_BYTES)
            .min(MAX_DIFF_BYTES);
        let retained = diff.tracked_patch.len().min(requested);
        let patch = String::from_utf8_lossy(&diff.tracked_patch[..retained]);
        Ok(TaskDiffResult {
            patch: patch.into_owned(),
            patch_truncated: diff.tracked_patch_truncated || retained < diff.tracked_patch.len(),
            untracked_paths: diff.untracked_paths,
        })
    }

    pub(crate) fn record_audit(
        &self,
        params: AuditRecordParams,
    ) -> Result<Audit, CoordinatorError> {
        let task_id = params.task_id.as_deref().and_then(|candidate| {
            if let Ok(Some(task)) = self.store.task_by_id(candidate) {
                return Some(task.id);
            }
            let repository_path = params.details.get("repositoryPath")?.as_str()?;
            let discovered = self.git.discover(repository_path).ok()?;
            let repository = self
                .store
                .repository_by_common_dir(&discovered.git_common_dir)
                .ok()??;
            let task = self.store.task_by_name(&repository.id, candidate).ok()??;
            Some(task.id)
        });
        let audit = self.store.append_audit(AuditDraft {
            source: params.source,
            action: params.action,
            task_id,
            operation_id: params.operation_id,
            outcome: params.outcome,
            details: params.details,
            occurred_at_ms: None,
        })?;
        Ok(audit)
    }
}

fn ensure_create_replay_matches(
    existing: &Task,
    params: &TaskCreateParams,
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

fn task_git_binding(task: &Task) -> Option<(&Path, &str, &str)> {
    Some((
        task.worktree_path.as_deref()?,
        task.branch_name.as_deref()?,
        task.base_sha.as_deref()?,
    ))
}
