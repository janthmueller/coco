use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::process::Command;
use std::sync::Mutex as StdMutex;

use async_trait::async_trait;
use serde_json::Value;
use tempfile::TempDir;
use tokio::sync::Notify;

use super::*;
use crate::codex::CodexEvent;
use crate::domain::runtime::WorkspaceRuntimeResources;
use crate::domain::{
    CodexModel, CodexReasoningEffort, CodexThreadStatus, ContextMode, DecisionKind, DecisionPrompt,
    DecisionState, Workspace, WorkspaceAvailability, WorkspaceLifecycle, WorkspacePhase,
    WorkspaceWaitReason,
};
use crate::hooks::HookRegistry;
use crate::protocol::{
    DecisionGetParams, DecisionRespondParams, DecisionSubmission, EventListParams,
    RepositoryRegisterParams, RepositoryScope, TurnResult, TurnResultParams, TurnStartParams,
    TurnTerminalStatus, WorkspaceAttachLaunch, WorkspaceAttachParams, WorkspaceAttachReleaseParams,
    WorkspaceBaseRequest, WorkspaceChangesRequest, WorkspaceCloseParams, WorkspaceContextRequest,
    WorkspaceContextSource, WorkspaceCreateParams, WorkspaceDeleteParams, WorkspaceDiffParams,
    WorkspaceGetParams, WorkspaceGitStatus, WorkspaceListParams, WorkspaceReopenParams,
    WorkspaceThreadDisposition, WorkspaceWorktreeRequest,
};
use crate::store::{OperationState, WorkspaceDeletionIntent};

mod context;
mod creation;
mod decisions;
mod events;
mod guards;
mod jump;
mod operations;
mod retirement;
mod retirement_confirmation;
mod retirement_dependencies;
mod retirement_safety;
mod workspace;

