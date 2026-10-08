use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Serialize;
use tokio::process::Command;
use tokio::time::{Instant, timeout, timeout_at};

use crate::diagnostics::{
    PROBE_TIMEOUT, ProbeError, REPORT_TIMEOUT, capture_command, parse_version,
};
use crate::paths::{CocoPaths, resolve_executable};
use crate::protocol::{
    DiagnosticCheck, DiagnosticStatus, DoctorDaemonInfo, DoctorParams, HealthParams,
};
use crate::rpc::{RpcClient, RpcClientError};

use super::output::{print_json, versioned};

mod output;
#[cfg(test)]
mod tests;

#[derive(Debug, thiserror::Error)]
#[error("doctor found errors")]
pub(super) struct ReportFailed;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DoctorReport {
    cli_version: &'static str,
    cli_executable: Option<PathBuf>,
    cocod_executable: Option<PathBuf>,
    codex_executable: Option<PathBuf>,
    daemon: Option<DoctorDaemonInfo>,
    checks: Vec<DiagnosticCheck>,
    complete: bool,
}

pub(super) async fn run(paths: &CocoPaths, json: bool) -> Result<()> {
    let report = collect(paths).await;
    if json {
        print_json(versioned(serde_json::to_value(&report)?))?;
    } else {
        print!(
            "{}",
            output::render(&report, super::style::Palette::stdout())
        );
    }
    if report
        .checks
        .iter()
        .any(|check| check.status == DiagnosticStatus::Error)
    {
        return Err(ReportFailed.into());
    }
    Ok(())
}

