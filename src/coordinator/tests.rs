use std::collections::BTreeMap;
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
    CodexThreadStatus, ContextMode, DecisionKind, DecisionPrompt, DecisionState, Workspace,
    WorkspaceLifecycle, WorkspacePhase, WorkspaceWaitReason,
};
use crate::protocol::{
    DecisionGetParams, DecisionRespondParams, DecisionSubmission, EventListParams,
    RepositoryRegisterParams, RepositoryScope, TurnStartParams, WorkspaceCreateParams,
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
    Fork {
        name: String,
        source_thread_id: String,
        cwd: PathBuf,
        config: Value,
    },
    Compact {
        thread_id: String,
    },
    Turn {
        thread_id: String,
        cwd: PathBuf,
        client_message_id: String,
        message: String,
        additional_context: Option<Value>,
    },
    Response {
        id: Value,
        result: Value,
    },
}

#[derive(Default)]
struct FakeWorker {
    calls: StdMutex<Vec<WorkerCall>>,
    fail_thread_start: bool,
    fail_resume_thread: Option<String>,
    fail_compact: bool,
}

impl FakeWorker {
    fn failing_thread_start() -> Self {
        Self {
            calls: StdMutex::new(Vec::new()),
            fail_thread_start: true,
            fail_resume_thread: None,
            fail_compact: false,
        }
    }

    fn failing_resume(thread_id: &str) -> Self {
        Self {
            calls: StdMutex::new(Vec::new()),
            fail_thread_start: false,
            fail_resume_thread: Some(thread_id.to_owned()),
            fail_compact: false,
        }
    }

