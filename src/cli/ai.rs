//! Thin native dialog launcher. Application actions remain Control MCP calls.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{Context, Result, bail, ensure};
use tokio::process::Command;

use super::args::AiArgs;
use crate::diagnostics::{capture_command, parse_version};
use crate::paths::{CocoPaths, resolve_executable};
use crate::protocol::{
    CONTROL_CODEX_VERSION, ControlSessionPreparation, ControlSessionPrepareParams,
};
use crate::rpc::RpcClient;

mod config;
#[cfg(test)]
mod tests;

pub(super) async fn run(paths: &CocoPaths, repository_path: PathBuf, args: AiArgs) -> Result<()> {
    let program = std::env::var_os("COCO_CODEX_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| "codex".into());
    let program = resolve_executable(&program).context("Codex is not installed or executable")?;
    run_verified(paths, repository_path, args, &program).await
}

async fn run_verified(
    paths: &CocoPaths,
    repository: PathBuf,
    args: AiArgs,
    program: &Path,
) -> Result<()> {
    let mut probe = Command::new(program);
    probe.arg("--version");
    let version = capture_command(probe)
        .await
        .map_err(|_| anyhow::anyhow!("could not check the Codex version"))?;
    ensure!(
        parse_version(&version, "codex-cli ").as_deref() == Some(CONTROL_CODEX_VERSION),
        "coco ai requires the verified Codex {CONTROL_CODEX_VERSION} configuration contract"
    );
    let client = RpcClient::new(paths.socket_path.clone());
    let prepared = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        client.request(ControlSessionPrepareParams {
            repository_path: repository,
        }),
    )
    .await
    .context("CoCo AI preparation timed out; inspect the coordinator before retrying")?
    .context("could not prepare CoCo AI; start a matching cocod and retry")?;
    validate_preparation(paths, &prepared)?;
    let current =
        std::env::current_exe().context("could not locate the running CoCo executable")?;
    let mut command = launch_command(paths, &prepared, &args, program, &current)?;
    let status = command
        .status()
        .await
        .context("could not start the CoCo control conversation")?;
    if !status.success() {
        bail!("Codex control conversation exited with {status}");
    }
    Ok(())
}

fn validate_preparation(paths: &CocoPaths, prepared: &ControlSessionPreparation) -> Result<()> {
    ensure!(
        prepared.coco_version == env!("CARGO_PKG_VERSION"),
        "coco and cocod versions differ; restart cocod with the matching version"
    );
    ensure!(
        std::fs::canonicalize(&prepared.codex_home)? == std::fs::canonicalize(&paths.codex_home)?,
        "coco ai and cocod must use the same CODEX_HOME"
    );
    ensure!(
        prepared.working_directory.is_absolute() && prepared.working_directory.is_dir(),
        "CoCo returned an unavailable control directory"
    );
    ensure!(
        !prepared.repositories.is_empty() && prepared.repositories.len() <= 33,
        "CoCo returned an invalid repository selection"
    );
    ensure!(
        prepared
            .repositories
            .iter()
            .all(|repo| repo.path.is_absolute() && repo.path.is_dir()),
        "CoCo returned an unavailable repository directory"
    );
    ensure!(
        prepared
            .default_repository_id
            .as_ref()
            .is_none_or(|id| prepared.repositories.iter().any(|repo| &repo.id == id)),
        "CoCo returned an unknown default repository"
    );
    Ok(())
}

fn launch_command(
    paths: &CocoPaths,
    prepared: &ControlSessionPreparation,
    args: &AiArgs,
    codex: &Path,
    coco: &Path,
) -> Result<Command> {
    let mut command = Command::new(codex);
    command
        .args(["--no-daemon", "--sandbox", "read-only"])
        .arg("--cd")
        .arg(&prepared.working_directory)
        .env("CODEX_HOME", std::fs::canonicalize(&prepared.codex_home)?)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    for value in config::overrides(paths, prepared, args, coco)? {
        command.arg("--config").arg(value);
    }
    if let Some(model) = &args.model {
        command.arg("--model").arg(model);
    }
    if let Some(prompt) = &args.prompt {
        command.arg("--").arg(prompt);
    }
    Ok(command)
}
