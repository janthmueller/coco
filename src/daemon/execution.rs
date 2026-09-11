use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use serde_json::json;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Child;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tracing::{debug, warn};

use crate::codex::{CodexClient, CodexError};
use crate::coordinator::WorkerExecutionEnvironment;
use crate::domain::runtime::{
    WorkspaceResourceCapabilities, WorkspaceResourceControllerStatus, WorkspaceResourcePolicyError,
    WorkspaceResourcePolicySnapshot, WorkspaceRuntimeBackend, WorkspaceRuntimeResources,
    WorkspaceRuntimeState,
};

mod containment;
mod resources;

pub(in crate::daemon) use containment::ContainmentError;
pub(super) use containment::WorkspaceContainment;
use containment::{ActiveContainment, PendingContainment};

const EXEC_SERVER_START_TIMEOUT: Duration = Duration::from_secs(10);
const EXEC_SERVER_OUTPUT_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
const EXEC_SERVER_CONNECT_TIMEOUT_MS: u64 = 10_000;
const EXEC_SERVER_ENDPOINT_BYTES: usize = 256;
const EXEC_SERVER_STDERR_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WorkspaceExecutionMode {
    ExecServer,
    Shared,
}

impl WorkspaceExecutionMode {
    pub(super) fn from_env() -> Result<Self, WorkspaceExecutionError> {
        match std::env::var("COCO_WORKSPACE_EXECUTION") {
            Ok(value) => parse_mode(Some(&value)),
            Err(std::env::VarError::NotPresent) => parse_mode(None),
            Err(std::env::VarError::NotUnicode(_)) => Err(WorkspaceExecutionError::InvalidMode(
                "<non-UTF-8>".to_owned(),
            )),
        }
    }
}