    fn failing_compact() -> Self {
        Self {
            calls: StdMutex::new(Vec::new()),
            fail_thread_start: false,
            fail_resume_thread: None,
            fail_compact: true,
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

    async fn fork_thread(
        &self,
        name: &str,
        source_thread_id: &str,
        cwd: &Path,
        config: Value,
    ) -> Result<StartedThread, WorkerError> {
        let mut calls = self.calls.lock().unwrap();
        calls.push(WorkerCall::Fork {
            name: name.to_owned(),
            source_thread_id: source_thread_id.to_owned(),
            cwd: cwd.to_owned(),
            config,
        });
        let sequence = calls
            .iter()
            .filter(|call| matches!(call, WorkerCall::Fork { .. }))
            .count();
        let id = format!("fork-thread-{sequence}");
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

    async fn compact_thread(&self, thread_id: &str) -> Result<(), WorkerError> {
        self.calls.lock().unwrap().push(WorkerCall::Compact {
            thread_id: thread_id.to_owned(),
        });
        if self.fail_compact {
            return Err(WorkerError::runtime(std::io::Error::other(
                "injected compaction failure",
            )));
        }
        Ok(())
    }

    async fn start_turn(
        &self,
        thread_id: &str,
        cwd: &Path,
        client_message_id: &str,
        message: &str,
        additional_context: Option<Value>,
    ) -> Result<StartedTurn, WorkerError> {
        let mut calls = self.calls.lock().unwrap();
        calls.push(WorkerCall::Turn {
            thread_id: thread_id.to_owned(),
            cwd: cwd.to_owned(),
            client_message_id: client_message_id.to_owned(),
            message: message.to_owned(),
            additional_context,
        });
        let sequence = calls
            .iter()
            .filter(|call| matches!(call, WorkerCall::Turn { .. }))
            .count();
        Ok(StartedTurn {
            id: format!("turn-{sequence}"),
        })
    }

    async fn respond_to_request(&self, id: Value, result: Value) -> Result<(), WorkerError> {
        self.calls
            .lock()
            .unwrap()
            .push(WorkerCall::Response { id, result });
        Ok(())
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
            fork_from: None,
            compact: false,
            profile: "default".to_owned(),
            operation_id: "create-operation-1".to_owned(),
        }
    }

    fn fork_params(&self, source: &Workspace, name: &str, compact: bool) -> WorkspaceCreateParams {
        WorkspaceCreateParams {
            repository_path: self.source.clone(),
            name: name.to_owned(),
            base_ref: "HEAD".to_owned(),
            context_mode: ContextMode::Fork,
            fork_from: Some(source.name.clone()),
            compact,
            profile: "default".to_owned(),
            operation_id: format!("create-{name}"),
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
async fn forks_committed_code_and_codex_history_from_an_idle_workspace() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let source = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let source_worktree = source.worktree_path.as_deref().unwrap();
    fs::write(source_worktree.join("source-commit.txt"), "from source\n").unwrap();
    run_git(source_worktree, &["add", "source-commit.txt"]);
    run_git(
        source_worktree,
        &["commit", "-m", "source workspace commit"],
    );
    let source_head = git_output(source_worktree, &["rev-parse", "HEAD"]);
    assert_ne!(
        source_head,
        git_output(&fixture.source, &["rev-parse", "HEAD"])
    );

    let fork_params = fixture.fork_params(&source, "forked-workspace", false);
    let created = fixture
        .coordinator
        .create_workspace(fork_params.clone())
        .await
        .unwrap();
    let workspace = created.workspace;
    let target_worktree = workspace.worktree_path.as_deref().unwrap();

    assert_eq!(workspace.lifecycle, WorkspaceLifecycle::Ready);
    assert_eq!(workspace.phase, WorkspacePhase::Idle);
    assert_eq!(workspace.context_mode, ContextMode::Fork);
    assert_eq!(workspace.base_sha.as_deref(), Some(source_head.as_str()));
    assert_eq!(workspace.parent_thread_id, source.codex_thread_id);
    assert_eq!(workspace.codex_thread_id.as_deref(), Some("fork-thread-1"));
    assert_eq!(workspace.context["mode"], "fork");
    assert_eq!(workspace.context["forkFrom"], source.name);
    assert_eq!(workspace.context["sourceWorkspaceId"], source.id);
    assert_eq!(workspace.context["sourceHeadSha"], source_head);
    assert_eq!(workspace.context["compact"], false);
    assert_eq!(
        git_output(target_worktree, &["rev-parse", "HEAD"]),
        source_head
    );
    assert_eq!(
        fs::read_to_string(target_worktree.join("source-commit.txt")).unwrap(),
        "from source\n"
    );
    assert!(matches!(
        fixture.worker.calls().as_slice(),
        [WorkerCall::Thread { .. }, WorkerCall::Fork {
            source_thread_id,
            cwd,
            config,
            ..
        }] if source_thread_id == "thread-1" && cwd == target_worktree && config == &json!({})
    ));
    let replay = fixture
        .coordinator
        .create_workspace(fork_params)
        .await
        .unwrap();
    assert_eq!(replay.workspace.id, workspace.id);
    assert_eq!(fixture.worker.calls().len(), 2);
}

#[tokio::test]
async fn rejects_a_dirty_or_active_fork_source_before_allocating_a_child() {
    let dirty_fixture = Fixture::new(FakeWorker::default());
    let repository = dirty_fixture.register().await;
    let source = dirty_fixture
        .coordinator
        .create_workspace(dirty_fixture.create_params())
        .await
        .unwrap()
        .workspace;
    fs::write(
        source.worktree_path.as_deref().unwrap().join("dirty.txt"),
        "not committed\n",
    )
    .unwrap();

    assert!(matches!(
        dirty_fixture
            .coordinator
            .create_workspace(dirty_fixture.fork_params(&source, "dirty-child", false))
            .await,
        Err(CoordinatorError::Git(
            crate::git::GitError::DirtyRepository(_)
        ))
    ));
    assert!(
        dirty_fixture
            .store
            .workspace_by_name(&repository.id, "dirty-child")
            .unwrap()
            .is_none()
    );
    assert_eq!(dirty_fixture.worker.calls().len(), 1);

    let active_fixture = Fixture::new(FakeWorker::default());
    active_fixture.register().await;
    let source = active_fixture
        .coordinator
        .create_workspace(active_fixture.create_params())
        .await
        .unwrap()
        .workspace;
    active_fixture
        .coordinator
        .start_turn(TurnStartParams {
            scope: RepositoryScope::repository(active_fixture.source.clone()),
            workspace: source.name.clone(),
            message: "keep working".to_owned(),
            operation_id: "activate-source".to_owned(),
        })
        .await
        .unwrap();
    assert!(matches!(
        active_fixture
            .coordinator
            .create_workspace(active_fixture.fork_params(&source, "active-child", false))
            .await,
        Err(CoordinatorError::InvalidWorkspaceState {
            expected: "an idle source workspace",
            actual: WorkspacePhase::Active,
        })
    ));
    assert_eq!(active_fixture.worker.calls().len(), 2);
}

#[tokio::test]
async fn compacts_only_the_child_before_it_accepts_a_message() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let source = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let create =
        fixture
            .coordinator
            .create_workspace(fixture.fork_params(&source, "compact-child", true));
    let complete_compaction = complete_fake_compaction(&fixture, "fork-thread-1");
    let (created, ()) = tokio::join!(create, complete_compaction);
    let workspace = created.unwrap().workspace;

    assert_eq!(workspace.lifecycle, WorkspaceLifecycle::Ready);
    assert_eq!(workspace.phase, WorkspacePhase::Idle);
    assert_eq!(workspace.context["compact"], true);
    assert!(matches!(
        fixture.worker.calls().as_slice(),
        [
            WorkerCall::Thread { .. },
            WorkerCall::Fork { .. },
            WorkerCall::Compact { thread_id },
        ] if thread_id == "fork-thread-1"
    ));
    let events = fixture.store.events_after(Some(&workspace.id), 0).unwrap();
    assert_eq!(
        events.iter().map(|event| event.kind).collect::<Vec<_>>(),
        [
            EventKind::WorkspaceCreated,
            EventKind::WorktreeCreated,
            EventKind::AgentStarted,
            EventKind::ContextCompacted,
        ]
    );

    fixture
        .coordinator
        .start_turn(TurnStartParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id.clone(),
            message: "continue in the child".to_owned(),
            operation_id: "send-after-compact".to_owned(),
        })
        .await
        .unwrap();
    let calls = fixture.worker.calls();
    let WorkerCall::Turn {
        additional_context: Some(additional_context),
        ..
    } = calls.last().unwrap()
    else {
        panic!("forked turn did not receive the workspace boundary context");
    };
    let binding = additional_context["coco.workspace-binding"]["value"]
        .as_str()
        .and_then(|value| serde_json::from_str::<Value>(value).ok())
        .expect("workspace boundary context was not valid JSON");
    assert_eq!(binding["workspaceId"], workspace.id);
    assert_eq!(binding["worktreePath"], json!(workspace.worktree_path));
    assert_eq!(binding["sourceWorkspaceId"], source.id);
}

async fn complete_fake_compaction(fixture: &Fixture, thread_id: &str) {
    wait_for_compaction_request(fixture).await;
    for event in [
        CodexEvent::Notification {
            method: "turn/started".to_owned(),
            params: json!({
                "threadId": thread_id,
                "turn": {"id": "compact-turn", "status": "inProgress"},
            }),
        },
        CodexEvent::Notification {
            method: "item/completed".to_owned(),
            params: json!({
                "threadId": thread_id,
                "turnId": "compact-turn",
                "item": {"id": "compact-item", "type": "contextCompaction"},
            }),
        },
        CodexEvent::Notification {
            method: "turn/completed".to_owned(),
            params: json!({
                "threadId": thread_id,
                "turn": {"id": "compact-turn", "status": "completed"},
            }),
        },
    ] {
        fixture.coordinator.record_codex_event(event).unwrap();
    }
}

async fn wait_for_compaction_request(fixture: &Fixture) {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if fixture
                .worker
                .calls()
                .iter()
                .any(|call| matches!(call, WorkerCall::Compact { .. }))
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("coordinator did not request child compaction");
}

#[tokio::test]
async fn retains_a_bound_failed_child_when_compaction_cannot_start() {
    let fixture = Fixture::new(FakeWorker::failing_compact());
    let repository = fixture.register().await;
    let source = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;

    assert!(matches!(
        fixture
            .coordinator
            .create_workspace(fixture.fork_params(&source, "failed-compact", true))
            .await,
        Err(CoordinatorError::CompactionFailed(_))
    ));
    let failed = fixture
        .store
        .workspace_by_name(&repository.id, "failed-compact")
        .unwrap()
        .unwrap();
    assert_eq!(failed.lifecycle, WorkspaceLifecycle::Failed);
    assert_eq!(failed.phase, WorkspacePhase::Failed);
    assert_eq!(
        failed.last_error_code.as_deref(),
        Some("CODEX_COMPACTION_FAILED")
    );
    assert_eq!(failed.codex_thread_id.as_deref(), Some("fork-thread-1"));
    assert_eq!(failed.parent_thread_id, source.codex_thread_id);
    assert!(failed.worktree_path.unwrap().is_dir());
}

#[tokio::test]
async fn retains_a_bound_failed_child_when_native_compaction_fails() {
    let fixture = Fixture::new(FakeWorker::default());
    let repository = fixture.register().await;
    let source = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let create = fixture.coordinator.create_workspace(fixture.fork_params(
        &source,
        "failed-native-compact",
        true,
    ));
    let fail_compaction = async {
        wait_for_compaction_request(&fixture).await;
        for event in [
            CodexEvent::Notification {
                method: "turn/started".to_owned(),
                params: json!({
                    "threadId": "fork-thread-1",
                    "turn": {"id": "failed-compact-turn", "status": "inProgress"},
                }),
            },
            CodexEvent::Notification {
                method: "turn/completed".to_owned(),
                params: json!({
                    "threadId": "fork-thread-1",
                    "turn": {"id": "failed-compact-turn", "status": "failed"},
                }),
            },
        ] {
            fixture.coordinator.record_codex_event(event).unwrap();
        }
    };
    let (result, ()) = tokio::join!(create, fail_compaction);
    assert!(matches!(result, Err(CoordinatorError::CompactionFailed(_))));

    let failed = fixture
        .store
        .workspace_by_name(&repository.id, "failed-native-compact")
        .unwrap()
        .unwrap();
    assert_eq!(failed.lifecycle, WorkspaceLifecycle::Failed);
    assert_eq!(failed.codex_thread_id.as_deref(), Some("fork-thread-1"));
    assert!(failed.worktree_path.unwrap().is_dir());
}

#[tokio::test]
async fn rejects_incoherent_or_unimplemented_context_requests() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;

    let mut fresh_with_fork = fixture.create_params();
    fresh_with_fork.fork_from = Some("source".to_owned());
    assert!(matches!(
        fixture.coordinator.create_workspace(fresh_with_fork).await,
        Err(CoordinatorError::InvalidParams(_))
    ));

    let mut missing_source = fixture.create_params();
    missing_source.context_mode = ContextMode::Fork;
    assert!(matches!(
        fixture.coordinator.create_workspace(missing_source).await,
        Err(CoordinatorError::InvalidParams(_))
    ));

    let mut blank_source = fixture.create_params();
    blank_source.context_mode = ContextMode::Fork;
    blank_source.fork_from = Some("  ".to_owned());
    assert!(matches!(
        fixture.coordinator.create_workspace(blank_source).await,
        Err(CoordinatorError::InvalidParams(_))
    ));

    let mut handoff = fixture.create_params();
    handoff.context_mode = ContextMode::Handoff;
    assert!(matches!(
        fixture.coordinator.create_workspace(handoff).await,
        Err(CoordinatorError::UnsupportedContext(mode)) if mode == "handoff"
    ));
    assert!(fixture.worker.calls().is_empty());
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
        scope: RepositoryScope::repository(fixture.source.clone()),
        workspace: workspace.name.clone(),
        message: "Implement the requested behavior".to_owned(),
        operation_id: "send-operation-initial".to_owned(),
    };
    let first_started = fixture.coordinator.start_turn(first_send).await.unwrap();
    assert_eq!(first_started.codex_turn_id.as_deref(), Some("turn-1"));
    assert_eq!(first_started.workspace.phase, WorkspacePhase::Active);

