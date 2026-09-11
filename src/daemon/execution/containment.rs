#[cfg(target_os = "linux")]
use std::ffi::OsStr;
#[cfg(target_os = "linux")]
use std::path::Component;
use std::path::{Path, PathBuf};
#[cfg(target_os = "linux")]
use std::process::{ExitStatus, Output};
#[cfg(target_os = "linux")]
use std::time::Duration;

#[cfg(target_os = "linux")]
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::process::Command;
#[cfg(target_os = "linux")]
use tokio::time::timeout;
#[cfg(target_os = "linux")]
use tracing::{debug, warn};

use crate::domain::runtime::{
    WorkspaceResourceCapabilities, WorkspaceResourceControllerBackend, WorkspaceResourcePolicy,
    WorkspaceResourcePolicySnapshot, WorkspaceResourceScope,
};

#[cfg(target_os = "linux")]
const SYSTEMD_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(target_os = "linux")]
const SYSTEMD_CONTROL_TIMEOUT: Duration = Duration::from_secs(15);
#[cfg(target_os = "linux")]
const SYSTEMD_OUTPUT_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContainmentMode {
    Auto,
    Systemd,
    ProcessTree,
}

impl ContainmentMode {
    fn from_env() -> Result<Self, ContainmentError> {
        match std::env::var("COCO_WORKSPACE_CONTAINMENT") {
            Ok(value) => parse_mode(Some(&value)),
            Err(std::env::VarError::NotPresent) => parse_mode(None),
            Err(std::env::VarError::NotUnicode(_)) => {
                Err(ContainmentError::InvalidMode("<non-UTF-8>".to_owned()))
            }
        }
    }
}

#[derive(Debug, Error)]
pub(in crate::daemon) enum ContainmentError {
    #[error(
        "invalid COCO_WORKSPACE_CONTAINMENT value {0:?}; expected `auto`, `systemd`, or `process-tree`"
    )]
    InvalidMode(String),
    #[error("systemd cgroup-v2 workspace containment is unavailable: {0}")]
    Unavailable(String),
    #[error("could not inspect CoCo's canonical data directory {path}: {source}")]
    DataDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("could not run {program} while {operation}: {source}")]
    CommandSpawn {
        program: &'static str,
        operation: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("timed out while {0}")]
    CommandTimeout(&'static str),
    #[error("systemd failed while {operation}: {message}")]
    CommandFailed {
        operation: &'static str,
        message: String,
    },
    #[error("systemd reported an invalid workspace control group: {0}")]
    InvalidControlGroup(String),
    #[error("could not inspect workspace control group {path}: {source}")]
    ControlGroupIo {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("the selected workspace resource controller cannot enforce: {fields}", fields = .fields.join(", "))]
    UnsupportedPolicy { fields: Vec<&'static str> },
    #[error(
        "refusing to lower the workspace memory maximum to {requested} bytes while it currently uses {current} bytes"
    )]
    MemoryMaximumBelowCurrent { requested: u64, current: u64 },
    #[error("removing an active CPU maximum requires restarting the workspace runtime")]
    CpuMaximumRemovalRequiresRestart,
    #[error(
        "workspace resource policy was not applied for {field}: expected {expected}, observed {actual}"
    )]
    PolicyVerification {
        field: &'static str,
        expected: String,
        actual: String,
    },
}

#[derive(Debug, Clone)]
pub(in crate::daemon) struct WorkspaceContainment {
    backend: ContainmentBackend,
}

#[derive(Debug, Clone)]
enum ContainmentBackend {
    ProcessTree,
    #[cfg(target_os = "linux")]
    Systemd(SystemdBackend),
}

#[derive(Debug, Clone)]
pub(super) struct PendingContainment {
    backend: PendingBackend,
}

#[derive(Debug, Clone)]
enum PendingBackend {
    ProcessTree,
    #[cfg(target_os = "linux")]
    Systemd(Box<PendingSystemd>),
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone)]
struct PendingSystemd {
    backend: SystemdBackend,
    unit: String,
    policy: WorkspaceResourcePolicySnapshot,
}

#[derive(Debug, Clone)]
pub(super) enum ActiveContainment {
    ProcessTree,
    #[cfg(target_os = "linux")]
    SystemdScope {
        backend: SystemdBackend,
        unit: String,
        cgroup_path: PathBuf,
    },
}

impl WorkspaceContainment {
    pub(super) async fn initialize(data_dir: &Path) -> Result<Self, ContainmentError> {
        Self::initialize_with_mode(ContainmentMode::from_env()?, data_dir).await
    }

