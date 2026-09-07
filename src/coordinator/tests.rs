use std::collections::BTreeMap;
use std::fs;
use std::process::Command;
use std::sync::Mutex as StdMutex;

use async_trait::async_trait;
use serde_json::Value;
use tempfile::TempDir;
use tokio::sync::Notify;

use super::*;
use crate::codex::CodexEvent;
use crate::domain::{
    CodexModel, CodexReasoningEffort, CodexThreadStatus, ContextMode, DecisionKind, DecisionPrompt,
    DecisionState, Workspace, WorkspaceLifecycle, WorkspacePhase, WorkspaceWaitReason,
};
use crate::protocol::{
    DecisionGetParams, DecisionRespondParams, DecisionSubmission, EventListParams,
    RepositoryRegisterParams, RepositoryScope, TurnStartParams, WorkspaceAttachParams,
    WorkspaceBaseRequest, WorkspaceChangesRequest, WorkspaceContextRequest, WorkspaceContextSource,
    WorkspaceCreateParams, WorkspaceDiffParams, WorkspaceGetParams, WorkspaceGitStatus,
    WorkspaceListParams, WorkspaceWorktreeRequest,
};
use crate::store::OperationState;

mod context;
mod creation;
mod decisions;
mod events;
mod operations;
mod workspace;

#[derive(Debug, Clone, PartialEq)]
enum WorkerCall {
    Models,
    Read {
        thread_id: String,
    },
    Thread {
        name: String,
        cwd: PathBuf,
        config: Value,
        model: Option<String>,
    },
    Resume {
        thread_id: String,
        cwd: PathBuf,
        config: Value,
        model: Option<String>,
    },
    Fork {
        name: String,
        source_thread_id: String,
        cwd: PathBuf,
        config: Value,
        model: Option<String>,
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
    native_threads: StdMutex<BTreeMap<String, NativeThread>>,
    failed_thread_reads: StdMutex<Vec<String>>,
    fail_thread_start: bool,
    fail_turn_start: bool,
    turn_start_entered: Option<Arc<Notify>>,
    turn_start_release: Option<Arc<Notify>>,
    fail_resume_thread: Option<String>,
    fail_compact: bool,
    resumed_thread_id: Option<String>,
    resumed_cwd: Option<PathBuf>,
    resumed_status: Option<CodexThreadStatus>,
}

impl FakeWorker {
    fn failing_thread_start() -> Self {
        Self {
            fail_thread_start: true,
            ..Self::default()
        }
    }

    fn failing_turn_start() -> Self {
        Self {
            fail_turn_start: true,
            ..Self::default()
        }
    }

    fn paused_turn_start(entered: Arc<Notify>, release: Arc<Notify>) -> Self {
        Self {
            turn_start_entered: Some(entered),
            turn_start_release: Some(release),
            ..Self::default()
        }
    }

    fn failing_compact() -> Self {
        Self {
            fail_compact: true,
            ..Self::default()
        }
    }

    fn with_resume_result(
        thread_id: Option<&str>,
        cwd: Option<PathBuf>,
        status: CodexThreadStatus,
    ) -> Self {
        Self {
            resumed_thread_id: thread_id.map(ToOwned::to_owned),
            resumed_cwd: cwd,
            resumed_status: Some(status),
            ..Self::default()
        }
    }

    fn calls(&self) -> Vec<WorkerCall> {
        self.calls.lock().unwrap().clone()
    }

    fn remember_native_thread(&self, thread: NativeThread) {
        self.native_threads
            .lock()
            .unwrap()
            .insert(thread.id.clone(), thread);
    }

    fn remember_bound_thread(&self, workspace: &Workspace, status: CodexThreadStatus) {
        self.remember_native_thread(NativeThread {
            id: workspace.codex_thread_id.clone().unwrap(),
            cwd: workspace.worktree_path.clone().unwrap(),
            name: Some(workspace.name.clone()),
            status,
            forked_from_id: workspace.parent_thread_id.clone(),
        });
    }

    fn set_native_status(&self, thread_id: &str, status: CodexThreadStatus) {
        self.native_threads
            .lock()
            .unwrap()
            .get_mut(thread_id)
            .expect("fake native thread was not registered")
            .status = status;
    }

    fn fail_thread_read(&self, thread_id: &str) {
        self.failed_thread_reads
            .lock()
            .unwrap()
            .push(thread_id.to_owned());
    }
}

#[async_trait]
impl WorkerRuntime for FakeWorker {
    async fn list_models(&self) -> Result<Vec<CodexModel>, WorkerError> {
        self.calls.lock().unwrap().push(WorkerCall::Models);
        Ok(vec![CodexModel {
            id: "gpt-test".to_owned(),
            model: "gpt-test".to_owned(),
            display_name: "GPT Test".to_owned(),
            description: "Test model".to_owned(),
            is_default: true,
            default_reasoning_effort: "medium".to_owned(),
            supported_reasoning_efforts: vec![CodexReasoningEffort {
                reasoning_effort: "medium".to_owned(),
                description: "Balanced".to_owned(),
            }],
            input_modalities: vec!["text".to_owned(), "image".to_owned()],
            supports_personality: true,
        }])
    }

    async fn read_thread(&self, thread_id: &str) -> Result<NativeThread, WorkerError> {
        self.calls.lock().unwrap().push(WorkerCall::Read {
            thread_id: thread_id.to_owned(),
        });
        if self
            .failed_thread_reads
            .lock()
            .unwrap()
            .iter()
            .any(|failed| failed == thread_id)
        {
            return Err(WorkerError::runtime(std::io::Error::other(
                "injected thread read failure",
            )));
        }
        self.native_threads
            .lock()
            .unwrap()
            .get(thread_id)
            .cloned()
            .ok_or_else(|| {
                WorkerError::InvalidThreadRead("fake native thread does not exist".to_owned())
            })
    }

