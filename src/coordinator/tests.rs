use std::fs;
use std::process::Command;
use std::sync::Mutex as StdMutex;

use async_trait::async_trait;
use serde_json::Value;
use tempfile::TempDir;

use super::turn::PendingTurnGuard;
use super::*;
use crate::codex::CodexEvent;
use crate::domain::{
    CodexThreadStatus, ContextMode, Workspace, WorkspaceLifecycle, WorkspacePhase,
    WorkspaceWaitReason,
};
use crate::protocol::{
    EventListParams, RepositoryRegisterParams, TurnStartParams, WorkspaceCreateParams,
    WorkspaceDiffParams, WorkspaceGetParams, WorkspaceGitStatus, WorkspaceListParams,
};

#[derive(Debug, Clone, PartialEq)]
enum WorkerCall {
    Thread {
        name: String,
        cwd: PathBuf,
        config: Value,
    },
    Resume {
        thread_id: String,
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
    fail_resume_thread: Option<String>,
}

impl FakeWorker {
    fn failing_thread_start() -> Self {
        Self {
            calls: StdMutex::new(Vec::new()),
            fail_thread_start: true,
            fail_resume_thread: None,
        }
    }

    fn failing_resume(thread_id: &str) -> Self {
        Self {
            calls: StdMutex::new(Vec::new()),
            fail_thread_start: false,
            fail_resume_thread: Some(thread_id.to_owned()),
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
        name: &str,
        cwd: &Path,
        config: Value,
    ) -> Result<StartedThread, WorkerError> {
        let mut calls = self.calls.lock().unwrap();
        calls.push(WorkerCall::Thread {
            name: name.to_owned(),
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
            status: CodexThreadStatus::Idle,
            cwd: cwd.to_owned(),
            response: json!({
                "thread": {"id": id, "status": {"type": "idle"}},
                "cwd": cwd,
                "model": "gpt-test",
                "modelProvider": "test-provider",
                "approvalPolicy": "on-request",
                "approvalsReviewer": "user",
                "sandbox": "workspace-write",
            }),
        })
    }

    async fn resume_thread(
        &self,
        thread_id: &str,
        cwd: &Path,
        config: Value,
    ) -> Result<StartedThread, WorkerError> {
        self.calls.lock().unwrap().push(WorkerCall::Resume {
            thread_id: thread_id.to_owned(),
            cwd: cwd.to_owned(),
            config,
        });
        if self.fail_resume_thread.as_deref() == Some(thread_id) {
            return Err(WorkerError::runtime(std::io::Error::other(
                "injected resume failure",
            )));
        }
        Ok(StartedThread {
            id: thread_id.to_owned(),
            status: CodexThreadStatus::Idle,
            cwd: cwd.to_owned(),
            response: json!({
                "thread": {"id": thread_id, "status": {"type": "idle"}},
                "cwd": cwd,
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
    codex_home: PathBuf,
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
            codex_home.clone(),
            "runtime-test".to_owned(),
        );
        Self {
            _temp: temp,
            source,
            worktrees,
            codex_home,
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

    fn create_params(&self) -> WorkspaceCreateParams {
        WorkspaceCreateParams {
            repository_path: self.source.clone(),
            name: "first-workspace".to_owned(),
            base_ref: "HEAD".to_owned(),
            context_mode: ContextMode::Fresh,
            profile: "default".to_owned(),
            operation_id: "create-operation-1".to_owned(),
        }
    }

    fn recovery_coordinator(
        &self,
        worker: Arc<FakeWorker>,
        runtime_generation: &str,
    ) -> Coordinator {
        Coordinator::new(
            Arc::clone(&self.store),
            Git::default(),
            worker,
            self.worktrees.clone(),
            self.codex_home.clone(),
            runtime_generation.to_owned(),
        )
    }
}

#[tokio::test]
async fn prepares_an_idle_workspace_without_starting_a_turn_and_replays_operation_ids() {
    let fixture = Fixture::new(FakeWorker::default());
    let repository = fixture.register().await;

    let created = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap();
    let workspace = created.workspace.clone();
    assert_eq!(workspace.lifecycle, WorkspaceLifecycle::Ready);
    assert_eq!(workspace.phase, WorkspacePhase::Idle);
    assert_eq!(
        workspace
            .thread_runtime
            .as_ref()
            .map(|snapshot| &snapshot.status),
        Some(&CodexThreadStatus::Idle)
    );
    assert!(workspace.thread_runtime.as_ref().unwrap().is_fresh);
    assert_eq!(workspace.codex_thread_id.as_deref(), Some("thread-1"));
    assert!(created.turn_id.is_none());
    assert_eq!(workspace.profile.effective_settings["model"], "gpt-test");
    let worktree = workspace.worktree_path.as_deref().unwrap();
    assert!(worktree.starts_with(fixture.worktrees.join(&repository.id)));
    assert!(worktree.join("README.md").is_file());

    let calls = fixture.worker.calls();
    assert_eq!(calls.len(), 1);
    assert!(matches!(
        &calls[0],
        WorkerCall::Thread { name, cwd, config }
            if name == "first-workspace" && cwd == worktree && config == &json!({})
    ));
    let replay = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap();
    assert_eq!(replay.workspace.id, workspace.id);
    assert_eq!(fixture.worker.calls().len(), 1);

    let mut conflict = fixture.create_params();
    conflict.base_ref = "different-base".to_owned();
    assert!(matches!(
        fixture.coordinator.create_workspace(conflict).await,
        Err(CoordinatorError::IdempotencyConflict)
    ));

    let events = fixture.store.events_after(Some(&workspace.id), 0).unwrap();
    assert_eq!(
        events.iter().map(|event| event.kind).collect::<Vec<_>>(),
        [
            EventKind::WorkspaceCreated,
            EventKind::WorktreeCreated,
            EventKind::AgentStarted,
        ]
    );
}

#[tokio::test]
async fn recovers_a_ready_thread_with_its_stored_worktree_and_profile() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let created = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap();
    let workspace = created.workspace;
    assert!(fixture.store.reconcile_unfinished().unwrap().is_empty());
    assert_eq!(
        fixture
            .store
            .workspace_by_id(&workspace.id)
            .unwrap()
            .unwrap()
            .phase,
        WorkspacePhase::Unavailable
    );

    let worker = Arc::new(FakeWorker::default());
    let coordinator = fixture.recovery_coordinator(worker.clone(), "runtime-recovered");
    let report = coordinator.recover_ready_threads().await.unwrap();
    assert_eq!(report.attempted, 1);
    assert_eq!(report.recovered, 1);
    assert_eq!(report.failed, 0);

    let recovered = fixture
        .store
        .workspace_by_id(&workspace.id)
        .unwrap()
        .unwrap();
    assert_eq!(recovered.phase, WorkspacePhase::Idle);
    let runtime = recovered.thread_runtime.unwrap();
    assert!(runtime.is_fresh);
    assert_eq!(runtime.runtime_generation, "runtime-recovered");
    assert_eq!(runtime.status, CodexThreadStatus::Idle);
    assert_eq!(
        worker.calls(),
        [WorkerCall::Resume {
            thread_id: "thread-1".to_owned(),
            cwd: workspace.worktree_path.unwrap(),
            config: json!({}),
        }]
    );
    let event = fixture
        .store
        .events_after(Some(&workspace.id), 0)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(event.kind, EventKind::ThreadStatusChanged);
    assert_eq!(event.source_method.as_deref(), Some("thread/resume"));
    assert_eq!(event.payload["reason"], "daemon_recovery");
}

#[tokio::test]
async fn isolates_resume_failure_and_retries_only_the_unavailable_thread() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let first = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let mut second_params = fixture.create_params();
    second_params.name = "second-workspace".to_owned();
    second_params.operation_id = "create-operation-2".to_owned();
    let second = fixture
        .coordinator
        .create_workspace(second_params)
        .await
        .unwrap()
        .workspace;
    fixture.store.reconcile_unfinished().unwrap();

    let failing_worker = Arc::new(FakeWorker::failing_resume("thread-1"));
    let coordinator = fixture.recovery_coordinator(failing_worker.clone(), "runtime-recovered");
    let report = coordinator.recover_ready_threads().await.unwrap();
    assert_eq!(report.attempted, 2);
    assert_eq!(report.recovered, 1);
    assert_eq!(report.failed, 1);

    let unavailable = fixture.store.workspace_by_id(&first.id).unwrap().unwrap();
    assert_eq!(unavailable.phase, WorkspacePhase::Unavailable);
    assert_eq!(
        unavailable.last_error_code.as_deref(),
        Some("THREAD_RECOVERY_FAILED")
    );
    assert_eq!(
        unavailable.last_error_message.as_deref(),
        Some("Codex could not resume the stored thread")
    );
    let recovered = fixture.store.workspace_by_id(&second.id).unwrap().unwrap();
    assert_eq!(recovered.phase, WorkspacePhase::Idle);
    assert!(recovered.thread_runtime.unwrap().is_fresh);

    let retry_worker = Arc::new(FakeWorker::default());
    let retry = fixture.recovery_coordinator(retry_worker.clone(), "runtime-recovered");
    let retry_report = retry.recover_ready_threads().await.unwrap();
    assert_eq!(retry_report.attempted, 1);
    assert_eq!(retry_report.recovered, 1);
    assert_eq!(retry_report.failed, 0);
    let retried = fixture.store.workspace_by_id(&first.id).unwrap().unwrap();
    assert_eq!(retried.phase, WorkspacePhase::Idle);
    assert_eq!(retried.last_error_code, None);
    assert_eq!(retried.last_error_message, None);
    assert_eq!(retry_worker.calls().len(), 1);
}

#[tokio::test]
async fn refuses_to_resume_when_the_named_profile_changed() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    fs::create_dir_all(&fixture.codex_home).unwrap();
    fs::write(
        fixture.codex_home.join("config.toml"),
        "[profiles.dev]\nmodel = \"gpt-before\"\n",
    )
    .unwrap();
    let mut params = fixture.create_params();
    params.profile = "dev".to_owned();
    let workspace = fixture
        .coordinator
        .create_workspace(params)
        .await
        .unwrap()
        .workspace;
    fixture.store.reconcile_unfinished().unwrap();
    fs::write(
        fixture.codex_home.join("config.toml"),
        "[profiles.dev]\nmodel = \"gpt-after\"\n",
    )
    .unwrap();

    let worker = Arc::new(FakeWorker::default());
    let coordinator = fixture.recovery_coordinator(worker.clone(), "runtime-recovered");
    let report = coordinator.recover_ready_threads().await.unwrap();
    assert_eq!(report.attempted, 1);
    assert_eq!(report.recovered, 0);
    assert_eq!(report.failed, 1);
    assert!(worker.calls().is_empty());
    let unavailable = fixture
        .store
        .workspace_by_id(&workspace.id)
        .unwrap()
        .unwrap();
    assert_eq!(unavailable.phase, WorkspacePhase::Unavailable);
    assert_eq!(
        unavailable.last_error_message.as_deref(),
        Some("The workspace profile changed after the thread was created")
    );
    let failure = fixture
        .store
        .events_after(Some(&workspace.id), 0)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(failure.kind, EventKind::AgentFailed);
    assert_eq!(failure.payload["causeCode"], "PROFILE_CHANGED");
    assert!(!failure.payload.to_string().contains("gpt-after"));
}

#[tokio::test]
async fn normalizes_codex_events_and_allows_an_idempotent_follow_up_turn() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let created = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap();
    let workspace = created.workspace;

    let first_send = TurnStartParams {
        repository_path: fixture.source.clone(),
        workspace: workspace.name.clone(),
        message: "Implement the requested behavior".to_owned(),
        operation_id: "send-operation-initial".to_owned(),
    };
    let first_started = fixture.coordinator.start_turn(first_send).await.unwrap();
    assert_eq!(first_started.codex_turn_id.as_deref(), Some("turn-1"));
    assert_eq!(first_started.workspace.phase, WorkspacePhase::Active);

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
        fixture
            .store
            .workspace_by_id(&workspace.id)
            .unwrap()
            .unwrap()
            .phase,
        WorkspacePhase::Active
    );
    let request = fixture
        .store
        .events_after(Some(&workspace.id), 0)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(request.kind, EventKind::ServerRequestReceived);
    assert!(!request.payload.to_string().contains("must-not-persist"));

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
        fixture
            .store
            .workspace_by_id(&workspace.id)
            .unwrap()
            .unwrap()
            .phase,
        WorkspacePhase::Idle
    );