    record_and_approve_command(&fixture, &workspace).await;

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
        scope: RepositoryScope::repository(fixture.source.clone()),
        workspace: workspace.name.clone(),
        message: "Run the final checks".to_owned(),
        operation_id: "send-operation-1".to_owned(),
    };
    let started = fixture.coordinator.start_turn(send.clone()).await.unwrap();
    assert_eq!(started.codex_turn_id.as_deref(), Some("turn-2"));
    assert_eq!(fixture.worker.calls().len(), 4);

    let replay = fixture.coordinator.start_turn(send.clone()).await.unwrap();
    assert_eq!(replay.turn_id, started.turn_id);
    assert_eq!(fixture.worker.calls().len(), 4);

    let mut conflict = send;
    conflict.message = "A different retry".to_owned();
    assert!(matches!(
        fixture.coordinator.start_turn(conflict).await,
        Err(CoordinatorError::IdempotencyConflict)
    ));
}

async fn record_and_approve_command(fixture: &Fixture, workspace: &Workspace) {
    fixture
        .coordinator
        .record_codex_event(CodexEvent::ServerRequest {
            id: json!(17),
            method: "item/commandExecution/requestApproval".to_owned(),
            params: json!({
                "threadId": "thread-1",
                "turnId": "turn-1",
                "reason": "needs network",
                "additionalPermissions": {
                    "fileSystem": {"write": ["/shared/cache"]},
                    "network": {"enabled": true}
                },
                "environment": {"TOKEN": "must-not-persist"},
            }),
        })
        .unwrap();
    let request = fixture
        .store
        .events_after(Some(&workspace.id), 0)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(request.kind, EventKind::DecisionRequested);
    assert!(!request.payload.to_string().contains("must-not-persist"));
    assert!(!request.payload.to_string().contains("17"));
    let status = fixture
        .coordinator
        .get_workspace(WorkspaceGetParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id.clone(),
        })
        .unwrap();
    let decision = status.open_decisions.first().unwrap();
    assert_eq!(decision.state, DecisionState::Pending);
    assert!(
        !serde_json::to_string(decision)
            .unwrap()
            .contains("must-not-persist")
    );
    assert!(
        serde_json::to_string(decision)
            .unwrap()
            .contains("/shared/cache")
    );
    assert!(matches!(decision.prompt, DecisionPrompt::Approval(_)));
    let submitted = fixture
        .coordinator
        .respond_decision(DecisionRespondParams {
            decision_id: decision.id.clone(),
            submission: DecisionSubmission::Choice { choice: 1 },
        })
        .await
        .unwrap();
    assert_eq!(submitted.decision.state, DecisionState::Submitted);
    assert!(matches!(
        fixture.worker.calls().last(),
        Some(WorkerCall::Response { id, result })
            if id == &json!(17) && result == &json!({"decision": "accept"})
    ));
    fixture
        .coordinator
        .record_codex_event(CodexEvent::Notification {
            method: "serverRequest/resolved".to_owned(),
            params: json!({"threadId": "thread-1", "requestId": 17}),
        })
        .unwrap();
    assert!(
        fixture
            .store
            .open_decisions_for_workspace(&workspace.id)
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn forwards_validated_user_input_without_exposing_answer_values() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    fixture
        .coordinator
        .start_turn(TurnStartParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id.clone(),
            message: "Ask me a question".to_owned(),
            operation_id: "question-turn".to_owned(),
        })
        .await
        .unwrap();
    fixture
        .coordinator
        .record_codex_event(CodexEvent::ServerRequest {
            id: json!("native-question-1"),
            method: "item/tool/requestUserInput".to_owned(),
            params: json!({
                "threadId": "thread-1",
                "turnId": "turn-1",
                "itemId": "question-item",
                "isBlocking": true,
                "questions": [{
                    "id": "strategy",
                    "header": "Strategy",
                    "question": "Which strategy should Codex use?",
                    "options": [
                        {"label": "Safe", "description": "Prefer safety"},
                        {"label": "Fast", "description": "Prefer speed"}
                    ],
                    "isOther": true,
                    "isSecret": false
                }]
            }),
        })
        .unwrap();
    let status = fixture
        .coordinator
        .get_workspace(WorkspaceGetParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id.clone(),
        })
        .unwrap();
    let decision = status.open_decisions.first().unwrap();
    assert_eq!(decision.kind, DecisionKind::UserInput);
    let decision_id = decision.id.clone();
    let calls_before = fixture.worker.calls().len();

    let invalid = fixture
        .coordinator
        .respond_decision(DecisionRespondParams {
            decision_id: decision_id.clone(),
            submission: DecisionSubmission::Answers {
                answers: BTreeMap::from([("strategy".to_owned(), "  ".to_owned())]),
            },
        })
        .await
        .unwrap_err();
    assert_eq!(invalid.code(), "INVALID_PARAMS");
    assert_eq!(fixture.worker.calls().len(), calls_before);

    let private_answer = "A private custom strategy".to_owned();
    fixture
        .coordinator
        .respond_decision(DecisionRespondParams {
            decision_id: decision_id.clone(),
            submission: DecisionSubmission::Answers {
                answers: BTreeMap::from([("strategy".to_owned(), private_answer.clone())]),
            },
        })
        .await
        .unwrap();
    assert!(matches!(
        fixture.worker.calls().last(),
        Some(WorkerCall::Response { id, result })
            if id == &json!("native-question-1")
                && result == &json!({
                    "answers": {"strategy": {"answers": ["A private custom strategy"]}}
                })
    ));
    let submitted = fixture
        .coordinator
        .get_decision(DecisionGetParams { decision_id })
        .unwrap();
    assert_eq!(submitted.decision.state, DecisionState::Submitted);
    assert!(
        !serde_json::to_string(&submitted)
            .unwrap()
            .contains(&private_answer)
    );
}

