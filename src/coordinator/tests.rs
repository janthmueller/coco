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
    async fn start_thread(&self, cwd: &Path, config: Value) -> Result<StartedThread, WorkerError> {
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
        let _pending = PendingTurnGuard::new(&fixture.coordinator.pending_turn_threads, "thread-1");
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
