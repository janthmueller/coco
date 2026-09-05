use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::Mutex as AsyncMutex;
use tracing::{debug, error, warn};

use crate::codex::{CodexClient, CodexError, CodexEvent};
use crate::domain::{
    AuditOutcome, ContextMode, EventKind, EventSource, Repository, Task, TaskPhase, Turn, TurnPhase,
};
use crate::git::{Git, GitError, GitRepository};
use crate::profile::{ProfileError, load_profile, with_effective_thread_settings};
use crate::rpc::{RpcErrorPayload, RpcHandler};
use crate::store::{AuditDraft, EventDraft, NewTask, NewTurn, Store, StoreError, TurnCompletion};

const DEFAULT_DIFF_BYTES: usize = 4 * 1024 * 1024;
const MAX_DIFF_BYTES: usize = 16 * 1024 * 1024;
const MAX_OPERATION_ID_BYTES: usize = 256;

#[derive(Debug, Clone, PartialEq)]
pub struct StartedThread {
    pub id: String,
    pub response: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartedTurn {
    pub id: String,
}

#[derive(Debug, Error)]
pub enum WorkerError {
    #[error(transparent)]
    Codex(#[from] CodexError),
    #[error("Codex response is missing required field {0}")]
    InvalidResponse(&'static str),
    #[error("worker runtime is unavailable: {0}")]
    Unavailable(String),
    #[error("Codex thread cwd mismatch: expected {expected}, received {actual}")]
    CwdMismatch { expected: PathBuf, actual: PathBuf },
}

#[async_trait]
pub trait WorkerRuntime: Send + Sync + 'static {
    async fn start_thread(&self, cwd: &Path, config: Value) -> Result<StartedThread, WorkerError>;

    async fn start_turn(
        &self,
        thread_id: &str,
        cwd: &Path,
        client_message_id: &str,
        message: &str,
    ) -> Result<StartedTurn, WorkerError>;
}

#[derive(Debug, Clone)]
pub struct CodexWorker {
    client: CodexClient,
}

impl CodexWorker {
    pub fn new(client: CodexClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl WorkerRuntime for CodexWorker {
    async fn start_thread(&self, cwd: &Path, config: Value) -> Result<StartedThread, WorkerError> {
        let response = self
            .client
            .request(
                "thread/start",
                json!({
                    "cwd": cwd,
                    "runtimeWorkspaceRoots": [cwd],
                    "config": config,
                    "ephemeral": false,
                }),
            )
            .await?;
        let id = response
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .ok_or(WorkerError::InvalidResponse("thread.id"))?
            .to_owned();
        let returned_cwd = response
            .get("cwd")
            .and_then(Value::as_str)
            .ok_or(WorkerError::InvalidResponse("cwd"))
            .map(PathBuf::from)?;
        if returned_cwd != cwd {
            return Err(WorkerError::CwdMismatch {
                expected: cwd.to_owned(),
                actual: returned_cwd,
            });
        }
        Ok(StartedThread { id, response })
    }

    async fn start_turn(
        &self,
        thread_id: &str,
        cwd: &Path,
        client_message_id: &str,
        message: &str,
    ) -> Result<StartedTurn, WorkerError> {
        let response = self
            .client
            .request(
                "turn/start",
                json!({
                    "threadId": thread_id,
                    "cwd": cwd,
                    "clientUserMessageId": client_message_id,
                    "input": [{"type": "text", "text": message}],
                }),
            )
            .await?;
        let id = response
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .ok_or(WorkerError::InvalidResponse("turn.id"))?
            .to_owned();
        Ok(StartedTurn { id })
    }
}

pub struct Coordinator {
    store: Arc<Store>,
    git: Git,
    worker: Arc<dyn WorkerRuntime>,
    worktrees_dir: PathBuf,
    codex_home: PathBuf,
    repository_locks: AsyncMutex<HashMap<String, Arc<AsyncMutex<()>>>>,
}

impl Coordinator {
    pub fn new(
        store: Arc<Store>,
        git: Git,
        worker: Arc<dyn WorkerRuntime>,
        worktrees_dir: PathBuf,
        codex_home: PathBuf,
    ) -> Self {
        Self {
            store,
            git,
            worker,
            worktrees_dir,
            codex_home,
            repository_locks: AsyncMutex::new(HashMap::new()),
        }
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    async fn dispatch(&self, method: &str, params: Value) -> Result<Value, CoordinatorError> {
        match method {
            "health" => Ok(json!({"status": "ok"})),
            "repository.register" => self.register_repository(parse_params(params)?),
            "task.create" => self.create_task(parse_params(params)?).await,
            "task.list" => self.list_tasks(parse_params(params)?),
            "task.get" => self.get_task(parse_params(params)?),
            "turn.start" => self.start_turn(parse_params(params)?).await,
            "event.list" => self.list_events(parse_params(params)?),
            "task.diff" => self.task_diff(parse_params(params)?),
            "audit.record" => self.record_audit(parse_params(params)?),
            _ => Err(CoordinatorError::MethodNotFound(method.to_owned())),
        }
    }

    fn register_repository(
        &self,
        params: RepositoryRegisterParams,
    ) -> Result<Value, CoordinatorError> {
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
        json_value(repository)
    }

    async fn create_task(&self, params: TaskCreateParams) -> Result<Value, CoordinatorError> {
        validate_non_empty("goal", &params.goal)?;
        validate_non_empty("baseRef", &params.base_ref)?;
        validate_operation_id(&params.operation_id)?;
        let context_mode = ContextMode::parse(&params.context_mode).ok_or_else(|| {
            CoordinatorError::InvalidParams("contextMode must be `fresh` in v0".to_owned())
        })?;
        if context_mode != ContextMode::Fresh {
            return Err(CoordinatorError::UnsupportedContext(params.context_mode));
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
                goal: params.goal.trim().to_owned(),
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
        self.store.bind_thread_with_event(
            &task.id,
            TaskPhase::Starting,
            &started_thread.id,
            None,
            EventDraft::task(
                EventKind::AgentStarted,
                EventSource::Codex,
                json!({"threadId": started_thread.id}),
            ),
        )?;

        let initial_operation_id = format!("{}:initial", params.operation_id);
        let client_message_id = message_fingerprint(&initial_operation_id, &task.id, &params.goal);
        self.store.append_event(EventDraft {
            task_id: Some(task.id.clone()),
            turn_id: None,
            kind: EventKind::MessageReceived,
            source: EventSource::Coco,
            source_method: Some("task.create".to_owned()),
            occurred_at_ms: None,
            payload: json!({
                "clientMessageId": client_message_id,
                "text": params.goal,
            }),
        })?;
        let prompt = initial_prompt(&task, &repository, &binding.path);
        let started_turn = match self
            .worker
            .start_turn(
                &started_thread.id,
                &binding.path,
                &client_message_id,
                &prompt,
            )
            .await
        {
            Ok(turn) => turn,
            Err(source) => {
                let error = CoordinatorError::Worker(source);
                self.mark_task_failed(&task.id, "turn.start", &error, EventSource::Codex);
                return Err(error);
            }
        };
        let (task, turn, _) = self.store.start_turn_with_event(
            &task.id,
            &[TaskPhase::Starting],
            NewTurn {
                operation_id: Some(initial_operation_id),
                client_message_id,
                codex_turn_id: Some(started_turn.id.clone()),
                started_at_ms: None,
            },
            EventDraft::task(
                EventKind::TurnStarted,
                EventSource::Codex,
                json!({"codexTurnId": started_turn.id}),
            ),
        )?;
        task_and_turn_response(task, &turn)
    }

    fn list_tasks(&self, params: TaskListParams) -> Result<Value, CoordinatorError> {
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
        json_value(tasks)
    }

    fn get_task(&self, params: TaskReferenceParams) -> Result<Value, CoordinatorError> {
        let (repository, git_repository) =
            self.registered_repository_for_path(&params.repository_path)?;
        let task = self.resolve_task(&repository, &params.task)?;
        let git = match task_git_binding(&task) {
            Some((worktree, branch, base)) => {
                match self.git.observe(&git_repository, worktree, branch, base) {
                    Ok(observation) => json_value(observation)?,
                    Err(source) => {
                        warn!(task_id = %task.id, %source, "could not refresh task Git state");
                        json!({
                            "observed": false,
                            "error": {
                                "code": "GIT_OBSERVATION_FAILED",
                                "message": "Git state could not be refreshed",
                            },
                        })
                    }
                }
            }
            None => json!({"observed": false, "reason": "task has no complete Git binding"}),
        };
        let events = self.store.events_after(Some(&task.id), 0)?;
        let next_sequence = events.last().map_or(0, |event| event.sequence);
        Ok(json!({
            "task": task,
            "git": git,
            "nextSequence": next_sequence,
        }))
    }

    async fn start_turn(&self, params: TurnStartParams) -> Result<Value, CoordinatorError> {
        validate_non_empty("message", &params.message)?;
        validate_operation_id(&params.operation_id)?;
        let (repository, _) = self.registered_repository_for_path(&params.repository_path)?;
        let repository_lock = self.repository_lock(&repository.id).await;
        let _guard = repository_lock.lock().await;
        let task = self.resolve_task(&repository, &params.task)?;
        let client_message_id =
            message_fingerprint(&params.operation_id, &task.id, &params.message);
        if let Some(existing) = self.store.turn_by_operation_id(&params.operation_id)? {
            if existing.task_id != task.id || existing.client_message_id != client_message_id {
                return Err(CoordinatorError::IdempotencyConflict);
            }
            return task_and_turn_response(task, &existing);
        }
        if task.phase != TaskPhase::Idle {
            return Err(CoordinatorError::InvalidTaskState {
                expected: "idle",
                actual: task.phase,
            });
        }
        let thread_id = task
            .codex_thread_id
            .as_deref()
            .ok_or(CoordinatorError::IncompleteTask("Codex thread"))?;
        let worktree = task
            .worktree_path
            .as_deref()
            .ok_or(CoordinatorError::IncompleteTask("worktree"))?;

        self.store.append_event(EventDraft {
            task_id: Some(task.id.clone()),
            turn_id: None,
            kind: EventKind::MessageReceived,
            source: EventSource::Coco,
            source_method: Some("turn.start".to_owned()),
            occurred_at_ms: None,
            payload: json!({
                "clientMessageId": client_message_id,
                "text": params.message,
            }),
        })?;
        let started = self
            .worker
            .start_turn(thread_id, worktree, &client_message_id, &params.message)
            .await?;
        let (task, turn, _) = self.store.start_turn_with_event(
            &task.id,
            &[TaskPhase::Idle],
            NewTurn {
                operation_id: Some(params.operation_id),
                client_message_id,
                codex_turn_id: Some(started.id.clone()),
                started_at_ms: None,
            },
            EventDraft::task(
                EventKind::TurnStarted,
                EventSource::Codex,
                json!({"codexTurnId": started.id}),
            ),
        )?;
        task_and_turn_response(task, &turn)
    }

    fn list_events(&self, params: EventListParams) -> Result<Value, CoordinatorError> {
        let (repository, _) = self.registered_repository_for_path(&params.repository_path)?;
        let task = self.resolve_task(&repository, &params.task)?;
        let events = self
            .store
            .events_after(Some(&task.id), params.after_sequence)?;
        let next_sequence = events
            .last()
            .map_or(params.after_sequence, |event| event.sequence);
        Ok(json!({
            "task": task,
            "events": events,
            "nextSequence": next_sequence,
        }))
    }

    fn task_diff(&self, params: TaskDiffParams) -> Result<Value, CoordinatorError> {
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
        Ok(json!({
            "patch": patch,
            "patchTruncated": diff.tracked_patch_truncated || retained < diff.tracked_patch.len(),
            "untrackedPaths": diff.untracked_paths,
        }))
    }

    fn record_audit(&self, params: AuditRecordParams) -> Result<Value, CoordinatorError> {
        let outcome = AuditOutcome::parse(&params.outcome).ok_or_else(|| {
            CoordinatorError::InvalidParams("outcome must be `succeeded` or `failed`".to_owned())
        })?;
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
            outcome,
            details: params.details,
            occurred_at_ms: None,
        })?;
        json_value(audit)
    }

    pub fn record_codex_event(&self, event: CodexEvent) -> Result<(), StoreError> {
        match event {
            CodexEvent::Notification { method, params } => {
                self.record_codex_notification(&method, params)
            }
            CodexEvent::ServerRequest { id, method, params } => {
                self.record_codex_server_request(id, &method, params)
            }
        }
    }

    fn record_codex_notification(&self, method: &str, params: Value) -> Result<(), StoreError> {
        let Some(task) = self.task_for_codex_params(&params)? else {
            debug!(method, "ignoring uncorrelated Codex notification");
            return Ok(());
        };
        let turn = self.turn_for_codex_params(&task, &params)?;
        match method {
            "turn/completed" => {
                let Some(turn) = turn else {
                    warn!(task_id = %task.id, "ignoring turn completion without a correlated turn");
                    return Ok(());
                };
                if matches!(
                    turn.phase,
                    TurnPhase::Completed | TurnPhase::Failed | TurnPhase::Interrupted
                ) {
                    return Ok(());
                }
                let status = params
                    .pointer("/turn/status")
                    .and_then(Value::as_str)
                    .unwrap_or("failed");
                let phase = match status {
                    "completed" => TurnPhase::Completed,
                    "interrupted" => TurnPhase::Interrupted,
                    "failed" => TurnPhase::Failed,
                    _ => {
                        warn!(status, "ignoring non-terminal turn/completed payload");
                        return Ok(());
                    }
                };
                let error = params
                    .pointer("/turn/error")
                    .filter(|value| !value.is_null())
                    .cloned();
                self.store.complete_turn_with_event(
                    &task.id,
                    &turn.id,
                    TurnCompletion {
                        phase,
                        error,
                        completed_at_ms: None,
                    },
                    EventDraft::task(
                        EventKind::TurnCompleted,
                        EventSource::Codex,
                        json!({"status": status}),
                    ),
                )?;
            }
            "item/completed"
                if params.pointer("/item/type").and_then(Value::as_str) == Some("agentMessage") =>
            {
                self.store.append_event(EventDraft {
                    task_id: Some(task.id),
                    turn_id: turn.map(|turn| turn.id),
                    kind: EventKind::AgentMessageCompleted,
                    source: EventSource::Codex,
                    source_method: Some(method.to_owned()),
                    occurred_at_ms: params.get("completedAtMs").and_then(Value::as_i64),
                    payload: json!({
                        "itemId": params.pointer("/item/id"),
                        "text": params.pointer("/item/text"),
                    }),
                })?;
            }
            "turn/plan/updated" => {
                self.store.append_event(codex_event_draft(
                    &task,
                    turn.as_ref(),
                    EventKind::PlanUpdated,
                    method,
                    params,
                ))?;
            }
            "turn/diff/updated" => {
                self.store.append_event(codex_event_draft(
                    &task,
                    turn.as_ref(),
                    EventKind::DiffUpdated,
                    method,
                    params,
                ))?;
            }
            "error"
                if !params
                    .get("willRetry")
                    .and_then(Value::as_bool)
                    .unwrap_or(false) =>
            {
                self.store.append_event(codex_event_draft(
                    &task,
                    turn.as_ref(),
                    EventKind::AgentFailed,
                    method,
                    params,
                ))?;
            }
            _ => {}
        }
        Ok(())
    }

    fn record_codex_server_request(
        &self,
        id: Value,
        method: &str,
        params: Value,
    ) -> Result<(), StoreError> {
        let Some(task) = self.task_for_codex_params(&params)? else {
            warn!(method, "ignoring uncorrelated Codex server request");
            return Ok(());
        };
        let turn = self.turn_for_codex_params(&task, &params)?;
        let waiting_phase = if method.contains("requestUserInput") {
            TaskPhase::WaitingForInput
        } else {
            TaskPhase::WaitingForApproval
        };
        let payload = json!({
            "requestId": id,
            "method": method,
            "reason": params.get("reason"),
        });
        let event = EventDraft {
            task_id: Some(task.id.clone()),
            turn_id: turn.map(|turn| turn.id),
            kind: EventKind::ApprovalRequested,
            source: EventSource::Codex,
            source_method: Some(method.to_owned()),
            occurred_at_ms: None,
            payload,
        };
        if task.phase == TaskPhase::Active {
            self.store.transition_task_with_event(
                &task.id,
                TaskPhase::Active,
                waiting_phase,
                None,
                event,
            )?;
        } else {
            self.store.append_event(event)?;
        }
        Ok(())
    }

    fn task_for_codex_params(&self, params: &Value) -> Result<Option<Task>, StoreError> {
        let thread_id = params
            .get("threadId")
            .or_else(|| params.pointer("/thread/id"))
            .and_then(Value::as_str);
        match thread_id {
            Some(thread_id) => self.store.task_by_thread_id(thread_id),
            None => Ok(None),
        }
    }

    fn turn_for_codex_params(
        &self,
        task: &Task,
        params: &Value,
    ) -> Result<Option<Turn>, StoreError> {
        let codex_turn_id = params
            .get("turnId")
            .or_else(|| params.pointer("/turn/id"))
            .and_then(Value::as_str);
        if let Some(codex_turn_id) = codex_turn_id
            && let Some(turn) = self.store.turn_by_codex_id(codex_turn_id)?
        {
            return Ok((turn.task_id == task.id).then_some(turn));
        }
        task.active_turn_id
            .as_deref()
            .map(|turn_id| self.store.turn_by_id(turn_id))
            .transpose()
            .map(Option::flatten)
    }

    fn registered_repository_for_path(
        &self,
        path: &Path,
    ) -> Result<(Repository, GitRepository), CoordinatorError> {
        let discovered = self.git.discover(path)?;
        let repository = self
            .store
            .repository_by_common_dir(&discovered.git_common_dir)?
            .ok_or_else(|| {
                CoordinatorError::RepositoryNotRegistered(discovered.root_path.clone())
            })?;
        Ok((repository, discovered))
    }

    fn resolve_task(
        &self,
        repository: &Repository,
        reference: &str,
    ) -> Result<Task, CoordinatorError> {
        if let Some(task) = self.store.task_by_id(reference)? {
            return if task.repository_id == repository.id {
                Ok(task)
            } else {
                Err(CoordinatorError::TaskNotFound(reference.to_owned()))
            };
        }
        self.store
            .task_by_name(&repository.id, reference)?
            .ok_or_else(|| CoordinatorError::TaskNotFound(reference.to_owned()))
    }

    async fn repository_lock(&self, repository_id: &str) -> Arc<AsyncMutex<()>> {
        let mut locks = self.repository_locks.lock().await;
        Arc::clone(
            locks
                .entry(repository_id.to_owned())
                .or_insert_with(|| Arc::new(AsyncMutex::new(()))),
        )
    }

    fn task_response(&self, task: Task) -> Result<Value, CoordinatorError> {
        let turn = task
            .active_turn_id
            .as_deref()
            .map(|turn_id| self.store.turn_by_id(turn_id))
            .transpose()?
            .flatten();
        match turn {
            Some(turn) => task_and_turn_response(task, &turn),
            None => Ok(json!({"task": task})),
        }
    }

    fn mark_task_failed(
        &self,
        task_id: &str,
        stage: &'static str,
        source_error: &CoordinatorError,
        source: EventSource,
    ) {
        let message = source_error.to_string();
        let code = source_error.code();
        if let Err(store_error) = self.store.transition_task_from_with_event(
            task_id,
            &[TaskPhase::Provisioning, TaskPhase::Starting],
            TaskPhase::Failed,
            Some((code, &message)),
            EventDraft::task(
                EventKind::AgentFailed,
                source,
                json!({"stage": stage, "code": code, "message": message}),
            ),
        ) {
            error!(task_id, stage, %store_error, "could not persist task failure");
        }
    }
}

#[async_trait]
impl RpcHandler for Coordinator {
    async fn handle(&self, method: &str, params: Value) -> Result<Value, RpcErrorPayload> {
        self.dispatch(method, params)
            .await
            .map_err(CoordinatorError::into_rpc)
    }
}

#[derive(Debug, Error)]
enum CoordinatorError {
    #[error("invalid request parameters: {0}")]
    InvalidParams(String),
    #[error("unsupported context mode {0:?}")]
    UnsupportedContext(String),
    #[error("repository is not registered: {0}")]
    RepositoryNotRegistered(PathBuf),
    #[error("task already exists: {0}")]
    TaskExists(String),
    #[error("task not found: {0}")]
    TaskNotFound(String),
    #[error("operation ID was already used with different parameters")]
    IdempotencyConflict,
    #[error("task must be {expected}, but is {actual:?}")]
    InvalidTaskState {
        expected: &'static str,
        actual: TaskPhase,
    },
    #[error("task has no bound {0}")]
    IncompleteTask(&'static str),
    #[error("unknown daemon method {0:?}")]
    MethodNotFound(String),
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Profile(#[from] ProfileError),
    #[error(transparent)]
    Worker(#[from] WorkerError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl CoordinatorError {
    fn code(&self) -> &'static str {
        match self {
            Self::InvalidParams(_) => "INVALID_PARAMS",
            Self::UnsupportedContext(_) => "UNSUPPORTED_CONTEXT",
            Self::RepositoryNotRegistered(_) => "REPOSITORY_NOT_REGISTERED",
            Self::TaskExists(_) => "TASK_EXISTS",
            Self::TaskNotFound(_) => "TASK_NOT_FOUND",
            Self::IdempotencyConflict => "IDEMPOTENCY_CONFLICT",
            Self::InvalidTaskState { .. } => "INVALID_TASK_STATE",
            Self::IncompleteTask(_) => "INCOMPLETE_TASK",
            Self::MethodNotFound(_) => "METHOD_NOT_FOUND",
            Self::Git(GitError::DirtyRepository(_)) => "DIRTY_SOURCE",
            Self::Git(GitError::InvalidTaskName(_)) => "INVALID_TASK_NAME",
            Self::Git(GitError::BranchExists(_) | GitError::DestinationExists(_)) => {
                "TASK_COLLISION"
            }
            Self::Git(GitError::NotAWorktree(_)) => "NOT_A_GIT_REPOSITORY",
            Self::Git(_) => "GIT_ERROR",
            Self::Store(StoreError::NotFound { .. }) => "NOT_FOUND",
            Self::Store(StoreError::InvalidTaskTransition { .. }) => "INVALID_TASK_STATE",
            Self::Store(_) | Self::Json(_) => "INTERNAL",
            Self::Profile(ProfileError::NotFound { .. }) => "PROFILE_NOT_FOUND",
            Self::Profile(_) => "INVALID_PROFILE",
            Self::Worker(_) => "CODEX_ERROR",
        }
    }

    fn into_rpc(self) -> RpcErrorPayload {
        let code = self.code();
        let message = match self {
            Self::Worker(ref source) => {
                error!(%source, "Codex operation failed");
                "Codex could not accept the operation".to_owned()
            }
            Self::Store(ref source) => {
                error!(%source, "persistence operation failed");
                if code == "INTERNAL" {
                    "CoCo could not persist the operation".to_owned()
                } else {
                    source.to_string()
                }
            }
            Self::Json(ref source) => {
                error!(%source, "serialization failed");
                "CoCo could not encode the result".to_owned()
            }
            _ => self.to_string(),
        };
        RpcErrorPayload::new(code, message)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RepositoryRegisterParams {
    path: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TaskCreateParams {
    repository_path: PathBuf,
    name: String,
    base_ref: String,
    context_mode: String,
    goal: String,
    #[serde(default = "default_profile")]
    profile: String,
    operation_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TaskListParams {
    repository_path: PathBuf,
    #[serde(default)]
    phases: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TaskReferenceParams {
    repository_path: PathBuf,
    task: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TurnStartParams {
    repository_path: PathBuf,
    task: String,
    message: String,
    operation_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EventListParams {
    repository_path: PathBuf,
    task: String,
    #[serde(default)]
    after_sequence: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TaskDiffParams {
    repository_path: PathBuf,
    task: String,
    #[serde(default)]
    max_bytes: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AuditRecordParams {
    source: String,
    action: String,
    #[serde(default)]
    task_id: Option<String>,
    #[serde(default)]
    operation_id: Option<String>,
    outcome: String,
    #[serde(default)]
    details: Value,
}

fn default_profile() -> String {
    "default".to_owned()
}

fn parse_params<T: DeserializeOwned>(params: Value) -> Result<T, CoordinatorError> {
    serde_json::from_value(params)
        .map_err(|error| CoordinatorError::InvalidParams(error.to_string()))
}

fn validate_non_empty(field: &str, value: &str) -> Result<(), CoordinatorError> {
    if value.trim().is_empty() {
        Err(CoordinatorError::InvalidParams(format!(
            "{field} must not be empty"
        )))
    } else {
        Ok(())
    }
}

fn validate_operation_id(operation_id: &str) -> Result<(), CoordinatorError> {
    if operation_id.is_empty() || operation_id.len() > MAX_OPERATION_ID_BYTES {
        return Err(CoordinatorError::InvalidParams(format!(
            "operationId must contain 1-{MAX_OPERATION_ID_BYTES} bytes"
        )));
    }
    Ok(())
}

fn ensure_create_replay_matches(
    existing: &Task,
    params: &TaskCreateParams,
    repository_id: &str,
) -> Result<(), CoordinatorError> {
    let matches = existing.repository_id == repository_id
        && existing.name == params.name
        && existing.goal == params.goal.trim()
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

fn initial_prompt(task: &Task, repository: &Repository, worktree: &Path) -> String {
    format!(
        "{}\n\nCoCo task context:\n- task: {}\n- repository: {}\n- worktree: {}\n- branch: {}\n- immutable base: {}\n\nWork only inside the assigned worktree. Preserve unrelated user changes and verify your result before reporting completion.",
        task.goal,
        task.name,
        repository.root_path.display(),
        worktree.display(),
        task.branch_name.as_deref().unwrap_or("unknown"),
        task.base_sha.as_deref().unwrap_or("unknown"),
    )
}

fn message_fingerprint(operation_id: &str, task_id: &str, message: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(operation_id.as_bytes());
    digest.update([0]);
    digest.update(task_id.as_bytes());
    digest.update([0]);
    digest.update(message.as_bytes());
    format!("coco-{}", hex::encode(digest.finalize()))
}

fn task_git_binding(task: &Task) -> Option<(&Path, &str, &str)> {
    Some((
        task.worktree_path.as_deref()?,
        task.branch_name.as_deref()?,
        task.base_sha.as_deref()?,
    ))
}

fn task_and_turn_response(task: Task, turn: &Turn) -> Result<Value, CoordinatorError> {
    Ok(json!({
        "task": task,
        "turnId": turn.id,
        "codexTurnId": turn.codex_turn_id,
    }))
}

fn codex_event_draft(
    task: &Task,
    turn: Option<&Turn>,
    kind: EventKind,
    method: &str,
    payload: Value,
) -> EventDraft {
    EventDraft {
        task_id: Some(task.id.clone()),
        turn_id: turn.map(|turn| turn.id.clone()),
        kind,
        source: EventSource::Codex,
        source_method: Some(method.to_owned()),
        occurred_at_ms: None,
        payload,
    }
}

fn json_value<T: serde::Serialize>(value: T) -> Result<Value, CoordinatorError> {
    serde_json::to_value(value).map_err(CoordinatorError::from)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;
    use std::sync::Mutex as StdMutex;

    use tempfile::TempDir;

    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    enum WorkerCall {
        Thread {
            cwd: PathBuf,
            config: Value,
        },
        Turn {
            thread_id: String,
            cwd: PathBuf,
            client_message_id: String,
            message: String,
        },
    }

    #[derive(Default)]
    struct FakeWorker {
        calls: StdMutex<Vec<WorkerCall>>,
        fail_thread_start: bool,
    }

    impl FakeWorker {
        fn failing_thread_start() -> Self {
            Self {
                calls: StdMutex::new(Vec::new()),
                fail_thread_start: true,
            }
        }

        fn calls(&self) -> Vec<WorkerCall> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl WorkerRuntime for FakeWorker {
        async fn start_thread(
            &self,
            cwd: &Path,
            config: Value,
        ) -> Result<StartedThread, WorkerError> {
            let mut calls = self.calls.lock().unwrap();
            calls.push(WorkerCall::Thread {
                cwd: cwd.to_owned(),
                config,
            });
            if self.fail_thread_start {
                return Err(WorkerError::Unavailable("injected failure".to_owned()));
            }
            let sequence = calls
                .iter()
                .filter(|call| matches!(call, WorkerCall::Thread { .. }))
                .count();
            let id = format!("thread-{sequence}");
            Ok(StartedThread {
                id: id.clone(),
                response: json!({
                    "thread": {"id": id},
                    "cwd": cwd,
                    "model": "gpt-test",
                    "modelProvider": "test-provider",
                    "approvalPolicy": "on-request",
                    "approvalsReviewer": "user",
                    "sandbox": "workspace-write",
                }),
            })
        }

        async fn start_turn(
            &self,
            thread_id: &str,
            cwd: &Path,
            client_message_id: &str,
            message: &str,
        ) -> Result<StartedTurn, WorkerError> {
            let mut calls = self.calls.lock().unwrap();
            calls.push(WorkerCall::Turn {
                thread_id: thread_id.to_owned(),
                cwd: cwd.to_owned(),
                client_message_id: client_message_id.to_owned(),
                message: message.to_owned(),
            });
            let sequence = calls
                .iter()
                .filter(|call| matches!(call, WorkerCall::Turn { .. }))
                .count();
            Ok(StartedTurn {
                id: format!("turn-{sequence}"),
            })
        }
    }

    struct Fixture {
        _temp: TempDir,
        source: PathBuf,
        worktrees: PathBuf,
        store: Arc<Store>,
        worker: Arc<FakeWorker>,
        coordinator: Coordinator,
    }

    impl Fixture {
        fn new(worker: FakeWorker) -> Self {
            let temp = tempfile::tempdir().unwrap();
            let source = temp.path().join("source");
            run_git(
                temp.path(),
                &["init", "--initial-branch=main", source.to_str().unwrap()],
            );
            run_git(&source, &["config", "user.name", "CoCo Tests"]);
            run_git(&source, &["config", "user.email", "coco@example.invalid"]);
            fs::write(source.join("README.md"), "fixture\n").unwrap();
            run_git(&source, &["add", "README.md"]);
            run_git(&source, &["commit", "-m", "fixture"]);

            let worktrees = temp.path().join("worktrees");
            let codex_home = temp.path().join("codex-home");
            let store = Arc::new(Store::in_memory().unwrap());
            let worker = Arc::new(worker);
            let coordinator = Coordinator::new(
                Arc::clone(&store),
                Git::default(),
                worker.clone(),
                worktrees.clone(),
                codex_home,
            );
            Self {
                _temp: temp,
                source,
                worktrees,
                store,
                worker,
                coordinator,
            }
        }

        async fn register(&self) -> Repository {
            let value = self
                .coordinator
                .dispatch("repository.register", json!({"path": self.source}))
                .await
                .unwrap();
            serde_json::from_value(value).unwrap()
        }

        fn create_params(&self) -> Value {
            json!({
                "repositoryPath": self.source,
                "name": "first-task",
                "baseRef": "HEAD",
                "contextMode": "fresh",
                "goal": "Implement the requested behavior",
                "profile": "default",
                "operationId": "create-operation-1",
            })
        }
    }

    #[tokio::test]
    async fn runs_the_first_vertical_slice_and_replays_operation_ids() {
        let fixture = Fixture::new(FakeWorker::default());
        let repository = fixture.register().await;

        let created = fixture
            .coordinator
            .dispatch("task.create", fixture.create_params())
            .await
            .unwrap();
        let task: Task = serde_json::from_value(created["task"].clone()).unwrap();
        assert_eq!(task.phase, TaskPhase::Active);
        assert_eq!(task.codex_thread_id.as_deref(), Some("thread-1"));
        assert_eq!(created["codexTurnId"], "turn-1");
        assert_eq!(task.profile.effective_settings["model"], "gpt-test");
        let worktree = task.worktree_path.as_deref().unwrap();
        assert!(worktree.starts_with(fixture.worktrees.join(&repository.id)));
        assert!(worktree.join("README.md").is_file());

        let calls = fixture.worker.calls();
        assert_eq!(calls.len(), 2);
        assert!(matches!(
            &calls[0],
            WorkerCall::Thread { cwd, config }
                if cwd == worktree && config == &json!({})
        ));
        assert!(matches!(
            &calls[1],
            WorkerCall::Turn { thread_id, cwd, message, .. }
                if thread_id == "thread-1"
                    && cwd == worktree
                    && message.contains("Implement the requested behavior")
                    && message.contains("immutable base")
        ));

        let replay = fixture
            .coordinator
            .dispatch("task.create", fixture.create_params())
            .await
            .unwrap();
        assert_eq!(replay["task"]["id"], task.id);
        assert_eq!(fixture.worker.calls().len(), 2);

        let mut conflict = fixture.create_params();
        conflict["goal"] = Value::String("Different work".to_owned());
        assert!(matches!(
            fixture.coordinator.dispatch("task.create", conflict).await,
            Err(CoordinatorError::IdempotencyConflict)
        ));

        let events = fixture.store.events_after(Some(&task.id), 0).unwrap();
        assert_eq!(
            events.iter().map(|event| event.kind).collect::<Vec<_>>(),
            [
                EventKind::TaskCreated,
                EventKind::WorktreeCreated,
                EventKind::AgentStarted,
                EventKind::MessageReceived,
                EventKind::TurnStarted,
            ]
        );
    }

    #[tokio::test]
    async fn normalizes_codex_events_and_allows_an_idempotent_follow_up_turn() {
        let fixture = Fixture::new(FakeWorker::default());
        fixture.register().await;
        let created = fixture
            .coordinator
            .dispatch("task.create", fixture.create_params())
            .await
            .unwrap();
        let task: Task = serde_json::from_value(created["task"].clone()).unwrap();

        fixture
            .coordinator
            .record_codex_event(CodexEvent::ServerRequest {
                id: json!(17),
                method: "item/commandExecution/requestApproval".to_owned(),
                params: json!({
                    "threadId": "thread-1",
                    "turnId": "turn-1",
                    "reason": "needs network",
                    "environment": {"TOKEN": "must-not-persist"},
                }),
            })
            .unwrap();
        assert_eq!(
            fixture.store.task_by_id(&task.id).unwrap().unwrap().phase,
            TaskPhase::WaitingForApproval
        );
        let approval = fixture
            .store
            .events_after(Some(&task.id), 0)
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(approval.kind, EventKind::ApprovalRequested);
        assert!(!approval.payload.to_string().contains("must-not-persist"));

        fixture
            .coordinator
            .record_codex_event(CodexEvent::Notification {
                method: "item/completed".to_owned(),
                params: json!({
                    "threadId": "thread-1",
                    "turnId": "turn-1",
                    "completedAtMs": 42,
                    "item": {"id": "message-1", "type": "agentMessage", "text": "Done"},
                }),
            })
            .unwrap();
        fixture
            .coordinator
            .record_codex_event(CodexEvent::Notification {
                method: "turn/completed".to_owned(),
                params: json!({
                    "threadId": "thread-1",
                    "turn": {"id": "turn-1", "status": "completed"},
                }),
            })
            .unwrap();
        assert_eq!(
            fixture.store.task_by_id(&task.id).unwrap().unwrap().phase,
            TaskPhase::Idle
        );

        let send = json!({
            "repositoryPath": fixture.source,
            "task": task.name,
            "message": "Run the final checks",
            "operationId": "send-operation-1",
        });
        let started = fixture
            .coordinator
            .dispatch("turn.start", send.clone())
            .await
            .unwrap();
        assert_eq!(started["codexTurnId"], "turn-2");
        assert_eq!(fixture.worker.calls().len(), 3);

        let replay = fixture
            .coordinator
            .dispatch("turn.start", send.clone())
            .await
            .unwrap();
        assert_eq!(replay["turnId"], started["turnId"]);
        assert_eq!(fixture.worker.calls().len(), 3);

        let mut conflict = send;
        conflict["message"] = json!("A different retry");
        assert!(matches!(
            fixture.coordinator.dispatch("turn.start", conflict).await,
            Err(CoordinatorError::IdempotencyConflict)
        ));
    }

    #[tokio::test]
    async fn preserves_the_worktree_and_marks_the_task_failed_after_worker_failure() {
        let fixture = Fixture::new(FakeWorker::failing_thread_start());
        let repository = fixture.register().await;

        assert!(matches!(
            fixture
                .coordinator
                .dispatch("task.create", fixture.create_params())
                .await,
            Err(CoordinatorError::Worker(WorkerError::Unavailable(_)))
        ));
        let task = fixture
            .store
            .task_by_name(&repository.id, "first-task")
            .unwrap()
            .unwrap();
        assert_eq!(task.phase, TaskPhase::Failed);
        assert_eq!(task.last_error_code.as_deref(), Some("CODEX_ERROR"));
        assert!(task.worktree_path.unwrap().is_dir());
        assert_eq!(fixture.worker.calls().len(), 1);
    }

    #[tokio::test]
    async fn serves_repository_views_events_and_bounded_diffs() {
        let fixture = Fixture::new(FakeWorker::default());
        fixture.register().await;
        let created = fixture
            .coordinator
            .dispatch("task.create", fixture.create_params())
            .await
            .unwrap();
        let task: Task = serde_json::from_value(created["task"].clone()).unwrap();
        let worktree = task.worktree_path.as_deref().unwrap();
        fs::write(worktree.join("new.txt"), "new content\n").unwrap();

        let listed = fixture
            .coordinator
            .dispatch(
                "task.list",
                json!({"repositoryPath": fixture.source, "phases": ["active"]}),
            )
            .await
            .unwrap();
        assert_eq!(listed.as_array().unwrap().len(), 1);

        let shown = fixture
            .coordinator
            .dispatch(
                "task.get",
                json!({"repositoryPath": fixture.source, "task": task.id}),
            )
            .await
            .unwrap();
        assert_eq!(shown["git"]["observed"], true);
        assert_eq!(shown["git"]["dirty"], true);

        let events = fixture
            .coordinator
            .dispatch(
                "event.list",
                json!({
                    "repositoryPath": fixture.source,
                    "task": "first-task",
                    "afterSequence": 0,
                }),
            )
            .await
            .unwrap();
        assert_eq!(events["events"].as_array().unwrap().len(), 5);

        let diff = fixture
            .coordinator
            .dispatch(
                "task.diff",
                json!({
                    "repositoryPath": fixture.source,
                    "task": "first-task",
                    "maxBytes": 16,
                }),
            )
            .await
            .unwrap();
        assert_eq!(diff["untrackedPaths"], json!(["new.txt"]));
    }

    fn run_git(cwd: &Path, args: &[&str]) {
        let output = Command::new("git")
            .current_dir(cwd)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("LC_ALL", "C")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