    let send = TurnStartParams {
        repository_path: fixture.source.clone(),
        workspace: workspace.name.clone(),
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

fn record_thread_status(fixture: &Fixture, status: Value) {
    fixture
        .coordinator
        .record_codex_event(CodexEvent::Notification {
            method: "thread/status/changed".to_owned(),
            params: json!({"threadId": "thread-1", "status": status}),
        })
        .unwrap();
}

fn assert_waiting_status_projection(fixture: &Fixture, workspace: &Workspace) {
    record_thread_status(
        fixture,
        json!({
            "type": "active",
            "activeFlags": ["waitingOnUserInput", "futureFlag", "waitingOnApproval"]
        }),
    );
    let waiting = fixture
        .store
        .workspace_by_id(&workspace.id)
        .unwrap()
        .unwrap();
    assert_eq!(waiting.phase, WorkspacePhase::WaitingForApproval);
    assert_eq!(
        waiting.wait_reasons,
        [
            WorkspaceWaitReason::Approval,
            WorkspaceWaitReason::UserInput
        ]
    );
    assert_eq!(
        waiting
            .thread_runtime
            .as_ref()
            .map(|snapshot| &snapshot.status),
        Some(&CodexThreadStatus::Active {
            active_flags: vec![
                "futureFlag".to_owned(),
                "waitingOnApproval".to_owned(),
                "waitingOnUserInput".to_owned(),
            ]
        })
    );

    record_thread_status(
        fixture,
        json!({"type": "active", "activeFlags": ["waitingOnUserInput"]}),
    );
    assert_eq!(
        fixture
            .store
            .workspace_by_id(&workspace.id)
            .unwrap()
            .unwrap()
            .phase,
        WorkspacePhase::WaitingForInput
    );
    record_thread_status(fixture, json!({"type": "active", "activeFlags": []}));
    assert_eq!(
        fixture
            .store
            .workspace_by_id(&workspace.id)
            .unwrap()
            .unwrap()
            .phase,
        WorkspacePhase::Active
    );
}

fn assert_nonactive_status_projection(fixture: &Fixture, workspace: &Workspace) {
    record_thread_status(fixture, json!({"type": "systemError"}));
    assert_eq!(
        fixture
            .store
            .workspace_by_id(&workspace.id)
            .unwrap()
            .unwrap()
            .phase,
        WorkspacePhase::SystemError
    );
    record_thread_status(fixture, json!({"type": "notLoaded"}));
    assert_eq!(
        fixture
            .store
            .workspace_by_id(&workspace.id)
            .unwrap()
            .unwrap()
            .phase,
        WorkspacePhase::NotLoaded
    );
    assert_eq!(fixture.coordinator.record_codex_disconnected().unwrap(), 1);
    assert_eq!(
        fixture
            .store
            .workspace_by_id(&workspace.id)
            .unwrap()
            .unwrap()
            .phase,
        WorkspacePhase::Unavailable
    );
}

#[tokio::test]
async fn tracks_turns_started_by_an_external_tui_and_runtime_waiting_states() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let created = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap();
    let workspace = created.workspace;
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
            fixture
                .store
                .workspace_by_id(&workspace.id)
                .unwrap()
                .unwrap()
                .phase,
            WorkspacePhase::Idle
        );
    }