    async fn initialize_with_mode(
        mode: ContainmentMode,
        data_dir: &Path,
    ) -> Result<Self, ContainmentError> {
        if mode == ContainmentMode::ProcessTree {
            return Ok(Self::process_tree());
        }

        #[cfg(target_os = "linux")]
        {
            match SystemdBackend::detect(data_dir).await {
                Ok(backend) => {
                    backend.cleanup_stale_scopes().await?;
                    debug!(
                        workspace_slice = %backend.workspace_slice,
                        "workspace cgroup-v2 containment is available"
                    );
                    Ok(Self {
                        backend: ContainmentBackend::Systemd(backend),
                    })
                }
                Err(error) if mode == ContainmentMode::Auto => {
                    warn!(%error, "workspace cgroup-v2 containment is unavailable; using process-tree observation");
                    Ok(Self::process_tree())
                }
                Err(error) => Err(error),
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = data_dir;
            if mode == ContainmentMode::Systemd {
                return Err(ContainmentError::Unavailable(
                    "the systemd backend is supported only on Linux".to_owned(),
                ));
            }
            Ok(Self::process_tree())
        }
    }

    fn process_tree() -> Self {
        Self {
            backend: ContainmentBackend::ProcessTree,
        }
    }

    #[cfg(test)]
    pub(super) fn prepare(&self, workspace_id: &str) -> PendingContainment {
        self.prepare_with_policy(workspace_id, WorkspaceResourcePolicySnapshot::default())
            .expect("an empty resource policy is supported by every containment backend")
    }

    pub(super) fn prepare_with_policy(
        &self,
        workspace_id: &str,
        policy: WorkspaceResourcePolicySnapshot,
    ) -> Result<PendingContainment, ContainmentError> {
        self.validate_policy(&policy.policy)?;
        let backend = match &self.backend {
            ContainmentBackend::ProcessTree => PendingBackend::ProcessTree,
            #[cfg(target_os = "linux")]
            ContainmentBackend::Systemd(backend) => {
                PendingBackend::Systemd(Box::new(PendingSystemd {
                    backend: backend.clone(),
                    unit: backend.workspace_unit(workspace_id),
                    policy,
                }))
            }
        };
        Ok(PendingContainment { backend })
    }

    pub(super) const fn resource_scope(&self) -> WorkspaceResourceScope {
        match self.backend {
            ContainmentBackend::ProcessTree => {
                if cfg!(target_os = "linux") {
                    WorkspaceResourceScope::ProcessTree
                } else {
                    WorkspaceResourceScope::RootProcess
                }
            }
            #[cfg(target_os = "linux")]
            ContainmentBackend::Systemd(_) => WorkspaceResourceScope::CgroupV2,
        }
    }

    pub(super) const fn capabilities(&self) -> WorkspaceResourceCapabilities {
        match self.backend {
            ContainmentBackend::ProcessTree => WorkspaceResourceCapabilities::unavailable(),
            #[cfg(target_os = "linux")]
            ContainmentBackend::Systemd(_) => WorkspaceResourceCapabilities {
                backend: WorkspaceResourceControllerBackend::SystemdCgroupV2,
                dynamic_updates: true,
                memory_high: true,
                memory_max: true,
                cpu_max: true,
                cpu_weight: true,
                tasks_max: true,
            },
        }
    }

    pub(super) fn validate_policy(
        &self,
        policy: &WorkspaceResourcePolicy,
    ) -> Result<(), ContainmentError> {
        let unsupported = self.capabilities().unsupported_fields(policy);
        if unsupported.is_empty() {
            Ok(())
        } else {
            Err(ContainmentError::UnsupportedPolicy {
                fields: unsupported,
            })
        }
    }
}

impl PendingContainment {
    pub(super) fn command(&self, executable: &Path, arguments: &[&str]) -> Command {
        match &self.backend {
            PendingBackend::ProcessTree => {
                let mut command = Command::new(executable);
                command.args(arguments).kill_on_drop(true);
                command
            }
            #[cfg(target_os = "linux")]
            PendingBackend::Systemd(pending) => {
                let PendingSystemd {
                    backend,
                    unit,
                    policy,
                } = pending.as_ref();
                let mut command = Command::new("systemd-run");
                command
                    .args([
                        "--user",
                        "--scope",
                        "--quiet",
                        "--collect",
                        "--expand-environment=no",
                    ])
                    .arg(format!("--unit={unit}"))
                    .arg(format!("--slice={}", backend.workspace_slice))
                    .args([
                        "--property=CPUAccounting=yes",
                        "--property=MemoryAccounting=yes",
                        "--property=TasksAccounting=yes",
                        "--property=KillMode=control-group",
                        "--property=TimeoutStopSec=5s",
                    ]);
                for property in configured_policy_properties(&policy.policy) {
                    command.arg(format!("--property={property}"));
                }
                command
                    .arg("--")
                    .arg(executable)
                    .args(arguments)
                    // Killing only this supervisor would not stop its systemd
                    // scope. Explicit cleanup and next-generation stale-scope
                    // recovery own that lifecycle instead.
                    .kill_on_drop(false);
                command
            }
        }
    }

