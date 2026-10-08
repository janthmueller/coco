use std::path::{Path, PathBuf};

use tokio::time::{Instant, timeout_at};

use crate::codex::{CodexClient, CodexClientOptions};
use crate::coordinator::Coordinator;
use crate::diagnostics::{PROBE_TIMEOUT, REPORT_TIMEOUT};
use crate::domain::runtime::WorkspaceResourceCapabilities;
use crate::paths::{CocoPaths, resolve_executable};
use crate::protocol::{DiagnosticCheck, DiagnosticStatus, DoctorDaemonInfo, DoctorResult};

use super::execution::{WorkspaceExecutionMode, WorkspaceExecutors};

pub(super) struct DoctorContext {
    paths: CocoPaths,
    executable: Option<PathBuf>,
    codex_binary: Option<PathBuf>,
    mode: WorkspaceExecutionMode,
    capabilities: WorkspaceResourceCapabilities,
    codex: CodexClient,
}

impl DoctorContext {
    pub(super) fn new(
        paths: &CocoPaths,
        options: &CodexClientOptions,
        mode: WorkspaceExecutionMode,
        executors: Option<&WorkspaceExecutors>,
        codex: CodexClient,
    ) -> Self {
        Self {
            paths: paths.clone(),
            executable: std::env::current_exe().ok(),
            codex_binary: resolve_executable(&options.codex_binary),
            mode,
            capabilities: executors.map_or_else(
                WorkspaceResourceCapabilities::unavailable,
                WorkspaceExecutors::resource_capabilities,
            ),
            codex,
        }
    }

    pub(super) async fn report(&self, coordinator: &Coordinator) -> DoctorResult {
        let deadline = Instant::now() + REPORT_TIMEOUT;
        let mut checks = Vec::new();
        for (id, label, path) in [
            ("daemon.socket", "Local connection", &self.paths.socket_path),
            (
                "daemon.endpoint",
                "Jump endpoint",
                &self.paths.codex_endpoint_path,
            ),
            (
                "daemon.token",
                "Jump credentials",
                &self.paths.codex_token_path,
            ),
        ] {
            checks.push(private_file_check(id, label, path, deadline).await);
        }
        checks.push(self.resource_check());
        let inspected = coordinator.doctor_checks(deadline).await;
        checks.extend(inspected.checks);
        let complete = inspected.complete && !checks.iter().any(|check| check.timed_out);
        DoctorResult {
            daemon: DoctorDaemonInfo {
                version: env!("CARGO_PKG_VERSION").to_owned(),
                pid: std::process::id(),
                executable: self.executable.clone(),
                codex_binary: self.codex_binary.clone(),
                codex_version: self.codex.server_version().map(str::to_owned),
                codex_home: self.paths.codex_home.clone(),
                database_path: self.paths.database_path.clone(),
                endpoint_path: self.paths.codex_endpoint_path.clone(),
                token_path: self.paths.codex_token_path.clone(),
                execution_mode: match self.mode {
                    WorkspaceExecutionMode::ExecServer => "exec-server",
                    WorkspaceExecutionMode::Shared => "shared",
                }
                .to_owned(),
            },
            checks,
            complete,
        }
    }

    fn resource_check(&self) -> DiagnosticCheck {
        let (status, message) = if self.mode == WorkspaceExecutionMode::Shared {
            (
                DiagnosticStatus::Skipped,
                "Shared execution; workspace measurements and limits are unavailable",
            )
        } else if !cfg!(target_os = "linux") {
            (
                DiagnosticStatus::Skipped,
                "Workspace measurements and limits require supported Linux systems",
            )
        } else if self.capabilities.memory_max
            && self.capabilities.cpu_max
            && self.capabilities.tasks_max
        {
            (
                DiagnosticStatus::Ok,
                "Memory, CPU, and process/thread limits are available",
            )
        } else {
            (
                DiagnosticStatus::Warning,
                "Basic measurements are available; some resource limits are unavailable",
            )
        };
        let check = DiagnosticCheck::new("resources", "Resource control", status, message);
        if status == DiagnosticStatus::Warning {
            check.hint("Use a systemd user session with cgroup v2 for resource limits, or keep using basic measurements.")
        } else {
            check
        }
    }
}

#[cfg(test)]
mod tests;

async fn private_file_check(
    id: &str,
    label: &str,
    path: &Path,
    deadline: Instant,
) -> DiagnosticCheck {
    let inspected = timeout_at(
        deadline.min(Instant::now() + PROBE_TIMEOUT),
        tokio::fs::symlink_metadata(path),
    )
    .await;
    match inspected {
        Ok(Ok(metadata)) => {
            #[cfg(unix)]
            let private = {
                use std::os::unix::fs::{FileTypeExt, PermissionsExt};
                let correct_type = if id == "daemon.socket" {
                    metadata.file_type().is_socket()
                } else {
                    metadata.is_file()
                };
                correct_type
                    && !metadata.file_type().is_symlink()
                    && metadata.permissions().mode() & 0o077 == 0
            };
            #[cfg(not(unix))]
            let private = !metadata.file_type().is_symlink();
            if private {
                DiagnosticCheck::new(
                    id,
                    label,
                    DiagnosticStatus::Ok,
                    "Private runtime file is present",
                )
            } else {
                DiagnosticCheck::new(
                    id,
                    label,
                    DiagnosticStatus::Error,
                    "Runtime file type or permissions are unsafe",
                )
                .hint("Stop active work, then restart cocod to recreate its private runtime files.")
            }
        }
        Ok(Err(_)) => DiagnosticCheck::new(
            id,
            label,
            DiagnosticStatus::Error,
            "Runtime file is missing or inaccessible",
        )
        .hint("Stop active work before restarting cocod. Check its runtime path overrides."),
        Err(_) => DiagnosticCheck::new(
            id,
            label,
            DiagnosticStatus::Error,
            "Runtime file inspection timed out",
        )
        .timed_out(),
    }
}