#[derive(Debug, Clone, PartialEq)]
enum WorkerCall {
    Models,
    Read {
        thread_id: String,
    },
    FindMaterialized {
        thread_id: String,
        cwd: PathBuf,
    },
    Name {
        thread_id: String,
        name: String,
    },
    Locate {
        thread_id: String,
    },
    Descendants {
        thread_id: String,
    },
    BackgroundTerminals {
        thread_id: String,
    },
    Unsubscribe {
        thread_id: String,
    },
    Archive {
        thread_id: String,
    },
    Unarchive {
        thread_id: String,
    },
    DeleteThread {
        thread_id: String,
    },
    StopExecution {
        workspace_id: String,
    },
    Resources {
        workspace_id: String,
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
    materialized_threads: StdMutex<HashSet<String>>,
    archived_threads: StdMutex<HashSet<String>>,
    descendants: StdMutex<BTreeMap<String, Vec<String>>>,
    background_terminals: StdMutex<BTreeMap<String, usize>>,
    runtime_resources: StdMutex<Option<WorkspaceRuntimeResources>>,
    failed_thread_reads: StdMutex<Vec<String>>,
    fail_thread_start: bool,
    fail_turn_start: bool,
    turn_start_entered: Option<Arc<Notify>>,
    turn_start_release: Option<Arc<Notify>>,
    thread_read_entered: Option<Arc<Notify>>,
    thread_read_release: Option<Arc<Notify>>,
    fail_resume_thread: Option<String>,
    fail_compact: bool,
    fail_delete_thread: bool,
    fail_delete_thread_after_removal: bool,
    activate_on_unsubscribe: StdMutex<Option<String>>,
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

    fn paused_thread_read(entered: Arc<Notify>, release: Arc<Notify>) -> Self {
        Self {
            thread_read_entered: Some(entered),
            thread_read_release: Some(release),
            ..Self::default()
        }
    }

    fn failing_compact() -> Self {
        Self {
            fail_compact: true,
            ..Self::default()
        }
    }

    fn failing_delete_thread() -> Self {
        Self {
            fail_delete_thread: true,
            ..Self::default()
        }
    }

    fn ambiguously_deleted_thread() -> Self {
        Self {
            fail_delete_thread_after_removal: true,
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

    fn remember_materialized_thread(&self, thread: NativeThread) {
        let thread_id = thread.id.clone();
        self.remember_native_thread(thread);
        self.materialized_threads.lock().unwrap().insert(thread_id);
    }

    fn remember_bound_thread(&self, workspace: &Workspace, status: CodexThreadStatus) {
        let thread_id = workspace.codex_thread_id.clone().unwrap();
        self.remember_native_thread(NativeThread {
            id: thread_id.clone(),
            cwd: workspace.worktree_path.clone().unwrap(),
            name: Some(workspace.name.clone()),
            status,
            forked_from_id: workspace.parent_thread_id.clone(),
        });
        self.materialized_threads.lock().unwrap().insert(thread_id);
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

    fn set_descendants(&self, thread_id: &str, descendants: &[&str]) {
        self.descendants.lock().unwrap().insert(
            thread_id.to_owned(),
            descendants
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
        );
    }

    fn set_background_terminals(&self, thread_id: &str, count: usize) {
        self.background_terminals
            .lock()
            .unwrap()
            .insert(thread_id.to_owned(), count);
    }

    fn set_runtime_resources(&self, resources: WorkspaceRuntimeResources) {
        *self.runtime_resources.lock().unwrap() = Some(resources);
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
        if let Some(entered) = &self.thread_read_entered {
            entered.notify_one();
        }
        if let Some(release) = &self.thread_read_release {
            release.notified().await;
        }
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

    async fn find_materialized_thread(
        &self,
        thread_id: &str,
        cwd: &Path,
    ) -> Result<Option<NativeThread>, WorkerError> {
        self.calls
            .lock()
            .unwrap()
            .push(WorkerCall::FindMaterialized {
                thread_id: thread_id.to_owned(),
                cwd: cwd.to_owned(),
            });
        if !self
            .materialized_threads
            .lock()
            .unwrap()
            .contains(thread_id)
        {
            return Ok(None);
        }
        self.native_threads
            .lock()
            .unwrap()
            .get(thread_id)
            .cloned()
            .map(Some)
            .ok_or_else(|| {
                WorkerError::InvalidThreadRead(
                    "materialized fake native thread does not exist".to_owned(),
                )
            })
    }

    async fn set_thread_name(&self, thread_id: &str, name: &str) -> Result<(), WorkerError> {
        self.calls.lock().unwrap().push(WorkerCall::Name {
            thread_id: thread_id.to_owned(),
            name: name.to_owned(),
        });
        let mut threads = self.native_threads.lock().unwrap();
        let thread = threads.get_mut(thread_id).ok_or_else(|| {
            WorkerError::InvalidThreadRead("fake native thread does not exist".to_owned())
        })?;
        thread.name = Some(name.to_owned());
        Ok(())
    }

    async fn locate_thread(
        &self,
        thread_id: &str,
    ) -> Result<Option<LocatedNativeThread>, WorkerError> {
        self.calls.lock().unwrap().push(WorkerCall::Locate {
            thread_id: thread_id.to_owned(),
        });
        let thread = self.native_threads.lock().unwrap().get(thread_id).cloned();
        Ok(thread.map(|thread| LocatedNativeThread {
            archived: self.archived_threads.lock().unwrap().contains(thread_id),
            thread,
        }))
    }

    async fn list_thread_descendants(&self, thread_id: &str) -> Result<Vec<String>, WorkerError> {
        self.calls.lock().unwrap().push(WorkerCall::Descendants {
            thread_id: thread_id.to_owned(),
        });
        Ok(self
            .descendants
            .lock()
            .unwrap()
            .get(thread_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn background_terminal_count(&self, thread_id: &str) -> Result<usize, WorkerError> {
        self.calls
            .lock()
            .unwrap()
            .push(WorkerCall::BackgroundTerminals {
                thread_id: thread_id.to_owned(),
            });
        Ok(*self
            .background_terminals
            .lock()
            .unwrap()
            .get(thread_id)
            .unwrap_or(&0))
    }

    async fn unsubscribe_thread(&self, thread_id: &str) -> Result<(), WorkerError> {
        self.calls.lock().unwrap().push(WorkerCall::Unsubscribe {
            thread_id: thread_id.to_owned(),
        });
        if let Some(child) = self.activate_on_unsubscribe.lock().unwrap().take() {
            self.set_native_status(
                &child,
                CodexThreadStatus::Active {
                    active_flags: Vec::new(),
                },
            );
        }
        Ok(())
    }

    async fn archive_thread(&self, thread_id: &str) -> Result<(), WorkerError> {
        self.calls.lock().unwrap().push(WorkerCall::Archive {
            thread_id: thread_id.to_owned(),
        });
        if !self.native_threads.lock().unwrap().contains_key(thread_id) {
            return Err(WorkerError::InvalidThreadRead(
                "fake native thread does not exist".to_owned(),
            ));
        }
        self.archived_threads
            .lock()
            .unwrap()
            .insert(thread_id.to_owned());
        Ok(())
    }

    async fn unarchive_thread(&self, thread_id: &str) -> Result<NativeThread, WorkerError> {
        self.calls.lock().unwrap().push(WorkerCall::Unarchive {
            thread_id: thread_id.to_owned(),
        });
        self.archived_threads.lock().unwrap().remove(thread_id);
        self.native_threads
            .lock()
            .unwrap()
            .get(thread_id)
            .cloned()
            .ok_or_else(|| {
                WorkerError::InvalidThreadRead("fake native thread does not exist".to_owned())
            })
    }

    async fn delete_thread(&self, thread_id: &str) -> Result<(), WorkerError> {
        self.calls.lock().unwrap().push(WorkerCall::DeleteThread {
            thread_id: thread_id.to_owned(),
        });
        if self.fail_delete_thread {
            return Err(WorkerError::InvalidThreadRead(
                "injected native thread deletion failure".to_owned(),
            ));
        }
        self.archived_threads.lock().unwrap().remove(thread_id);
        self.native_threads.lock().unwrap().remove(thread_id);
        if self.fail_delete_thread_after_removal {
            return Err(WorkerError::InvalidThreadRead(
                "injected ambiguous native thread deletion".to_owned(),
            ));
        }
        Ok(())
    }

    async fn stop_workspace_execution(&self, workspace_id: &str) -> Result<(), WorkerError> {
        self.calls.lock().unwrap().push(WorkerCall::StopExecution {
            workspace_id: workspace_id.to_owned(),
        });
        Ok(())
    }

    async fn workspace_resources(
        &self,
        workspace_id: &str,
    ) -> Result<Option<WorkspaceRuntimeResources>, WorkerError> {
        self.calls.lock().unwrap().push(WorkerCall::Resources {
            workspace_id: workspace_id.to_owned(),
        });
        Ok(self.runtime_resources.lock().unwrap().clone())
    }

    async fn start_thread(
        &self,
        _workspace_id: &str,
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
        _workspace_id: &str,
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
        self.materialized_threads
            .lock()
            .unwrap()
            .insert(resumed_thread_id.clone());
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
        _workspace_id: &str,
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
        self.materialized_threads.lock().unwrap().insert(id.clone());
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
        _workspace_id: &str,
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
        self.materialized_threads
            .lock()
            .unwrap()
            .insert(thread_id.to_owned());
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
        Self::new_with_hooks(worker, HookRegistry::empty())
    }

    fn new_with_hooks(worker: FakeWorker, hooks: HookRegistry) -> Self {
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
            Arc::new(hooks),
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

    async fn attach(&self, workspace: &Workspace) -> Workspace {
        self.coordinator
            .attach_workspace(WorkspaceAttachParams {
                scope: RepositoryScope::repository(self.source.clone()),
                workspace: workspace.id.clone(),
            })
            .await
            .unwrap()
            .workspace
    }

    async fn create_and_materialize(&self, params: WorkspaceCreateParams) -> Workspace {
        let prepared = self
            .coordinator
            .create_workspace(params)
            .await
            .unwrap()
            .workspace;
        self.coordinator
            .materialize_workspace_thread(prepared)
            .await
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
            Arc::new(HookRegistry::empty()),
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