    pub(super) async fn activate(
        self,
        supervisor_pid: u32,
    ) -> Result<ActiveContainment, ContainmentError> {
        match self.backend {
            PendingBackend::ProcessTree => Ok(ActiveContainment::ProcessTree),
            #[cfg(target_os = "linux")]
            PendingBackend::Systemd(pending) => {
                let PendingSystemd {
                    backend,
                    unit,
                    policy,
                } = *pending;
                let cgroup_path = backend.control_group(&unit, supervisor_pid).await?;
                backend.verify_configured_policy(&cgroup_path, &policy.policy)?;
                Ok(ActiveContainment::SystemdScope {
                    backend,
                    unit,
                    cgroup_path,
                })
            }
        }
    }

    pub(super) async fn cleanup(&self) -> Result<(), ContainmentError> {
        match &self.backend {
            PendingBackend::ProcessTree => Ok(()),
            #[cfg(target_os = "linux")]
            PendingBackend::Systemd(pending) => pending.backend.stop_unit(&pending.unit).await,
        }
    }
}

impl ActiveContainment {
    pub(super) const fn resource_scope(&self) -> WorkspaceResourceScope {
        match self {
            Self::ProcessTree => {
                if cfg!(target_os = "linux") {
                    WorkspaceResourceScope::ProcessTree
                } else {
                    WorkspaceResourceScope::RootProcess
                }
            }
            #[cfg(target_os = "linux")]
            Self::SystemdScope { .. } => WorkspaceResourceScope::CgroupV2,
        }
    }

    pub(super) fn cgroup_path(&self) -> Option<&Path> {
        match self {
            Self::ProcessTree => None,
            #[cfg(target_os = "linux")]
            Self::SystemdScope { cgroup_path, .. } => Some(cgroup_path),
        }
    }

    pub(super) fn unit(&self) -> Option<&str> {
        match self {
            Self::ProcessTree => None,
            #[cfg(target_os = "linux")]
            Self::SystemdScope { unit, .. } => Some(unit),
        }
    }

    pub(super) async fn stop(&self) -> Result<(), ContainmentError> {
        match self {
            Self::ProcessTree => Ok(()),
            #[cfg(target_os = "linux")]
            Self::SystemdScope { backend, unit, .. } => backend.stop_unit(unit).await,
        }
    }