#[tokio::test]
async fn presents_bounded_file_changes_and_orphans_them_on_disconnect() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    fixture
        .coordinator
        .start_turn(TurnStartParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id.clone(),
            message: "Change a file".to_owned(),
            operation_id: "file-turn".to_owned(),
        })
        .await
        .unwrap();
    fixture
        .coordinator
        .record_codex_event(CodexEvent::Notification {
            method: "item/started".to_owned(),
            params: json!({
                "threadId": "thread-1",
                "turnId": "turn-1",
                "startedAtMs": 20,
                "item": {
                    "id": "file-item",
                    "type": "fileChange",
                    "status": "inProgress",
                    "changes": [{
                        "path": "src/main.rs",
                        "kind": {"type": "update", "move_path": null},
                        "diff": "@@ -1 +1 @@\n-old\n+new\n"
                    }]
                }
            }),
        })
        .unwrap();
    fixture
        .coordinator
        .record_codex_event(CodexEvent::ServerRequest {
            id: json!(18),
            method: "item/fileChange/requestApproval".to_owned(),
            params: json!({
                "threadId": "thread-1",
                "turnId": "turn-1",
                "itemId": "file-item",
                "startedAtMs": 21,
                "reason": "Apply the patch"
            }),
        })
        .unwrap();
    let status = fixture
        .coordinator
        .get_workspace(WorkspaceGetParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id.clone(),
        })
        .unwrap();
    let decision = status.open_decisions.first().unwrap();
    let DecisionPrompt::Approval(prompt) = &decision.prompt else {
        panic!("file approval was not projected as an approval")
    };
    assert_eq!(prompt.changes.len(), 1);
    assert_eq!(prompt.changes[0].path, PathBuf::from("src/main.rs"));
    assert_eq!(prompt.changes[0].kind, "update");
    assert!(prompt.changes[0].diff.contains("+new"));
    let decision_id = decision.id.clone();

    assert_eq!(fixture.coordinator.record_codex_disconnected().unwrap(), 1);
    let orphaned = fixture
        .coordinator
        .get_decision(DecisionGetParams { decision_id })
        .unwrap();
    assert_eq!(orphaned.decision.state, DecisionState::Orphaned);
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
            scope: RepositoryScope::repository(fixture.source.clone()),
            phases: Some(vec!["idle".to_owned()]),
        })
        .unwrap();
    assert_eq!(listed.len(), 1);

    let shown = fixture
        .coordinator
        .get_workspace(WorkspaceGetParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
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
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: "first-workspace".to_owned(),
            after_sequence: 0,
        })
        .unwrap();
    assert_eq!(events.events.len(), 3);

    let diff = fixture
        .coordinator
        .workspace_diff(WorkspaceDiffParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: "first-workspace".to_owned(),
            max_bytes: Some(16),
        })
        .unwrap();
    assert_eq!(diff.untracked_paths, [PathBuf::from("new.txt")]);
}

