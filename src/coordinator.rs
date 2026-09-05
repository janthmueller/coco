use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};

use serde_json::json;
use tokio::sync::Mutex as AsyncMutex;
use tracing::error;

use crate::domain::{EventKind, EventSource, Repository, Task, TaskPhase};
use crate::git::{Git, GitRepository};
use crate::protocol::TaskResult;
use crate::store::{EventDraft, Store};

const MAX_OPERATION_ID_BYTES: usize = 256;

mod codex_events;
mod error;
mod task;
mod turn;
mod worker;

pub(crate) use error::CoordinatorError;
pub(crate) use worker::{StartedThread, StartedTurn, WorkerError, WorkerRuntime};

pub(crate) struct Coordinator {
    store: Arc<Store>,
    git: Git,
    worker: Arc<dyn WorkerRuntime>,
    worktrees_dir: PathBuf,
    codex_home: PathBuf,
    repository_locks: AsyncMutex<HashMap<String, Arc<AsyncMutex<()>>>>,
    pending_turn_threads: StdMutex<HashSet<String>>,
}

impl Coordinator {
    pub(crate) fn new(
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
            pending_turn_threads: StdMutex::new(HashSet::new()),
        }
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

    fn task_response(&self, task: Task) -> Result<TaskResult, CoordinatorError> {
        let turn = task
            .active_turn_id
            .as_deref()
            .map(|turn_id| self.store.turn_by_id(turn_id))
            .transpose()?
            .flatten();
        match turn {
            Some(turn) => Ok(TaskResult::with_turn(task, &turn)),
            None => Ok(TaskResult::prepared(task)),
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;
    use std::sync::Mutex as StdMutex;

    use async_trait::async_trait;
    use serde_json::Value;
    use tempfile::TempDir;

    use super::turn::PendingTurnGuard;
    use super::*;
    use crate::codex::CodexEvent;
    use crate::domain::ContextMode;
    use crate::protocol::{
        EventListParams, RepositoryRegisterParams, TaskCreateParams, TaskDiffParams, TaskGetParams,
        TaskGitStatus, TaskListParams, TurnStartParams,
    };

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
                return Err(WorkerError::runtime(std::io::Error::other(
                    "injected failure",
                )));
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
            self.coordinator
                .register_repository(RepositoryRegisterParams {
                    path: self.source.clone(),
                })
                .unwrap()
        }

        fn create_params(&self) -> TaskCreateParams {
            TaskCreateParams {
                repository_path: self.source.clone(),
                name: "first-task".to_owned(),
                base_ref: "HEAD".to_owned(),
                context_mode: ContextMode::Fresh,
                profile: "default".to_owned(),
                operation_id: "create-operation-1".to_owned(),
            }
        }
    }

    #[tokio::test]
    async fn prepares_an_idle_task_without_starting_a_turn_and_replays_operation_ids() {
        let fixture = Fixture::new(FakeWorker::default());
        let repository = fixture.register().await;

        let created = fixture
            .coordinator
            .create_task(fixture.create_params())
            .await
            .unwrap();
        let task = created.task.clone();
        assert_eq!(task.phase, TaskPhase::Idle);
        assert_eq!(task.codex_thread_id.as_deref(), Some("thread-1"));
        assert!(created.turn_id.is_none());
        assert_eq!(task.profile.effective_settings["model"], "gpt-test");
        let worktree = task.worktree_path.as_deref().unwrap();
        assert!(worktree.starts_with(fixture.worktrees.join(&repository.id)));
        assert!(worktree.join("README.md").is_file());

        let calls = fixture.worker.calls();
        assert_eq!(calls.len(), 1);
        assert!(matches!(
            &calls[0],
            WorkerCall::Thread { cwd, config }
                if cwd == worktree && config == &json!({})
        ));
        let replay = fixture
            .coordinator
            .create_task(fixture.create_params())
            .await
            .unwrap();
        assert_eq!(replay.task.id, task.id);
        assert_eq!(fixture.worker.calls().len(), 1);

        let mut conflict = fixture.create_params();
        conflict.base_ref = "different-base".to_owned();
        assert!(matches!(
            fixture.coordinator.create_task(conflict).await,
            Err(CoordinatorError::IdempotencyConflict)
        ));

        let events = fixture.store.events_after(Some(&task.id), 0).unwrap();
        assert_eq!(
            events.iter().map(|event| event.kind).collect::<Vec<_>>(),
            [
                EventKind::TaskCreated,
                EventKind::WorktreeCreated,
                EventKind::AgentStarted,
            ]
        );
    }

    #[tokio::test]
    async fn normalizes_codex_events_and_allows_an_idempotent_follow_up_turn() {
        let fixture = Fixture::new(FakeWorker::default());
        fixture.register().await;
        let created = fixture
            .coordinator
            .create_task(fixture.create_params())
            .await
            .unwrap();
        let task = created.task;

        let first_send = TurnStartParams {
            repository_path: fixture.source.clone(),
            task: task.name.clone(),
            message: "Implement the requested behavior".to_owned(),
            operation_id: "send-operation-initial".to_owned(),
        };
        let first_started = fixture.coordinator.start_turn(first_send).await.unwrap();
        assert_eq!(first_started.codex_turn_id.as_deref(), Some("turn-1"));

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

        let send = TurnStartParams {
            repository_path: fixture.source.clone(),
            task: task.name.clone(),
            message: "Run the final checks".to_owned(),
            operation_id: "send-operation-1".to_owned(),
        };
        let started = fixture.coordinator.start_turn(send.clone()).await.unwrap();
        assert_eq!(started.codex_turn_id.as_deref(), Some("turn-2"));
        assert_eq!(fixture.worker.calls().len(), 3);

        let replay = fixture.coordinator.start_turn(send.clone()).await.unwrap();
        assert_eq!(replay.turn_id, started.turn_id);
        assert_eq!(fixture.worker.calls().len(), 3);

        let mut conflict = send;
        conflict.message = "A different retry".to_owned();
        assert!(matches!(
            fixture.coordinator.start_turn(conflict).await,
            Err(CoordinatorError::IdempotencyConflict)
        ));
    }