    pub(super) async fn apply_policy(
        &self,
        policy: &WorkspaceResourcePolicy,
    ) -> Result<(), ContainmentError> {
        match self {
            Self::ProcessTree if policy.is_empty() => Ok(()),
            Self::ProcessTree => Err(ContainmentError::UnsupportedPolicy {
                fields: WorkspaceResourceCapabilities::unavailable().unsupported_fields(policy),
            }),
            #[cfg(target_os = "linux")]
            Self::SystemdScope {
                backend,
                unit,
                cgroup_path,
            } => {
                if policy.cpu_max_millicores.is_none() && active_cpu_max_is_limited(cgroup_path)? {
                    return Err(ContainmentError::CpuMaximumRemovalRequiresRestart);
                }
                if let Some(requested) = policy.memory_max_bytes {
                    let current = read_cgroup_u64(&cgroup_path.join("memory.current"))?;
                    if requested < current {
                        return Err(ContainmentError::MemoryMaximumBelowCurrent {
                            requested,
                            current,
                        });
                    }
                }
                backend.set_unit_policy(unit, policy).await?;
                backend.verify_complete_policy(cgroup_path, policy)
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn configured_policy_properties(policy: &WorkspaceResourcePolicy) -> Vec<String> {
    let mut properties = Vec::new();
    if let Some(value) = policy.memory_high_bytes {
        properties.push(format!("MemoryHigh={value}"));
    }
    if let Some(value) = policy.memory_max_bytes {
        properties.push(format!("MemoryMax={value}"));
    }
    if let Some(value) = policy.cpu_max_millicores {
        properties.push(format!("CPUQuota={}", cpu_quota_percent(value)));
    }
    if let Some(value) = policy.cpu_weight {
        properties.push(format!("CPUWeight={value}"));
    }
    if let Some(value) = policy.tasks_max {
        properties.push(format!("TasksMax={value}"));
    }
    properties
}

#[cfg(target_os = "linux")]
fn complete_policy_properties(policy: &WorkspaceResourcePolicy) -> Vec<String> {
    let mut configured = configured_policy_properties(policy);
    for (configured_value, reset) in [
        (policy.memory_high_bytes.is_some(), "MemoryHigh="),
        (policy.memory_max_bytes.is_some(), "MemoryMax="),
        (policy.cpu_max_millicores.is_some(), "CPUQuota="),
        (policy.cpu_weight.is_some(), "CPUWeight=100"),
        (policy.tasks_max.is_some(), "TasksMax="),
    ] {
        if !configured_value {
            configured.push(reset.to_owned());
        }
    }
    configured
}

#[cfg(target_os = "linux")]
fn cpu_quota_percent(millicores: u32) -> String {
    let whole = millicores / 10;
    let tenths = millicores % 10;
    if tenths == 0 {
        format!("{whole}%")
    } else {
        format!("{whole}.{tenths}%")
    }
}

fn parse_mode(value: Option<&str>) -> Result<ContainmentMode, ContainmentError> {
    match value {
        None | Some("auto") => Ok(ContainmentMode::Auto),
        Some("systemd") => Ok(ContainmentMode::Systemd),
        Some("process-tree") => Ok(ContainmentMode::ProcessTree),
        Some(value) => Err(ContainmentError::InvalidMode(value.to_owned())),
    }
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone)]
pub(super) struct SystemdBackend {
    cgroup_mount: PathBuf,
    instance: String,
    instance_slice: String,
    workspace_slice: String,
    scope_prefix: String,
}

#[cfg(target_os = "linux")]
impl SystemdBackend {
    async fn detect(data_dir: &Path) -> Result<Self, ContainmentError> {
        let mountinfo = std::fs::read_to_string("/proc/self/mountinfo").map_err(|source| {
            ContainmentError::Unavailable(format!("could not read /proc/self/mountinfo: {source}"))
        })?;
        let cgroup_mount = find_cgroup2_mount(&mountinfo).ok_or_else(|| {
            ContainmentError::Unavailable("no unified cgroup-v2 mount was found".to_owned())
        })?;
        let controllers_path = cgroup_mount.join("cgroup.controllers");
        let controllers = std::fs::read_to_string(&controllers_path).map_err(|source| {
            ContainmentError::Unavailable(format!(
                "could not read {}: {source}",
                controllers_path.display()
            ))
        })?;
        for required in ["cpu", "memory", "pids"] {
            if !controllers
                .split_whitespace()
                .any(|value| value == required)
            {
                return Err(ContainmentError::Unavailable(format!(
                    "the unified hierarchy does not expose the {required} controller"
                )));
            }
        }

        require_success(
            run_command(
                "systemd-run",
                &["--expand-environment=no", "--version"],
                "checking the systemd-run installation",
                SYSTEMD_PROBE_TIMEOUT,
            )
            .await?,
            "checking the systemd-run installation",
        )?;
        require_success(
            run_command(
                "systemctl",
                &["--user", "show", "--property=Version", "--value"],
                "contacting the systemd user manager",
                SYSTEMD_PROBE_TIMEOUT,
            )
            .await?,
            "contacting the systemd user manager",
        )?;

        let canonical =
            std::fs::canonicalize(data_dir).map_err(|source| ContainmentError::DataDirectory {
                path: data_dir.to_owned(),
                source,
            })?;
        let digest = Sha256::digest(canonical.as_os_str().as_encoded_bytes());
        let instance = hex::encode(&digest[..16]);
        Ok(Self {
            cgroup_mount,
            instance_slice: format!("app-coco{instance}.slice"),
            workspace_slice: format!("app-coco{instance}-workspaces.slice"),
            scope_prefix: format!("coco{instance}-workspace-"),
            instance,
        })
    }

    fn workspace_unit(&self, workspace_id: &str) -> String {
        let digest = Sha256::digest(workspace_id.as_bytes());
        format!("{}{}.scope", self.scope_prefix, hex::encode(&digest[..16]))
    }

    async fn cleanup_stale_scopes(&self) -> Result<(), ContainmentError> {
        let pattern = format!("{}*.scope", self.scope_prefix);
        let output = run_command(
            "systemctl",
            &[
                "--user",
                "--no-pager",
                "--no-legend",
                "--plain",
                "list-units",
                "--all",
                "--type=scope",
                &pattern,
            ],
            "listing stale workspace scopes",
            SYSTEMD_CONTROL_TIMEOUT,
        )
        .await?;
        let output = require_success(output, "listing stale workspace scopes")?;
        for unit in parse_scope_units(&output.stdout, &self.scope_prefix) {
            debug!(%unit, instance = %self.instance, "stopping stale workspace scope");
            self.stop_unit(&unit).await?;
        }
        Ok(())
    }

    async fn control_group(
        &self,
        unit: &str,
        supervisor_pid: u32,
    ) -> Result<PathBuf, ContainmentError> {
        let output = run_command(
            "systemctl",
            &[
                "--user",
                "--no-pager",
                "show",
                "--property=ControlGroup",
                "--value",
                unit,
            ],
            "resolving a workspace scope",
            SYSTEMD_PROBE_TIMEOUT,
        )
        .await?;
        let output = require_success(output, "resolving a workspace scope")?;
        let control_group = std::str::from_utf8(&output.stdout)
            .ok()
            .map(str::trim)
            .filter(|value| value.starts_with('/') && value.len() > 1)
            .ok_or_else(|| ContainmentError::InvalidControlGroup(bounded_text(&output.stdout)))?;
        validate_scope_control_group(
            control_group,
            &self.instance_slice,
            &self.workspace_slice,
            unit,
        )?;

        let supervisor_membership = std::fs::read_to_string(format!(
            "/proc/{supervisor_pid}/cgroup"
        ))
        .map_err(|source| ContainmentError::ControlGroupIo {
            path: PathBuf::from(format!("/proc/{supervisor_pid}/cgroup")),
            source,
        })?;
        let supervisor_group =
            parse_unified_membership(&supervisor_membership).ok_or_else(|| {
                ContainmentError::InvalidControlGroup(format!(
                    "supervisor {supervisor_pid} has no unified cgroup membership"
                ))
            })?;
        if supervisor_group != control_group {
            return Err(ContainmentError::InvalidControlGroup(format!(
                "scope {unit} resolved to {control_group}, but supervisor {supervisor_pid} belongs to {supervisor_group}"
            )));
        }

        let path = self
            .cgroup_mount
            .join(control_group.trim_start_matches('/'));
        let canonical =
            std::fs::canonicalize(&path).map_err(|source| ContainmentError::ControlGroupIo {
                path: path.clone(),
                source,
            })?;
        let canonical_mount = std::fs::canonicalize(&self.cgroup_mount).map_err(|source| {
            ContainmentError::ControlGroupIo {
                path: self.cgroup_mount.clone(),
                source,
            }
        })?;
        if !canonical.starts_with(&canonical_mount) {
            return Err(ContainmentError::InvalidControlGroup(format!(
                "{} escapes the unified hierarchy",
                canonical.display()
            )));
        }
        for required in [
            "cgroup.events",
            "cgroup.procs",
            "cpu.stat",
            "memory.current",
            "memory.events",
            "pids.current",
            "pids.events",
        ] {
            let candidate = canonical.join(required);
            if !candidate.is_file() {
                return Err(ContainmentError::InvalidControlGroup(format!(
                    "{} does not expose {required}",
                    canonical.display()
                )));
            }
        }
        Ok(canonical)
    }

    async fn set_unit_policy(
        &self,
        unit: &str,
        policy: &WorkspaceResourcePolicy,
    ) -> Result<(), ContainmentError> {
        let mut arguments = vec![
            "--user".to_owned(),
            "set-property".to_owned(),
            "--runtime".to_owned(),
            unit.to_owned(),
        ];
        arguments.extend(complete_policy_properties(policy));
        let borrowed = arguments.iter().map(String::as_str).collect::<Vec<_>>();
        let output = run_command(
            "systemctl",
            &borrowed,
            "updating a workspace resource policy",
            SYSTEMD_CONTROL_TIMEOUT,
        )
        .await?;
        require_success(output, "updating a workspace resource policy")?;
        Ok(())
    }

    fn verify_configured_policy(
        &self,
        cgroup_path: &Path,
        policy: &WorkspaceResourcePolicy,
    ) -> Result<(), ContainmentError> {
        if let Some(expected) = policy.memory_high_bytes {
            verify_u64_property(cgroup_path, "memory.high", expected)?;
        }
        if let Some(expected) = policy.memory_max_bytes {
            verify_u64_property(cgroup_path, "memory.max", expected)?;
        }
        if let Some(expected) = policy.cpu_weight {
            verify_u64_property(cgroup_path, "cpu.weight", u64::from(expected))?;
        }
        if let Some(expected) = policy.tasks_max {
            verify_u64_property(cgroup_path, "pids.max", expected)?;
        }
        if policy.cpu_max_millicores.is_some() {
            verify_cpu_max(cgroup_path, policy.cpu_max_millicores)?;
        }
        Ok(())
    }

    fn verify_complete_policy(
        &self,
        cgroup_path: &Path,
        policy: &WorkspaceResourcePolicy,
    ) -> Result<(), ContainmentError> {
        verify_limit_property(cgroup_path, "memory.high", policy.memory_high_bytes)?;
        verify_limit_property(cgroup_path, "memory.max", policy.memory_max_bytes)?;
        verify_u64_property(
            cgroup_path,
            "cpu.weight",
            u64::from(policy.cpu_weight.unwrap_or(100)),
        )?;
        verify_limit_property(cgroup_path, "pids.max", policy.tasks_max)?;
        verify_cpu_max(cgroup_path, policy.cpu_max_millicores)?;
        Ok(())
    }

    async fn stop_unit(&self, unit: &str) -> Result<(), ContainmentError> {
        let output = run_command(
            "systemctl",
            &["--user", "stop", unit],
            "stopping a workspace scope",
            SYSTEMD_CONTROL_TIMEOUT,
        )
        .await?;
        if output.status.success() {
            return Ok(());
        }

        let state = run_command(
            "systemctl",
            &[
                "--user",
                "--no-pager",
                "show",
                "--property=LoadState",
                "--value",
                unit,
            ],
            "checking a stopped workspace scope",
            SYSTEMD_PROBE_TIMEOUT,
        )
        .await?;
        if state.status.success() && bounded_text(&state.stdout) == "not-found" {
            return Ok(());
        }
        Err(command_failed("stopping a workspace scope", &output))
    }
}

#[cfg(target_os = "linux")]
fn read_cgroup_value(path: &Path) -> Result<String, ContainmentError> {
    std::fs::read_to_string(path)
        .map(|value| value.trim().to_owned())
        .map_err(|source| ContainmentError::ControlGroupIo {
            path: path.to_owned(),
            source,
        })
}

#[cfg(target_os = "linux")]
fn active_cpu_max_is_limited(cgroup_path: &Path) -> Result<bool, ContainmentError> {
    match read_cgroup_value(&cgroup_path.join("cpu.max")) {
        Ok(value) => Ok(!value.starts_with("max ")),
        Err(ContainmentError::ControlGroupIo { source, .. })
            if source.kind() == std::io::ErrorKind::NotFound =>
        {
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

#[cfg(target_os = "linux")]
fn read_cgroup_u64(path: &Path) -> Result<u64, ContainmentError> {
    let value = read_cgroup_value(path)?;
    value
        .parse()
        .map_err(|_| ContainmentError::PolicyVerification {
            field: "cgroup counter",
            expected: "an unsigned integer".to_owned(),
            actual: value,
        })
}

#[cfg(target_os = "linux")]
fn verify_u64_property(
    cgroup_path: &Path,
    name: &'static str,
    expected: u64,
) -> Result<(), ContainmentError> {
    let path = cgroup_path.join(name);
    let actual = read_cgroup_value(&path)?;
    if actual == expected.to_string() {
        Ok(())
    } else {
        Err(ContainmentError::PolicyVerification {
            field: name,
            expected: expected.to_string(),
            actual,
        })
    }
}

#[cfg(target_os = "linux")]
fn verify_limit_property(
    cgroup_path: &Path,
    name: &'static str,
    expected: Option<u64>,
) -> Result<(), ContainmentError> {
    let actual = read_cgroup_value(&cgroup_path.join(name))?;
    let expected = expected.map_or_else(|| "max".to_owned(), |value| value.to_string());
    if actual == expected {
        Ok(())
    } else {
        Err(ContainmentError::PolicyVerification {
            field: name,
            expected,
            actual,
        })
    }
}

#[cfg(target_os = "linux")]
fn verify_cpu_max(
    cgroup_path: &Path,
    expected_millicores: Option<u32>,
) -> Result<(), ContainmentError> {
    let path = cgroup_path.join("cpu.max");
    let value = read_cgroup_value(&path)?;
    let mut fields = value.split_whitespace();
    let quota = fields.next();
    let period = fields.next().and_then(|field| field.parse::<u64>().ok());
    let matches = match (expected_millicores, quota, period, fields.next()) {
        (None, Some("max"), Some(_), None) => true,
        (Some(expected), Some(quota), Some(period), None) => quota
            .parse::<u64>()
            .ok()
            .is_some_and(|quota| cpu_ratio_matches(quota, period, expected)),
        _ => false,
    };
    if matches {
        Ok(())
    } else {
        Err(ContainmentError::PolicyVerification {
            field: "cpuMaxMillicores",
            expected: expected_millicores
                .map_or_else(|| "unlimited".to_owned(), |value| value.to_string()),
            actual: value,
        })
    }
}

#[cfg(target_os = "linux")]
fn cpu_ratio_matches(quota: u64, period: u64, expected_millicores: u32) -> bool {
    let actual = u128::from(quota) * 1_000;
    let expected = u128::from(expected_millicores) * u128::from(period);
    actual.abs_diff(expected) <= u128::from(period)
}

#[cfg(target_os = "linux")]
fn find_cgroup2_mount(mountinfo: &str) -> Option<PathBuf> {
    mountinfo.lines().find_map(|line| {
        let (mount, filesystem) = line.split_once(" - ")?;
        if filesystem.split_whitespace().next()? != "cgroup2" {
            return None;
        }
        let encoded = mount.split_whitespace().nth(4)?;
        decode_mount_field(encoded).map(PathBuf::from)
    })
}

#[cfg(target_os = "linux")]
fn decode_mount_field(value: &str) -> Option<String> {
    let mut decoded = String::with_capacity(value.len());
    let mut bytes = value.as_bytes().iter().copied();
    while let Some(byte) = bytes.next() {
        if byte != b'\\' {
            decoded.push(char::from(byte));
            continue;
        }
        let digits = [bytes.next()?, bytes.next()?, bytes.next()?];
        let value = digits.into_iter().try_fold(0_u8, |value, digit| {
            digit
                .checked_sub(b'0')
                .filter(|digit| *digit < 8)
                .and_then(|digit| value.checked_mul(8)?.checked_add(digit))
        })?;
        decoded.push(char::from(value));
    }
    Some(decoded)
}

#[cfg(target_os = "linux")]
fn parse_unified_membership(contents: &str) -> Option<&str> {
    contents.lines().find_map(|line| line.strip_prefix("0::"))
}

#[cfg(target_os = "linux")]
fn validate_relative_control_group(value: &str) -> Result<(), ContainmentError> {
    let relative = value.trim_start_matches('/');
    if relative.is_empty()
        || Path::new(relative)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(ContainmentError::InvalidControlGroup(value.to_owned()));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn validate_scope_control_group(
    value: &str,
    instance_slice: &str,
    workspace_slice: &str,
    unit: &str,
) -> Result<(), ContainmentError> {
    validate_relative_control_group(value)?;
    let path = Path::new(value);
    let workspace_parent = path.parent();
    if path.file_name() != Some(OsStr::new(unit))
        || workspace_parent.and_then(Path::file_name) != Some(OsStr::new(workspace_slice))
        || workspace_parent
            .and_then(Path::parent)
            .and_then(Path::file_name)
            != Some(OsStr::new(instance_slice))
    {
        return Err(ContainmentError::InvalidControlGroup(format!(
            "{value} is outside {instance_slice}/{workspace_slice}/{unit}"
        )));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn parse_scope_units(output: &[u8], prefix: &str) -> Vec<String> {
    String::from_utf8_lossy(output)
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|unit| {
            unit.strip_prefix(prefix)
                .and_then(|suffix| suffix.strip_suffix(".scope"))
                .is_some_and(|digest| {
                    digest.len() == 32
                        && digest
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                })
        })
        .map(str::to_owned)
        .collect()
}

#[cfg(target_os = "linux")]
async fn run_command(
    program: &'static str,
    arguments: &[&str],
    operation: &'static str,
    deadline: Duration,
) -> Result<Output, ContainmentError> {
    let mut command = Command::new(program);
    command
        .args(arguments)
        .env("LC_ALL", "C")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    timeout(deadline, command.output())
        .await
        .map_err(|_| ContainmentError::CommandTimeout(operation))?
        .map_err(|source| ContainmentError::CommandSpawn {
            program,
            operation,
            source,
        })
}

#[cfg(target_os = "linux")]
fn require_success(output: Output, operation: &'static str) -> Result<Output, ContainmentError> {
    if output.status.success() {
        Ok(output)
    } else {
        Err(command_failed(operation, &output))
    }
}

#[cfg(target_os = "linux")]
fn command_failed(operation: &'static str, output: &Output) -> ContainmentError {
    let message = if output.stderr.is_empty() {
        format_exit_status(output.status)
    } else {
        bounded_text(&output.stderr)
    };
    ContainmentError::CommandFailed { operation, message }
}

#[cfg(target_os = "linux")]
fn format_exit_status(status: ExitStatus) -> String {
    status.code().map_or_else(
        || "terminated by a signal".to_owned(),
        |code| format!("exit {code}"),
    )
}

#[cfg(target_os = "linux")]
fn bounded_text(bytes: &[u8]) -> String {
    let start = bytes.len().saturating_sub(SYSTEMD_OUTPUT_BYTES);
    String::from_utf8_lossy(&bytes[start..])
        .chars()
        .map(|character| {
            if character.is_control() && character != '\n' && character != '\t' {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    mod policy;

    #[test]
    fn containment_mode_defaults_to_auto_and_rejects_unknown_values() {
        assert_eq!(parse_mode(None).unwrap(), ContainmentMode::Auto);
        assert_eq!(
            parse_mode(Some("process-tree")).unwrap(),
            ContainmentMode::ProcessTree
        );
        assert!(parse_mode(Some("shared")).is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn parses_cgroup2_mounts_and_kernel_path_escapes() {
        let mountinfo = "24 22 0:20 / / rw - ext4 /dev/root rw\n\
                         30 24 0:26 / /sys/fs/cgroup\\040test rw - cgroup2 cgroup rw\n";
        assert_eq!(
            find_cgroup2_mount(mountinfo),
            Some(PathBuf::from("/sys/fs/cgroup test"))
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn accepts_only_unified_absolute_membership_paths() {
        assert_eq!(
            parse_unified_membership("1:name:/legacy\n0::/user.slice/test.scope\n"),
            Some("/user.slice/test.scope")
        );
        assert!(validate_relative_control_group("/user.slice/test.scope").is_ok());
        assert!(validate_relative_control_group("/").is_err());
        assert!(validate_relative_control_group("/user.slice/../other").is_err());

        let workspace_slice = "app-cocoabc-workspaces.slice";
        let instance_slice = "app-cocoabc.slice";
        let unit = "cocoabc-workspace-11111111111111111111111111111111.scope";
        assert!(
            validate_scope_control_group(
                &format!("/user.slice/{instance_slice}/{workspace_slice}/{unit}"),
                instance_slice,
                workspace_slice,
                unit,
            )
            .is_ok()
        );
        assert!(
            validate_scope_control_group(
                &format!("/user.slice/{instance_slice}/app-other.slice/{unit}"),
                instance_slice,
                workspace_slice,
                unit,
            )
            .is_err()
        );
        assert!(
            validate_scope_control_group(
                &format!("/user.slice/app-other.slice/{workspace_slice}/{unit}"),
                instance_slice,
                workspace_slice,
                unit,
            )
            .is_err()
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn stale_scope_parser_cannot_expand_beyond_the_instance_prefix() {
        let output = b"cocoabc-workspace-11111111111111111111111111111111.scope loaded active running one\n\
                       cocodef-workspace-22222222222222222222222222222222.scope loaded active running other\n\
                       cocoabc-workspace-33333333333333333333333333333333.service loaded active running wrong\n\
                       cocoabc-workspace-short.scope loaded active running malformed\n\
                       cocoabc-workspace-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA.scope loaded active running foreign\n";
        assert_eq!(
            parse_scope_units(output, "cocoabc-workspace-"),
            vec!["cocoabc-workspace-11111111111111111111111111111111.scope"]
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn systemd_names_are_stable_opaque_and_hierarchical() {
        let backend = SystemdBackend {
            cgroup_mount: PathBuf::from("/sys/fs/cgroup"),
            instance: "0123456789abcdef0123456789abcdef".to_owned(),
            instance_slice: "app-coco0123456789abcdef0123456789abcdef.slice".to_owned(),
            workspace_slice: "app-coco0123456789abcdef0123456789abcdef-workspaces.slice".to_owned(),
            scope_prefix: "coco0123456789abcdef0123456789abcdef-workspace-".to_owned(),
        };
        let unit = backend.workspace_unit("secret/workspace-name");
        assert_eq!(unit, backend.workspace_unit("secret/workspace-name"));
        assert_ne!(unit, backend.workspace_unit("another"));
        assert!(!unit.contains("secret"));
        assert!(backend.instance_slice.starts_with("app-coco"));
        assert!(backend.instance_slice.ends_with(".slice"));
        assert!(backend.workspace_slice.starts_with("app-coco"));
        assert!(backend.workspace_slice.ends_with("-workspaces.slice"));
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[ignore = "requires a cgroup-v2 systemd user manager"]
    async fn live_scope_contains_descendants_and_restart_cleanup_stops_the_tree() {
        let directory = tempfile::tempdir().unwrap();
        let backend = SystemdBackend::detect(directory.path()).await.unwrap();
        backend.cleanup_stale_scopes().await.unwrap();
        let containment = WorkspaceContainment {
            backend: ContainmentBackend::Systemd(backend),
        };
        let pending = containment.prepare("live-containment-test");
        let mut command = pending.command(Path::new("/bin/sh"), &["-c", "sleep 30 & wait"]);
        command
            .current_dir(directory.path())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let mut child = command.spawn().unwrap();
        let supervisor_pid = child.id().unwrap();

        let result: Result<(), String> = async {
            let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
            let active = loop {
                match pending.clone().activate(supervisor_pid).await {
                    Ok(active) => break active,
                    Err(error) if tokio::time::Instant::now() < deadline => {
                        debug!(%error, "waiting for disposable test scope");
                        tokio::time::sleep(Duration::from_millis(25)).await;
                    }
                    Err(error) => {
                        return Err(format!("test scope did not become active: {error}"));
                    }
                }
            };
            let cgroup_path = active
                .cgroup_path()
                .ok_or_else(|| "live systemd scope had no cgroup path".to_owned())?;
            let usage = super::super::resources::inspect_cgroup(cgroup_path, None)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "live systemd scope had no usage snapshot".to_owned())?;
            if !usage.process_count.is_some_and(|count| count >= 2)
                || !usage.task_count.is_some_and(|count| count >= 2)
                || !usage.memory_current_bytes.is_some_and(|bytes| bytes > 0)
            {
                return Err("live systemd scope did not account its descendant".to_owned());
            }
            let next_generation = SystemdBackend::detect(directory.path())
                .await
                .map_err(|error| error.to_string())?;
            next_generation
                .cleanup_stale_scopes()
                .await
                .map_err(|error| error.to_string())?;
            Ok(())
        }
        .await;

        let cleanup = pending.cleanup().await;
        let status = timeout(Duration::from_secs(5), child.wait())
            .await
            .expect("systemd-run supervisor did not exit")
            .expect("could not wait for systemd-run supervisor");
        cleanup.expect("could not clean up disposable test scope");
        result.expect("live containment contract failed");
        assert!(
            !status.success(),
            "stopped workload unexpectedly exited zero"
        );
    }
}