#[derive(Debug, Error)]
pub(super) enum WorkspaceExecutionError {
    #[error("invalid COCO_WORKSPACE_EXECUTION value {0:?}; expected `exec-server` or `shared`")]
    InvalidMode(String),
    #[error("workspace execution requires an absolute working directory: {0}")]
    RelativeWorkingDirectory(PathBuf),
    #[error("could not start `codex exec-server`: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("`codex exec-server` did not publish an endpoint within 10 seconds")]
    StartupTimeout,
    #[error("could not read the `codex exec-server` endpoint: {0}")]
    EndpointRead(#[source] std::io::Error),
    #[error("`codex exec-server` published an invalid endpoint: {0}")]
    InvalidEndpoint(String),
    #[error("`codex exec-server` exited before publishing an endpoint: {stderr}")]
    EarlyExit { stderr: String },
    #[error(
        "the Codex App Server could not register the workspace executor; the tested codex-cli 0.154.0 environment API is required: {source}"
    )]
    Registration {
        #[source]
        source: CodexError,
    },
    #[error("the Codex App Server could not connect to the workspace executor: {source}")]
    Connection {
        #[source]
        source: CodexError,
    },
    #[error("could not inspect the workspace executor process: {0}")]
    ProcessInspection(#[source] std::io::Error),
    #[error("could not stop the workspace executor: {0}")]
    Shutdown(#[source] std::io::Error),
    #[error("workspace executor output collection failed: {0}")]
    OutputCollection(#[source] tokio::task::JoinError),
    #[error("workspace executor containment failed: {0}")]
    Containment(#[from] ContainmentError),
    #[error("invalid workspace resource policy: {0}")]
    InvalidResourcePolicy(#[source] WorkspaceResourcePolicyError),
    #[error("stale workspace resource policy revision {requested}; current revision is {current}")]
    StaleResourcePolicy { requested: u64, current: u64 },
    #[error("workspace resource policy revision {0} has conflicting contents")]
    ResourcePolicyRevisionConflict(u64),
}

#[derive(Clone)]
pub(super) struct WorkspaceExecutors {
    inner: Arc<WorkspaceExecutorsInner>,
}

impl std::fmt::Debug for WorkspaceExecutors {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WorkspaceExecutors")
            .finish_non_exhaustive()
    }
}

struct WorkspaceExecutorsInner {
    client: CodexClient,
    codex_binary: PathBuf,
    codex_home: Option<PathBuf>,
    containment: WorkspaceContainment,
    state: Mutex<WorkspaceExecutorState>,
}

struct WorkspaceExecutorState {
    runtimes: HashMap<String, ManagedExecServer>,
    policies: HashMap<String, WorkspaceResourcePolicySnapshot>,
}

impl WorkspaceExecutors {
    pub(super) fn new(
        client: CodexClient,
        codex_binary: PathBuf,
        codex_home: Option<PathBuf>,
        containment: WorkspaceContainment,
        policies: HashMap<String, WorkspaceResourcePolicySnapshot>,
    ) -> Self {
        Self {
            inner: Arc::new(WorkspaceExecutorsInner {
                client,
                codex_binary,
                codex_home,
                containment,
                state: Mutex::new(WorkspaceExecutorState {
                    runtimes: HashMap::new(),
                    policies,
                }),
            }),
        }
    }

    pub(super) async fn ensure(
        &self,
        workspace_id: &str,
        cwd: &Path,
    ) -> Result<WorkerExecutionEnvironment, WorkspaceExecutionError> {
        if !cwd.is_absolute() {
            return Err(WorkspaceExecutionError::RelativeWorkingDirectory(
                cwd.to_owned(),
            ));
        }

        let mut state = self.inner.state.lock().await;
        if let Some(runtime) = state.runtimes.get_mut(workspace_id) {
            match runtime.child.try_wait() {
                Ok(None) if runtime.cwd == cwd => return Ok(runtime.environment.clone()),
                Ok(None) | Ok(Some(_)) => {}
                Err(error) => return Err(WorkspaceExecutionError::ProcessInspection(error)),
            }
        }
        if let Some(runtime) = state.runtimes.remove(workspace_id) {
            runtime.shutdown().await?;
        }

        let policy = state
            .policies
            .get(workspace_id)
            .cloned()
            .unwrap_or_default();

        let environment = WorkerExecutionEnvironment {
            environment_id: environment_id(workspace_id),
            cwd: cwd.to_owned(),
            runtime_workspace_roots: vec![cwd.to_owned()],
        };
        let containment = self
            .inner
            .containment
            .prepare_with_policy(workspace_id, policy.clone())?;
        let runtime = ManagedExecServer::spawn(
            &self.inner.codex_binary,
            self.inner.codex_home.as_deref(),
            cwd,
            environment.clone(),
            containment,
            policy,
        )
        .await?;
        debug!(
            workspace_id,
            environment_id = %environment.environment_id,
            workspace_executor_pid = runtime.process_id,
            "registering workspace exec-server"
        );

        if let Err(source) = self
            .inner
            .client
            .request(
                "environment/add",
                json!({
                    "environmentId": environment.environment_id,
                    "execServerUrl": runtime.endpoint,
                    "connectTimeoutMs": EXEC_SERVER_CONNECT_TIMEOUT_MS,
                }),
            )
            .await
        {
            if let Err(error) = runtime.shutdown().await {
                warn!(%error, "could not clean up an unregistered workspace exec-server");
            }
            return Err(WorkspaceExecutionError::Registration { source });
        }
        debug!(
            workspace_id,
            environment_id = %environment.environment_id,
            "workspace exec-server registered"
        );
        if let Err(source) = self
            .inner
            .client
            .request(
                "environment/info",
                json!({"environmentId": environment.environment_id}),
            )
            .await
        {
            if let Err(error) = runtime.shutdown().await {
                warn!(%error, "could not clean up a disconnected workspace exec-server");
            }
            return Err(WorkspaceExecutionError::Connection { source });
        }
        debug!(
            workspace_id,
            environment_id = %environment.environment_id,
            "workspace exec-server ready"
        );

        state.runtimes.insert(workspace_id.to_owned(), runtime);
        Ok(environment)
    }

    pub(super) async fn close(&self) {
        let runtimes = {
            let mut state = self.inner.state.lock().await;
            state
                .runtimes
                .drain()
                .map(|(_, runtime)| runtime)
                .collect::<Vec<_>>()
        };
        for runtime in runtimes {
            if let Err(error) = runtime.shutdown().await {
                warn!(%error, "could not stop a workspace exec-server cleanly");
            }
        }
    }

    pub(super) async fn stop(&self, workspace_id: &str) -> Result<(), WorkspaceExecutionError> {
        let runtime = self.inner.state.lock().await.runtimes.remove(workspace_id);
        if let Some(runtime) = runtime {
            debug!(
                workspace_id,
                workspace_executor_pid = runtime.process_id,
                "stopping workspace exec-server"
            );
            runtime.shutdown().await?;
        }
        Ok(())
    }

    pub(super) async fn resources(
        &self,
        workspace_id: &str,
    ) -> Result<WorkspaceRuntimeResources, WorkspaceExecutionError> {
        let mut state = self.inner.state.lock().await;
        let Some(runtime) = state.runtimes.get_mut(workspace_id) else {
            return Ok(WorkspaceRuntimeResources {
                backend: WorkspaceRuntimeBackend::ExecServer,
                state: WorkspaceRuntimeState::Inactive,
                scope: self.inner.containment.resource_scope(),
                process_id: None,
                process_count: None,
                task_count: None,
                resident_memory_bytes: None,
                memory_current_bytes: None,
                cpu_percent: None,
                cpu_usage_usec: None,
                cgroup_unit: None,
                events: None,
                sampled_at_ms: None,
            });
        };
        runtime.resources()
    }

    pub(super) fn resource_capabilities(&self) -> WorkspaceResourceCapabilities {
        self.inner.containment.capabilities()
    }

    pub(super) async fn resource_policy_status(
        &self,
        workspace_id: &str,
    ) -> Result<WorkspaceResourceControllerStatus, WorkspaceExecutionError> {
        let mut state = self.inner.state.lock().await;
        let Some(runtime) = state.runtimes.get_mut(workspace_id) else {
            return Ok(WorkspaceResourceControllerStatus {
                capabilities: self.resource_capabilities(),
                runtime_state: WorkspaceRuntimeState::Inactive,
                applied_policy: None,
            });
        };
        let running = runtime
            .child
            .try_wait()
            .map_err(WorkspaceExecutionError::ProcessInspection)?
            .is_none();
        Ok(WorkspaceResourceControllerStatus {
            capabilities: self.resource_capabilities(),
            runtime_state: if running {
                WorkspaceRuntimeState::Running
            } else {
                WorkspaceRuntimeState::Exited
            },
            applied_policy: running.then(|| runtime.applied_policy.clone()),
        })
    }

    pub(super) async fn configure_resource_policy(
        &self,
        workspace_id: &str,
        snapshot: WorkspaceResourcePolicySnapshot,
    ) -> Result<WorkspaceResourceControllerStatus, WorkspaceExecutionError> {
        snapshot
            .policy
            .validate()
            .map_err(WorkspaceExecutionError::InvalidResourcePolicy)?;
        self.inner.containment.validate_policy(&snapshot.policy)?;

        let mut state = self.inner.state.lock().await;
        let current = state
            .policies
            .get(workspace_id)
            .cloned()
            .unwrap_or_default();
        if snapshot.revision < current.revision {
            return Err(WorkspaceExecutionError::StaleResourcePolicy {
                requested: snapshot.revision,
                current: current.revision,
            });
        }
        if snapshot.revision == current.revision {
            if snapshot.policy != current.policy {
                return Err(WorkspaceExecutionError::ResourcePolicyRevisionConflict(
                    snapshot.revision,
                ));
            }
            drop(state);
            return self.resource_policy_status(workspace_id).await;
        }

        let mut runtime_state = WorkspaceRuntimeState::Inactive;
        let mut applied_policy = None;
        if let Some(runtime) = state.runtimes.get_mut(workspace_id) {
            if runtime
                .child
                .try_wait()
                .map_err(WorkspaceExecutionError::ProcessInspection)?
                .is_none()
            {
                if !resource_policy_update_requires_restart(&runtime.applied_policy, &snapshot) {
                    runtime.apply_policy(&snapshot).await?;
                }
                runtime_state = WorkspaceRuntimeState::Running;
                applied_policy = Some(runtime.applied_policy.clone());
            } else {
                runtime_state = WorkspaceRuntimeState::Exited;
            }
        }
        state.policies.insert(workspace_id.to_owned(), snapshot);
        Ok(WorkspaceResourceControllerStatus {
            capabilities: self.resource_capabilities(),
            runtime_state,
            applied_policy,
        })
    }
}

pub(super) async fn initialize_containment(
    data_dir: &Path,
) -> Result<WorkspaceContainment, WorkspaceExecutionError> {
    WorkspaceContainment::initialize(data_dir)
        .await
        .map_err(Into::into)
}

struct ManagedExecServer {
    child: Child,
    process_id: u32,
    cwd: PathBuf,
    endpoint: String,
    environment: WorkerExecutionEnvironment,
    containment: ActiveContainment,
    stderr_tail: Arc<Mutex<ByteTail>>,
    stderr_task: JoinHandle<()>,
    cpu_counters: Option<resources::CpuCounters>,
    cgroup_cpu_counters: Option<resources::CgroupCpuCounters>,
    applied_policy: WorkspaceResourcePolicySnapshot,
}

impl ManagedExecServer {
    async fn spawn(
        codex_binary: &Path,
        codex_home: Option<&Path>,
        cwd: &Path,
        environment: WorkerExecutionEnvironment,
        containment: PendingContainment,
        policy: WorkspaceResourcePolicySnapshot,
    ) -> Result<Self, WorkspaceExecutionError> {
        let mut command = containment.command(
            codex_binary,
            &["exec-server", "--listen", "ws://127.0.0.1:0"],
        );
        command
            .current_dir(cwd)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        if let Some(codex_home) = codex_home {
            command.env("CODEX_HOME", codex_home);
        }
        let mut child = command.spawn().map_err(WorkspaceExecutionError::Spawn)?;
        let Some(process_id) = child.id() else {
            abort_pending_start(&mut child, &containment).await?;
            return Err(WorkspaceExecutionError::InvalidEndpoint(
                "the child process did not expose a process ID".to_owned(),
            ));
        };
        let Some(mut stdout) = child.stdout.take() else {
            abort_pending_start(&mut child, &containment).await?;
            return Err(WorkspaceExecutionError::InvalidEndpoint(
                "the child process did not expose stdout".to_owned(),
            ));
        };
        let Some(stderr) = child.stderr.take() else {
            abort_pending_start(&mut child, &containment).await?;
            return Err(WorkspaceExecutionError::InvalidEndpoint(
                "the child process did not expose stderr".to_owned(),
            ));
        };
        let stderr_tail = Arc::new(Mutex::new(ByteTail::new(EXEC_SERVER_STDERR_BYTES)));
        let stderr_task = tokio::spawn(collect_stderr(stderr, Arc::clone(&stderr_tail)));

        let endpoint = match timeout(
            EXEC_SERVER_START_TIMEOUT,
            read_bounded_line(&mut stdout, EXEC_SERVER_ENDPOINT_BYTES),
        )
        .await
        {
            Ok(Ok(Some(endpoint))) => endpoint,
            Ok(Ok(None)) => {
                abort_started_process(&mut child, &containment, stderr_task, process_id).await?;
                return Err(WorkspaceExecutionError::EarlyExit {
                    stderr: stderr_tail.lock().await.display(),
                });
            }
            Ok(Err(error)) => {
                abort_started_process(&mut child, &containment, stderr_task, process_id).await?;
                return Err(WorkspaceExecutionError::EndpointRead(error));
            }
            Err(_) => {
                abort_started_process(&mut child, &containment, stderr_task, process_id).await?;
                return Err(WorkspaceExecutionError::StartupTimeout);
            }
        };
        let endpoint = match validate_endpoint(endpoint) {
            Ok(endpoint) => endpoint,
            Err(error) => {
                abort_started_process(&mut child, &containment, stderr_task, process_id).await?;
                return Err(error);
            }
        };
        let active_containment = match containment.clone().activate(process_id).await {
            Ok(active) => active,
            Err(error) => {
                abort_started_process(&mut child, &containment, stderr_task, process_id).await?;
                return Err(WorkspaceExecutionError::Containment(error));
            }
        };

        Ok(Self {
            child,
            process_id,
            cwd: cwd.to_owned(),
            endpoint,
            environment,
            containment: active_containment,
            stderr_tail,
            stderr_task,
            cpu_counters: None,
            cgroup_cpu_counters: None,
            applied_policy: policy,
        })
    }

    async fn apply_policy(
        &mut self,
        snapshot: &WorkspaceResourcePolicySnapshot,
    ) -> Result<(), WorkspaceExecutionError> {
        self.containment.apply_policy(&snapshot.policy).await?;
        self.applied_policy = snapshot.clone();
        Ok(())
    }

    fn resources(&mut self) -> Result<WorkspaceRuntimeResources, WorkspaceExecutionError> {
        let sampled_at_ms = Utc::now().timestamp_millis();
        if self
            .child
            .try_wait()
            .map_err(WorkspaceExecutionError::ProcessInspection)?
            .is_some()
        {
            return Ok(self.resource_snapshot(WorkspaceRuntimeState::Exited, None, sampled_at_ms));
        }
        if let Some(cgroup_path) = self.containment.cgroup_path() {
            return match resources::inspect_cgroup(cgroup_path, self.cgroup_cpu_counters) {
                Ok(Some(usage)) => {
                    self.cgroup_cpu_counters = usage.counters;
                    Ok(WorkspaceRuntimeResources {
                        backend: WorkspaceRuntimeBackend::ExecServer,
                        state: WorkspaceRuntimeState::Running,
                        scope: self.containment.resource_scope(),
                        process_id: Some(self.process_id),
                        process_count: usage.process_count,
                        task_count: usage.task_count,
                        resident_memory_bytes: None,
                        memory_current_bytes: usage.memory_current_bytes,
                        cpu_percent: usage.cpu_percent,
                        cpu_usage_usec: usage.cpu_usage_usec,
                        cgroup_unit: self.containment.unit().map(str::to_owned),
                        events: usage.events,
                        sampled_at_ms: Some(sampled_at_ms),
                    })
                }
                Ok(None) => {
                    Ok(self.resource_snapshot(WorkspaceRuntimeState::Exited, None, sampled_at_ms))
                }
                Err(error) => {
                    warn!(%error, workspace_executor_pid = self.process_id, "workspace cgroup resource observation failed");
                    Ok(self.resource_snapshot(
                        WorkspaceRuntimeState::Running,
                        Some(self.process_id),
                        sampled_at_ms,
                    ))
                }
            };
        }

        match resources::inspect(self.process_id, self.cpu_counters) {
            Ok(Some(usage)) => {
                self.cpu_counters = usage.counters;
                Ok(WorkspaceRuntimeResources {
                    backend: WorkspaceRuntimeBackend::ExecServer,
                    state: WorkspaceRuntimeState::Running,
                    scope: self.containment.resource_scope(),
                    process_id: Some(self.process_id),
                    process_count: usage.process_count,
                    task_count: None,
                    resident_memory_bytes: usage.resident_memory_bytes,
                    memory_current_bytes: None,
                    cpu_percent: usage.cpu_percent,
                    cpu_usage_usec: None,
                    cgroup_unit: None,
                    events: None,
                    sampled_at_ms: Some(sampled_at_ms),
                })
            }
            Ok(None) => {
                Ok(self.resource_snapshot(WorkspaceRuntimeState::Exited, None, sampled_at_ms))
            }
            Err(error) => {
                warn!(%error, workspace_executor_pid = self.process_id, "workspace resource observation failed");
                Ok(self.resource_snapshot(
                    WorkspaceRuntimeState::Running,
                    Some(self.process_id),
                    sampled_at_ms,
                ))
            }
        }
    }

    fn resource_snapshot(
        &self,
        state: WorkspaceRuntimeState,
        process_id: Option<u32>,
        sampled_at_ms: i64,
    ) -> WorkspaceRuntimeResources {
        WorkspaceRuntimeResources {
            backend: WorkspaceRuntimeBackend::ExecServer,
            state,
            scope: self.containment.resource_scope(),
            process_id,
            process_count: None,
            task_count: None,
            resident_memory_bytes: None,
            memory_current_bytes: None,
            cpu_percent: None,
            cpu_usage_usec: None,
            cgroup_unit: self.containment.unit().map(str::to_owned),
            events: None,
            sampled_at_ms: Some(sampled_at_ms),
        }
    }

    async fn shutdown(mut self) -> Result<(), WorkspaceExecutionError> {
        let mut containment_error = self.containment.stop().await.err();
        let mut shutdown_error = None;
        if self
            .child
            .try_wait()
            .map_err(WorkspaceExecutionError::ProcessInspection)?
            .is_none()
        {
            shutdown_error = self.child.kill().await.err();
        }
        // A failed scope stop must not skip the direct-process fallback, and
        // killing the direct process must not replace a second attempt to
        // stop any descendants that may still belong to the scope.
        if containment_error.is_some() {
            containment_error = self.containment.stop().await.err();
        }
        finish_stderr_collection(self.stderr_task, self.process_id).await?;
        let stderr = self.stderr_tail.lock().await.display();
        if !stderr.is_empty() {
            tracing::debug!(%stderr, "workspace exec-server stopped");
        }
        if let Some(error) = containment_error {
            return Err(WorkspaceExecutionError::Containment(error));
        }
        if let Some(error) = shutdown_error
            && self
                .child
                .try_wait()
                .map_err(WorkspaceExecutionError::ProcessInspection)?
                .is_none()
        {
            return Err(WorkspaceExecutionError::Shutdown(error));
        }
        Ok(())
    }
}

async fn abort_started_process(
    child: &mut Child,
    containment: &PendingContainment,
    stderr_task: JoinHandle<()>,
    process_id: u32,
) -> Result<(), WorkspaceExecutionError> {
    let cleanup = abort_pending_start(child, containment).await;
    if let Err(error) = finish_stderr_collection(stderr_task, process_id).await {
        warn!(%error, workspace_executor_pid = process_id, "workspace exec-server output collection failed during startup cleanup");
    }
    cleanup
}

async fn finish_stderr_collection(
    mut stderr_task: JoinHandle<()>,
    process_id: u32,
) -> Result<(), WorkspaceExecutionError> {
    match timeout(EXEC_SERVER_OUTPUT_SHUTDOWN_TIMEOUT, &mut stderr_task).await {
        Ok(result) => result.map_err(WorkspaceExecutionError::OutputCollection),
        Err(_) => {
            stderr_task.abort();
            let _ = stderr_task.await;
            warn!(
                workspace_executor_pid = process_id,
                "workspace exec-server stderr remained open after shutdown"
            );
            Ok(())
        }
    }
}

async fn abort_pending_start(
    child: &mut Child,
    containment: &PendingContainment,
) -> Result<(), WorkspaceExecutionError> {
    let mut cleanup_error = containment.cleanup().await.err();
    let mut shutdown_error = None;
    if child
        .try_wait()
        .map_err(WorkspaceExecutionError::ProcessInspection)?
        .is_none()
    {
        shutdown_error = child.kill().await.err();
    }
    if cleanup_error.is_some() {
        cleanup_error = containment.cleanup().await.err();
    }
    if let Some(error) = cleanup_error {
        return Err(WorkspaceExecutionError::Containment(error));
    }
    if let Some(error) = shutdown_error
        && child
            .try_wait()
            .map_err(WorkspaceExecutionError::ProcessInspection)?
            .is_none()
    {
        return Err(WorkspaceExecutionError::Shutdown(error));
    }
    Ok(())
}

fn environment_id(workspace_id: &str) -> String {
    let digest = Sha256::digest(workspace_id.as_bytes());
    format!("coco-{}", hex::encode(&digest[..16]))
}

fn resource_policy_update_requires_restart(
    applied: &WorkspaceResourcePolicySnapshot,
    desired: &WorkspaceResourcePolicySnapshot,
) -> bool {
    applied.policy.cpu_max_millicores.is_some() && desired.policy.cpu_max_millicores.is_none()
}

fn parse_mode(value: Option<&str>) -> Result<WorkspaceExecutionMode, WorkspaceExecutionError> {
    match value {
        None | Some("exec-server") => Ok(WorkspaceExecutionMode::ExecServer),
        Some("shared") => Ok(WorkspaceExecutionMode::Shared),
        Some(value) => Err(WorkspaceExecutionError::InvalidMode(value.to_owned())),
    }
}

fn validate_endpoint(endpoint: String) -> Result<String, WorkspaceExecutionError> {
    let endpoint = endpoint.trim().to_owned();
    let address = endpoint
        .strip_prefix("ws://")
        .and_then(|address| address.parse::<SocketAddr>().ok());
    match address {
        Some(SocketAddr::V4(address))
            if address.ip() == &Ipv4Addr::LOCALHOST && address.port() != 0 =>
        {
            Ok(endpoint)
        }
        Some(SocketAddr::V6(address))
            if IpAddr::V6(*address.ip()).is_loopback() && address.port() != 0 =>
        {
            Ok(endpoint)
        }
        _ => Err(WorkspaceExecutionError::InvalidEndpoint(endpoint)),
    }
}

async fn read_bounded_line<R>(
    reader: &mut R,
    limit: usize,
) -> Result<Option<String>, std::io::Error>
where
    R: AsyncRead + Unpin,
{
    let mut bytes = Vec::with_capacity(limit.min(128));
    let mut byte = [0_u8; 1];
    loop {
        let read = reader.read(&mut byte).await?;
        if read == 0 {
            if bytes.is_empty() {
                return Ok(None);
            }
            break;
        }
        if byte[0] == b'\n' {
            break;
        }
        if byte[0] != b'\r' {
            bytes.push(byte[0]);
        }
        if bytes.len() > limit {
            return Ok(Some("<endpoint exceeded 256 bytes>".to_owned()));
        }
    }
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

async fn collect_stderr<R>(mut stderr: R, tail: Arc<Mutex<ByteTail>>)
where
    R: AsyncRead + Unpin,
{
    let mut buffer = [0_u8; 4096];
    loop {
        match stderr.read(&mut buffer).await {
            Ok(0) => return,
            Ok(read) => tail.lock().await.push(&buffer[..read]),
            Err(error) => {
                tail.lock()
                    .await
                    .push(format!("\n[stderr read failed: {error}]").as_bytes());
                return;
            }
        }
    }
}

struct ByteTail {
    bytes: Vec<u8>,
    limit: usize,
}

impl ByteTail {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
        }
    }

    fn push(&mut self, chunk: &[u8]) {
        if chunk.len() >= self.limit {
            self.bytes.clear();
            self.bytes
                .extend_from_slice(&chunk[chunk.len() - self.limit..]);
            return;
        }
        let overflow = self
            .bytes
            .len()
            .saturating_add(chunk.len())
            .saturating_sub(self.limit);
        if overflow > 0 {
            self.bytes.drain(..overflow);
        }
        self.bytes.extend_from_slice(chunk);
    }

    fn display(&self) -> String {
        String::from_utf8_lossy(&self.bytes).trim().to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_defaults_to_exec_server_and_rejects_unknown_values() {
        assert_eq!(
            parse_mode(None).unwrap(),
            WorkspaceExecutionMode::ExecServer
        );
        assert_eq!(
            parse_mode(Some("shared")).unwrap(),
            WorkspaceExecutionMode::Shared
        );
        assert!(parse_mode(Some("auto")).is_err());
    }

    #[test]
    fn environment_ids_are_stable_and_do_not_expose_workspace_ids() {
        let first = environment_id("workspace/private-name");
        assert_eq!(first, environment_id("workspace/private-name"));
        assert_ne!(first, environment_id("workspace/other"));
        assert!(!first.contains("private-name"));
    }

    #[test]
    fn accepts_only_nonzero_loopback_websocket_endpoints() {
        assert_eq!(
            validate_endpoint("ws://127.0.0.1:43123".to_owned()).unwrap(),
            "ws://127.0.0.1:43123"
        );
        for endpoint in [
            "http://127.0.0.1:43123",
            "ws://127.0.0.1:0",
            "ws://0.0.0.0:43123",
            "ws://192.0.2.1:43123",
        ] {
            assert!(validate_endpoint(endpoint.to_owned()).is_err());
        }
    }

    #[test]
    fn removing_an_applied_cpu_cap_is_staged_for_the_next_runtime() {
        let applied = WorkspaceResourcePolicySnapshot {
            revision: 3,
            policy: crate::domain::runtime::WorkspaceResourcePolicy {
                cpu_max_millicores: Some(750),
                ..crate::domain::runtime::WorkspaceResourcePolicy::default()
            },
        };
        let removed = WorkspaceResourcePolicySnapshot {
            revision: 4,
            policy: crate::domain::runtime::WorkspaceResourcePolicy::default(),
        };
        let changed = WorkspaceResourcePolicySnapshot {
            revision: 4,
            policy: crate::domain::runtime::WorkspaceResourcePolicy {
                cpu_max_millicores: Some(1_500),
                ..crate::domain::runtime::WorkspaceResourcePolicy::default()
            },
        };

        assert!(resource_policy_update_requires_restart(&applied, &removed));
        assert!(!resource_policy_update_requires_restart(&applied, &changed));
    }
}