struct MultiRepositorySetup {
    first_repository: Repository,
    second_repository: Repository,
    second_source: PathBuf,
    first: Workspace,
    second: Workspace,
    unique: Workspace,
}

async fn prepare_multi_repository_workspaces(fixture: &Fixture) -> MultiRepositorySetup {
    let first_repository = fixture.register().await;
    let second_source = fixture._temp.path().join("source-two");
    initialize_repository(&second_source);
    let second_repository = fixture
        .coordinator
        .register_repository(RepositoryRegisterParams {
            path: second_source.clone(),
        })
        .unwrap();

    let first = fixture
        .coordinator
        .create_workspace(WorkspaceCreateParams {
            repository_path: fixture.source.clone(),
            name: "feat/shared".to_owned(),
            base_ref: "HEAD".to_owned(),
            context_mode: ContextMode::Fresh,
            fork_from: None,
            compact: false,
            profile: "default".to_owned(),
            operation_id: "create-first-shared".to_owned(),
        })
        .await
        .unwrap()
        .workspace;
    let second = fixture
        .coordinator
        .create_workspace(WorkspaceCreateParams {
            repository_path: second_source.clone(),
            name: "feat/shared".to_owned(),
            base_ref: "HEAD".to_owned(),
            context_mode: ContextMode::Fresh,
            fork_from: None,
            compact: false,
            profile: "default".to_owned(),
            operation_id: "create-second-shared".to_owned(),
        })
        .await
        .unwrap()
        .workspace;
    let unique = fixture
        .coordinator
        .create_workspace(WorkspaceCreateParams {
            repository_path: second_source.clone(),
            name: "fix/unique".to_owned(),
            base_ref: "HEAD".to_owned(),
            context_mode: ContextMode::Fresh,
            fork_from: None,
            compact: false,
            profile: "default".to_owned(),
            operation_id: "create-second-unique".to_owned(),
        })
        .await
        .unwrap()
        .workspace;

    MultiRepositorySetup {
        first_repository,
        second_repository,
        second_source,
        first,
        second,
        unique,
    }
}