    #[tokio::test]
    async fn tracks_turns_started_by_an_external_tui_and_runtime_waiting_states() {
        let fixture = Fixture::new(FakeWorker::default());
        fixture.register().await;
        let created = fixture
            .coordinator
            .create_task(fixture.create_params())
            .await
            .unwrap();
        let task = created.task;
        let started = CodexEvent::Notification {
            method: "turn/started".to_owned(),
            params: json!({
                "threadId": "thread-1",
                "turn": {"id": "external-turn-1", "status": "inProgress"},
            }),
        };

        {
            let _pending =
                PendingTurnGuard::new(&fixture.coordinator.pending_turn_threads, "thread-1");
            fixture
                .coordinator
                .record_codex_event(started.clone())
                .unwrap();
            assert_eq!(
                fixture.store.task_by_id(&task.id).unwrap().unwrap().phase,
                TaskPhase::Idle
            );
        }

        fixture.coordinator.record_codex_event(started).unwrap();
        let active = fixture.store.task_by_id(&task.id).unwrap().unwrap();
        assert_eq!(active.phase, TaskPhase::Active);
        assert!(active.active_turn_id.is_some());
        assert!(
            fixture
                .store
                .turn_by_codex_id("external-turn-1")
                .unwrap()
                .is_some()
        );

        fixture
            .coordinator
            .record_codex_event(CodexEvent::Notification {
                method: "thread/status/changed".to_owned(),
                params: json!({
                    "threadId": "thread-1",
                    "status": {"type": "active", "activeFlags": ["waitingOnUserInput"]},
                }),
            })
            .unwrap();
        assert_eq!(
            fixture.store.task_by_id(&task.id).unwrap().unwrap().phase,
            TaskPhase::WaitingForInput
        );

        fixture
            .coordinator
            .record_codex_event(CodexEvent::Notification {
                method: "thread/status/changed".to_owned(),
                params: json!({
                    "threadId": "thread-1",
                    "status": {"type": "active", "activeFlags": ["waitingOnApproval"]},
                }),
            })
            .unwrap();
        assert_eq!(
            fixture.store.task_by_id(&task.id).unwrap().unwrap().phase,
            TaskPhase::WaitingForApproval
        );

        fixture
            .coordinator
            .record_codex_event(CodexEvent::Notification {
                method: "thread/status/changed".to_owned(),
                params: json!({
                    "threadId": "thread-1",
                    "status": {"type": "active", "activeFlags": []},
                }),
            })
            .unwrap();
        assert_eq!(
            fixture.store.task_by_id(&task.id).unwrap().unwrap().phase,
            TaskPhase::Active
        );

        fixture
            .coordinator
            .record_codex_event(CodexEvent::Notification {
                method: "turn/completed".to_owned(),
                params: json!({
                    "threadId": "thread-1",
                    "turn": {"id": "external-turn-1", "status": "completed"},
                }),
            })
            .unwrap();
        assert_eq!(
            fixture.store.task_by_id(&task.id).unwrap().unwrap().phase,
            TaskPhase::Idle
        );
    }

    #[tokio::test]
    async fn preserves_the_worktree_and_marks_the_task_failed_after_worker_failure() {
        let fixture = Fixture::new(FakeWorker::failing_thread_start());
        let repository = fixture.register().await;

        assert!(matches!(
            fixture
                .coordinator
                .create_task(fixture.create_params())
                .await,
            Err(CoordinatorError::Worker(WorkerError::Runtime(_)))
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
            .create_task(fixture.create_params())
            .await
            .unwrap();
        let task = created.task;
        let worktree = task.worktree_path.as_deref().unwrap();
        fs::write(worktree.join("new.txt"), "new content\n").unwrap();

        let listed = fixture
            .coordinator
            .list_tasks(TaskListParams {
                repository_path: fixture.source.clone(),
                phases: Some(vec!["idle".to_owned()]),
            })
            .unwrap();
        assert_eq!(listed.len(), 1);

        let shown = fixture
            .coordinator
            .get_task(TaskGetParams {
                repository_path: fixture.source.clone(),
                task: task.id.clone(),
            })
            .unwrap();
        assert!(matches!(
            shown.git,
            TaskGitStatus::Observed(ref observation) if observation.observed && observation.dirty
        ));

        let events = fixture
            .coordinator
            .list_events(EventListParams {
                repository_path: fixture.source.clone(),
                task: "first-task".to_owned(),
                after_sequence: 0,
            })
            .unwrap();
        assert_eq!(events.events.len(), 3);

        let diff = fixture
            .coordinator
            .task_diff(TaskDiffParams {
                repository_path: fixture.source.clone(),
                task: "first-task".to_owned(),
                max_bytes: Some(16),
            })
            .unwrap();
        assert_eq!(diff.untracked_paths, [PathBuf::from("new.txt")]);
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