    fixture.coordinator.record_codex_event(started).unwrap();
    let active = fixture
        .store
        .workspace_by_id(&workspace.id)
        .unwrap()
        .unwrap();
    assert_eq!(active.phase, WorkspacePhase::Active);
    assert!(active.active_turn_id.is_some());
    assert!(
        fixture
            .store
            .turn_by_codex_id("external-turn-1")
            .unwrap()
            .is_some()
    );

    assert_waiting_status_projection(&fixture, &workspace);

    fixture
        .coordinator
        .record_codex_event(CodexEvent::Notification {
            method: "thread/status/changed".to_owned(),
            params: json!({
                "threadId": "thread-1",
                "status": {"type": "idle"},
            }),
        })
        .unwrap();
    assert_eq!(
        fixture
            .store
            .workspace_by_id(&workspace.id)
            .unwrap()
            .unwrap()
            .phase,
        WorkspacePhase::Active
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
        fixture
            .store
            .workspace_by_id(&workspace.id)
            .unwrap()
            .unwrap()
            .phase,
        WorkspacePhase::Idle
    );

    assert_nonactive_status_projection(&fixture, &workspace);
}

#[tokio::test]
async fn preserves_the_worktree_and_marks_the_workspace_failed_after_worker_failure() {
    let fixture = Fixture::new(FakeWorker::failing_thread_start());
    let repository = fixture.register().await;

    assert!(matches!(
        fixture
            .coordinator
            .create_workspace(fixture.create_params())
            .await,
        Err(CoordinatorError::Worker(WorkerError::Runtime(_)))
    ));
    let workspace = fixture
        .store
        .workspace_by_name(&repository.id, "first-workspace")
        .unwrap()
        .unwrap();
    assert_eq!(workspace.phase, WorkspacePhase::Failed);
    assert_eq!(workspace.lifecycle, WorkspaceLifecycle::Failed);
    assert_eq!(workspace.last_error_code.as_deref(), Some("CODEX_ERROR"));
    assert!(workspace.worktree_path.unwrap().is_dir());
    assert_eq!(fixture.worker.calls().len(), 1);
}