#[tokio::test]
async fn scopes_workspace_names_to_repositories_and_resolves_global_references() {
    let fixture = Fixture::new(FakeWorker::default());
    let MultiRepositorySetup {
        first_repository,
        second_repository,
        second_source,
        first,
        second,
        unique,
    } = prepare_multi_repository_workspaces(&fixture).await;

    let repositories = fixture
        .coordinator
        .list_repositories(crate::protocol::RepositoryListParams {})
        .unwrap();
    assert_eq!(repositories.len(), 2);
    assert!(
        repositories
            .iter()
            .any(|item| item.id == first_repository.id)
    );
    assert!(
        repositories
            .iter()
            .any(|item| item.id == second_repository.id)
    );

    let listed = fixture
        .coordinator
        .list_workspaces(WorkspaceListParams {
            scope: RepositoryScope::AllRepositories,
            phases: None,
        })
        .unwrap();
    assert_eq!(listed.len(), 3);
    assert!(listed.iter().all(|item| {
        item.repository.id == item.workspace.repository_id
            && [fixture.source.as_path(), second_source.as_path()]
                .contains(&item.repository.root_path.as_path())
    }));

    let local = fixture
        .coordinator
        .get_workspace(WorkspaceGetParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: "feat/shared".to_owned(),
        })
        .unwrap();
    assert_eq!(local.workspace.id, first.id);

    let global_id = fixture
        .coordinator
        .get_workspace(WorkspaceGetParams {
            scope: RepositoryScope::AllRepositories,
            workspace: second.id.clone(),
        })
        .unwrap();
    assert_eq!(global_id.workspace.id, second.id);

    let global_unique = fixture
        .coordinator
        .get_workspace(WorkspaceGetParams {
            scope: RepositoryScope::AllRepositories,
            workspace: unique.name.clone(),
        })
        .unwrap();
    assert_eq!(global_unique.workspace.id, unique.id);

    let ambiguous = fixture
        .coordinator
        .get_workspace(WorkspaceGetParams {
            scope: RepositoryScope::AllRepositories,
            workspace: "feat/shared".to_owned(),
        })
        .unwrap_err();
    assert_error_match_count(&ambiguous, "WORKSPACE_REFERENCE_AMBIGUOUS", 2);
    let CoordinatorError::WorkspaceReferenceAmbiguous { candidates, .. } = ambiguous else {
        panic!("global duplicate name did not produce an ambiguity error");
    };
    assert_eq!(candidates.len(), 2);
    assert!(candidates.iter().any(|item| item.workspace_id == first.id));
    assert!(candidates.iter().any(|item| item.workspace_id == second.id));

    let local_miss = fixture
        .coordinator
        .get_workspace(WorkspaceGetParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: unique.name.clone(),
        })
        .unwrap_err();
    assert_error_match_count(&local_miss, "WORKSPACE_NOT_FOUND", 1);
    let CoordinatorError::WorkspaceNotFound { candidates, .. } = local_miss else {
        panic!("repository-local miss did not remain a not-found error");
    };
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].workspace_id, unique.id);
    assert_eq!(candidates[0].repository_path, second_source);
}

fn assert_error_match_count(error: &CoordinatorError, code: &str, expected: usize) {
    assert_eq!(error.code(), code);
    assert_eq!(
        error
            .data()
            .and_then(|data| data["matches"].as_array().map(Vec::len)),
        Some(expected)
    );
}

fn initialize_repository(path: &Path) {
    run_git(
        path.parent().unwrap(),
        &["init", "--initial-branch=main", path.to_str().unwrap()],
    );
    run_git(path, &["config", "user.name", "CoCo Tests"]);
    run_git(path, &["config", "user.email", "coco@example.invalid"]);
    fs::write(path.join("README.md"), "fixture\n").unwrap();
    run_git(path, &["add", "README.md"]);
    run_git(path, &["commit", "-m", "fixture"]);
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

fn git_output(cwd: &Path, args: &[&str]) -> String {
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
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