    async fn start_thread(
        &self,
        name: &str,
        cwd: &Path,
        config: Value,
        model: Option<&str>,
    ) -> Result<StartedThread, WorkerError> {
        let effective_model = model.unwrap_or("gpt-test").to_owned();
        let mut calls = self.calls.lock().unwrap();
        calls.push(WorkerCall::Thread {
            name: name.to_owned(),
            cwd: cwd.to_owned(),
            config,
            model: model.map(ToOwned::to_owned),
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
        drop(calls);
        self.remember_native_thread(NativeThread {
            id: id.clone(),
            cwd: cwd.to_owned(),
            name: Some(name.to_owned()),
            status: CodexThreadStatus::Idle,
            forked_from_id: None,
        });
        Ok(StartedThread {
            id: id.clone(),
            status: CodexThreadStatus::Idle,
            cwd: cwd.to_owned(),
            response: json!({
                "thread": {"id": id, "status": {"type": "idle"}},
                "cwd": cwd,
                "model": effective_model,
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
        model: Option<&str>,
    ) -> Result<StartedThread, WorkerError> {
        self.calls.lock().unwrap().push(WorkerCall::Resume {
            thread_id: thread_id.to_owned(),
            cwd: cwd.to_owned(),
            config,
            model: model.map(ToOwned::to_owned),
        });
        if self.fail_resume_thread.as_deref() == Some(thread_id) {
            return Err(WorkerError::runtime(std::io::Error::other(
                "injected resume failure",
            )));
        }
        let resumed_thread_id = self
            .resumed_thread_id
            .as_deref()
            .unwrap_or(thread_id)
            .to_owned();
        let resumed_cwd = self.resumed_cwd.as_deref().unwrap_or(cwd).to_owned();
        let resumed_status = self
            .resumed_status
            .clone()
            .unwrap_or(CodexThreadStatus::Idle);
        self.remember_native_thread(NativeThread {
            id: resumed_thread_id.clone(),
            cwd: resumed_cwd.clone(),
            name: None,
            status: resumed_status.clone(),
            forked_from_id: None,
        });
        Ok(StartedThread {
            id: resumed_thread_id,
            status: resumed_status,
            cwd: resumed_cwd,
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
        model: Option<&str>,
    ) -> Result<StartedThread, WorkerError> {
        let effective_model = model.unwrap_or("gpt-test").to_owned();
        let mut calls = self.calls.lock().unwrap();
        calls.push(WorkerCall::Fork {
            name: name.to_owned(),
            source_thread_id: source_thread_id.to_owned(),
            cwd: cwd.to_owned(),
            config,
            model: model.map(ToOwned::to_owned),
        });
        let sequence = calls
            .iter()
            .filter(|call| matches!(call, WorkerCall::Fork { .. }))
            .count();
        let id = format!("fork-thread-{sequence}");
        drop(calls);
        self.remember_native_thread(NativeThread {
            id: id.clone(),
            cwd: cwd.to_owned(),
            name: Some(name.to_owned()),
            status: CodexThreadStatus::Idle,
            forked_from_id: Some(source_thread_id.to_owned()),
        });
        Ok(StartedThread {
            id: id.clone(),
            status: CodexThreadStatus::Idle,
            cwd: cwd.to_owned(),
            response: json!({
                "thread": {"id": id, "status": {"type": "idle"}},
                "cwd": cwd,
                "model": effective_model,
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
        let sequence = {
            let mut calls = self.calls.lock().unwrap();
            calls.push(WorkerCall::Turn {
                thread_id: thread_id.to_owned(),
                cwd: cwd.to_owned(),
                client_message_id: client_message_id.to_owned(),
                message: message.to_owned(),
                additional_context,
            });
            calls
                .iter()
                .filter(|call| matches!(call, WorkerCall::Turn { .. }))
                .count()
        };
        if let Some(entered) = &self.turn_start_entered {
            entered.notify_one();
        }
        if let Some(release) = &self.turn_start_release {
            release.notified().await;
        }
        if self.fail_turn_start {
            return Err(WorkerError::runtime(std::io::Error::other(
                "injected ambiguous turn failure",
            )));
        }
        self.set_native_status(
            thread_id,
            CodexThreadStatus::Active {
                active_flags: Vec::new(),
            },
        );
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
        fresh_create_params(self.source.clone(), "first-workspace", "create-operation-1")
    }

    fn fork_params(&self, source: &Workspace, name: &str, compact: bool) -> WorkspaceCreateParams {
        WorkspaceCreateParams {
            repository_path: self.source.clone(),
            name: name.to_owned(),
            context: WorkspaceContextRequest::Fork {
                source: WorkspaceContextSource::Workspace {
                    workspace: source.name.clone(),
                },
                compact,
            },
            worktree: WorkspaceWorktreeRequest::NewBranch {
                branch: None,
                base: WorkspaceBaseRequest::Workspace {
                    workspace: source.name.clone(),
                },
            },
            changes: WorkspaceChangesRequest::Reject,
            profile: "default".to_owned(),
            model: None,
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

fn fresh_create_params(
    repository_path: PathBuf,
    name: &str,
    operation_id: &str,
) -> WorkspaceCreateParams {
    WorkspaceCreateParams {
        repository_path,
        name: name.to_owned(),
        context: WorkspaceContextRequest::Fresh,
        worktree: WorkspaceWorktreeRequest::NewBranch {
            branch: None,
            base: WorkspaceBaseRequest::Revision {
                revision: "HEAD".to_owned(),
            },
        },
        changes: WorkspaceChangesRequest::Reject,
        profile: "default".to_owned(),
        model: None,
        operation_id: operation_id.to_owned(),
    }
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