#[tokio::test]
async fn serves_repository_views_events_and_bounded_diffs() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let created = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap();
    let workspace = created.workspace;
    let worktree = workspace.worktree_path.as_deref().unwrap();
    fs::write(worktree.join("new.txt"), "new content\n").unwrap();

    let listed = fixture
        .coordinator
        .list_workspaces(WorkspaceListParams {
            repository_path: fixture.source.clone(),
            phases: Some(vec!["idle".to_owned()]),
        })
        .unwrap();
    assert_eq!(listed.len(), 1);

    let shown = fixture
        .coordinator
        .get_workspace(WorkspaceGetParams {
            repository_path: fixture.source.clone(),
            workspace: workspace.id.clone(),
        })
        .unwrap();
    assert!(matches!(
        shown.git,
        WorkspaceGitStatus::Observed(ref observation) if observation.observed && observation.dirty
    ));

    let events = fixture
        .coordinator
        .list_events(EventListParams {
            repository_path: fixture.source.clone(),
            workspace: "first-workspace".to_owned(),
            after_sequence: 0,
        })
        .unwrap();
    assert_eq!(events.events.len(), 3);

    let diff = fixture
        .coordinator
        .workspace_diff(WorkspaceDiffParams {
            repository_path: fixture.source.clone(),
            workspace: "first-workspace".to_owned(),
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