async fn collect(paths: &CocoPaths) -> DoctorReport {
    let codex_program = std::env::var_os("COCO_CODEX_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| "codex".into());
    let mut report = DoctorReport {
        cli_version: env!("CARGO_PKG_VERSION"),
        cli_executable: std::env::current_exe().ok(),
        cocod_executable: resolve_executable(Path::new("cocod")),
        codex_executable: resolve_executable(&codex_program),
        daemon: None,
        checks: vec![DiagnosticCheck::new(
            "installation.coco",
            "coco",
            DiagnosticStatus::Ok,
            env!("CARGO_PKG_VERSION"),
        )],
        complete: true,
    };
    let daemon_version = version_check(
        "cocod",
        report.cocod_executable.as_deref(),
        "cocod ",
        &mut report.checks,
    )
    .await;
    let codex_version = version_check(
        "codex",
        report.codex_executable.as_deref(),
        "codex-cli ",
        &mut report.checks,
    )
    .await;
    let git = resolve_executable(Path::new("git"));
    version_check("git", git.as_deref(), "git version ", &mut report.checks).await;
    let client = RpcClient::new(paths.socket_path.clone());
    match timeout(PROBE_TIMEOUT, client.request(HealthParams {})).await {
        Ok(Ok(health)) if health.status == "ok" => {
            report.checks.push(DiagnosticCheck::new(
                "daemon",
                "Coordinator",
                DiagnosticStatus::Ok,
                "Responding",
            ));
            collect_live(
                &client,
                paths,
                daemon_version.as_deref(),
                codex_version.as_deref(),
                &mut report,
                Instant::now() + REPORT_TIMEOUT + PROBE_TIMEOUT,
            )
            .await;
        }
        response => {
            report.complete = false;
            let timed_out = response.is_err();
            let message = if response.is_err() {
                "No response within 3s"
            } else {
                "Not reachable or did not pass its health check"
            };
            let mut check = DiagnosticCheck::new("daemon", "Coordinator", DiagnosticStatus::Error, message)
                .hint("Start cocod in another terminal. If it is already running, check COCO_SOCKET_PATH and its log.");
            if timed_out {
                check = check.timed_out();
            }
            report.checks.push(check);
            report.checks.push(DiagnosticCheck::new(
                "daemon.checks",
                "Workspace checks",
                DiagnosticStatus::Skipped,
                "Require a responding coordinator",
            ));
        }
    }
    report.complete &= !report.checks.iter().any(|check| check.timed_out);
    report
}

async fn collect_live(
    client: &RpcClient,
    paths: &CocoPaths,
    daemon_version: Option<&str>,
    codex_version: Option<&str>,
    report: &mut DoctorReport,
    deadline: Instant,
) {
    match timeout_at(deadline, client.request(DoctorParams {})).await {
        Ok(Ok(inspected)) => {
            report.complete &= inspected.complete;
            installation_consistency(
                paths,
                daemon_version,
                codex_version,
                &inspected.daemon,
                report,
            );
            report.daemon = Some(inspected.daemon);
            report.checks.extend(inspected.checks);
        }
        Ok(Err(RpcClientError::Remote(error))) if error.code == "METHOD_NOT_FOUND" => {
            report.complete = false;
            report.checks.push(DiagnosticCheck::new("daemon.checks", "Workspace checks", DiagnosticStatus::Error, "Running coordinator does not support doctor")
                .hint("Update coco and cocod together. Stop active work before restarting the coordinator."));
        }
        response => {
            report.complete = false;
            let mut check = DiagnosticCheck::new(
                "daemon.checks",
                "Workspace checks",
                DiagnosticStatus::Error,
                if response.is_err() {
                    "Coordinator diagnostics timed out"
                } else {
                    "Coordinator diagnostics failed"
                },
            )
            .hint("Check the coordinator's log. Stop active work before restarting it.");
            if response.is_err() {
                check = check.timed_out();
            }
            report.checks.push(check);
        }
    }
}

async fn version_check(
    name: &str,
    executable: Option<&Path>,
    prefix: &str,
    checks: &mut Vec<DiagnosticCheck>,
) -> Option<String> {
    let Some(executable) = executable else {
        checks.push(
            DiagnosticCheck::new(
                format!("installation.{name}"),
                name,
                DiagnosticStatus::Error,
                "Executable not found",
            )
            .hint(if name == "codex" {
                "Install Codex, or set COCO_CODEX_BINARY in both terminals."
            } else {
                "Install the command and make it available on PATH."
            }),
        );
        return None;
    };
    let mut command = Command::new(executable);
    command.arg("--version");
    record_version_result(name, prefix, capture_command(command).await, checks)
}

fn record_version_result(
    name: &str,
    prefix: &str,
    result: Result<String, ProbeError>,
    checks: &mut Vec<DiagnosticCheck>,
) -> Option<String> {
    match result.and_then(|output| parse_version(&output, prefix).ok_or(ProbeError::InvalidOutput))
    {
        Ok(version) => {
            checks.push(DiagnosticCheck::new(
                format!("installation.{name}"),
                name,
                DiagnosticStatus::Ok,
                &version,
            ));
            Some(version)
        }
        Err(error) => {
            let mut check = DiagnosticCheck::new(
                format!("installation.{name}"),
                name,
                DiagnosticStatus::Error,
                if error == ProbeError::TimedOut {
                    "Version check did not finish within 3s"
                } else {
                    "Version check failed or returned unexpected output"
                },
            )
            .hint("Check the selected executable. Doctor does not include its raw output.");
            if error == ProbeError::TimedOut {
                check = check.timed_out();
            }
            checks.push(check);
            None
        }
    }
}

fn installation_consistency(
    paths: &CocoPaths,
    daemon_version: Option<&str>,
    codex_version: Option<&str>,
    daemon: &DoctorDaemonInfo,
    report: &mut DoctorReport,
) {
    let versions_match = daemon.version == report.cli_version
        && daemon_version.is_none_or(|version| version == daemon.version);
    let binary_matches = match (&report.cocod_executable, &daemon.executable) {
        (Some(installed), Some(running)) => installed == running,
        _ => true,
    };
    let check = if versions_match && binary_matches {
        DiagnosticCheck::new(
            "installation.live",
            "Running installation",
            DiagnosticStatus::Ok,
            format!("cocod {}", daemon.version),
        )
    } else {
        DiagnosticCheck::new(
            "installation.live",
            "Running installation",
            DiagnosticStatus::Warning,
            "CLI, installed cocod, and running coordinator differ",
        )
        .hint(
            "Use the same installation. Stop active work before restarting cocod after an update.",
        )
    };
    report.checks.push(check);
    if daemon
        .codex_version
        .as_deref()
        .zip(codex_version)
        .is_some_and(|(running, installed)| running != installed)
        || daemon
            .codex_binary
            .as_ref()
            .zip(report.codex_executable.as_ref())
            .is_some_and(|(running, installed)| running != installed)
    {
        report.checks.push(DiagnosticCheck::new("installation.codex.live", "Running Codex", DiagnosticStatus::Warning, "Coordinator and this terminal select different Codex installations")
            .hint("Use the same COCO_CODEX_BINARY and stop active work before restarting cocod after an update."));
    }
    if paths.codex_home != daemon.codex_home
        || paths.database_path != daemon.database_path
        || paths.codex_endpoint_path != daemon.endpoint_path
        || paths.codex_token_path != daemon.token_path
    {
        report.checks.push(
            DiagnosticCheck::new(
                "installation.paths",
                "Configuration",
                DiagnosticStatus::Warning,
                "Coordinator and CLI use different data or Codex paths",
            )
            .hint("Use matching CODEX_HOME and COCO_* path overrides in both terminals."),
        );
    }
}
