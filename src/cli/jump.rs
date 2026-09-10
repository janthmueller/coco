use std::path::PathBuf;
use std::process::Stdio;

use anyhow::{Context, Result, bail};

use crate::paths::CocoPaths;
use crate::protocol::{
    AppServerEndpoint, WorkspaceAttachLaunch, WorkspaceAttachReleaseParams,
    WorkspaceAttachRenewParams, WorkspaceAttachResult, WorkspaceExecutionEnvironment,
};
use crate::rpc::RpcClient;

mod relay;

const REMOTE_TOKEN_ENV: &str = "COCO_CODEX_REMOTE_CAPABILITY_TOKEN";

pub(super) async fn jump(
    paths: &CocoPaths,
    client: &RpcClient,
    result: WorkspaceAttachResult,
) -> Result<()> {
    let lease_id = match &result.launch {
        WorkspaceAttachLaunch::Start { lease_id }
        | WorkspaceAttachLaunch::Resume { lease_id, .. } => lease_id.clone(),
    };
    let release = (result.workspace.id.clone(), lease_id);
    let outcome = jump_inner(paths, client, result).await;
    let (workspace_id, lease_id) = release;
    let release_outcome = client
        .request(WorkspaceAttachReleaseParams {
            workspace_id,
            lease_id,
        })
        .await
        .map(|_| ())
        .context("could not release the temporary jump lease");
    match (outcome, release_outcome) {
        (Ok(()), release) => release,
        (Err(error), Ok(())) => Err(error),
        (Err(error), Err(release_error)) => Err(error.context(format!(
            "the jump also failed to release its lease: {release_error:#}"
        ))),
    }
}

async fn jump_inner(
    paths: &CocoPaths,
    client: &RpcClient,
    result: WorkspaceAttachResult,
) -> Result<()> {
    let target = load_jump_target(paths, result).await?;
    let codex_binary = std::env::var_os("COCO_CODEX_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("codex"));
    match target.launch.clone() {
        WorkspaceAttachLaunch::Resume {
            thread_id,
            lease_id,
        } => {
            if target.execution_environment.is_some() {
                run_relayed_resume(target, thread_id, lease_id, codex_binary, client.clone()).await
            } else {
                run_resume_command(
                    resume_command(&target, &thread_id, codex_binary),
                    client,
                    &target.workspace_id,
                    &lease_id,
                )
                .await
            }
        }
        WorkspaceAttachLaunch::Start { lease_id } => {
            run_fresh_jump(target, lease_id, codex_binary, client.clone()).await
        }
    }
}

async fn run_fresh_jump(
    target: JumpTarget,
    lease_id: String,
    codex_binary: PathBuf,
    client: RpcClient,
) -> Result<()> {
    let relay = relay::PreparedRelay::start(
        client,
        target.workspace_id.clone(),
        lease_id,
        &target.endpoint_url,
        &target.capability_token,
        target.execution_environment.clone(),
    )
    .await?;
    let mut command = fresh_command(
        &target,
        codex_binary,
        relay.endpoint_url(),
        relay.capability_token(),
    );
    let status = match command.status().await {
        Ok(status) => status,
        Err(error) => {
            relay.abort().await;
            return Err(error).context("could not start the Codex terminal UI");
        }
    };
    let relay_outcome = relay.finish().await;
    if !status.success() {
        bail!("Codex terminal UI exited with {status}");
    }
    relay_outcome
}

async fn run_relayed_resume(
    target: JumpTarget,
    thread_id: String,
    lease_id: String,
    codex_binary: PathBuf,
    client: RpcClient,
) -> Result<()> {
    let relay = relay::PreparedRelay::start(
        client,
        target.workspace_id.clone(),
        lease_id,
        &target.endpoint_url,
        &target.capability_token,
        target.execution_environment.clone(),
    )
    .await?;
    let mut command = resume_command_to(
        &target,
        &thread_id,
        codex_binary,
        relay.endpoint_url(),
        relay.capability_token(),
    );
    let status = match command.status().await {
        Ok(status) => status,
        Err(error) => {
            relay.abort().await;
            return Err(error).context("could not start the Codex terminal UI");
        }
    };
    let relay_outcome = relay.finish().await;
    if !status.success() {
        bail!("Codex terminal UI exited with {status}");
    }
    relay_outcome
}

async fn run_resume_command(
    mut command: tokio::process::Command,
    client: &RpcClient,
    workspace_id: &str,
    lease_id: &str,
) -> Result<()> {
    let mut child = command
        .spawn()
        .context("could not start the Codex terminal UI")?;
    let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(10));
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let status = loop {
        tokio::select! {
            status = child.wait() => break status.context("could not wait for the Codex terminal UI")?,
            _ = heartbeat.tick() => {
                if let Err(error) = client.request(WorkspaceAttachRenewParams {
                    workspace_id: workspace_id.to_owned(),
                    lease_id: lease_id.to_owned(),
                }).await {
                    let _ = child.kill().await;
                    return Err(error).context("cocod could not renew the TUI lease");
                }
            }
        }
    };
    if !status.success() {
        bail!("Codex terminal UI exited with {status}");
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct JumpTarget {
    pub(super) workspace_id: String,
    pub(super) worktree: PathBuf,
    pub(super) profile: String,
    pub(super) model: Option<String>,
    pub(super) launch: WorkspaceAttachLaunch,
    pub(super) endpoint_url: String,
    pub(super) capability_token: String,
    pub(super) codex_home: PathBuf,
    pub(super) execution_environment: Option<WorkspaceExecutionEnvironment>,
}

pub(super) async fn load_jump_target(
    paths: &CocoPaths,
    result: WorkspaceAttachResult,
) -> Result<JumpTarget> {
    validate_launch_binding(&result)?;
    let worktree = result
        .workspace
        .worktree_path
        .clone()
        .context("workspace has no managed worktree yet")?;
    let (endpoint_url, capability_token) = load_app_server_endpoint(paths).await?;
    Ok(JumpTarget {
        workspace_id: result.workspace.id,
        worktree,
        profile: result.workspace.profile.name,
        model: result.workspace.profile.model_override,
        launch: result.launch,
        endpoint_url,
        capability_token,
        codex_home: paths.codex_home.clone(),
        execution_environment: result.execution_environment,
    })
}

fn validate_launch_binding(result: &WorkspaceAttachResult) -> Result<()> {
    match (&result.launch, result.workspace.codex_thread_id.as_deref()) {
        (WorkspaceAttachLaunch::Start { .. }, None) => Ok(()),
        (WorkspaceAttachLaunch::Resume { thread_id, .. }, Some(bound)) if thread_id == bound => {
            Ok(())
        }
        (WorkspaceAttachLaunch::Start { .. }, Some(_)) => {
            bail!("cocod requested a fresh TUI for an already-bound workspace")
        }
        (WorkspaceAttachLaunch::Resume { .. }, None) => {
            bail!("cocod requested a TUI resume without a bound Codex thread")
        }
        (WorkspaceAttachLaunch::Resume { .. }, Some(_)) => {
            bail!("cocod returned conflicting Codex thread IDs for jump")
        }
    }
}

async fn load_app_server_endpoint(paths: &CocoPaths) -> Result<(String, String)> {
    let descriptor_bytes = tokio::fs::read(&paths.codex_endpoint_path)
        .await
        .with_context(|| {
            format!(
                "could not read {}; is cocod running?",
                paths.codex_endpoint_path.display()
            )
        })?;
    let descriptor: AppServerEndpoint = serde_json::from_slice(&descriptor_bytes)
        .context("cocod published an invalid App Server endpoint")?;
    let port = descriptor.url.strip_prefix("ws://127.0.0.1:");
    if descriptor.schema_version != 1
        || port
            .and_then(|port| port.parse::<u16>().ok())
            .is_none_or(|port| port == 0)
    {
        bail!("cocod published an unsupported App Server endpoint");
    }
    let token = tokio::fs::read_to_string(&paths.codex_token_path)
        .await
        .with_context(|| format!("could not read {}", paths.codex_token_path.display()))?;
    let capability_token = token.trim().to_owned();
    if capability_token.is_empty() {
        bail!("cocod published an empty App Server capability token");
    }
    Ok((descriptor.url, capability_token))
}

pub(super) fn resume_command(
    target: &JumpTarget,
    thread_id: &str,
    codex_binary: PathBuf,
) -> tokio::process::Command {
    resume_command_to(
        target,
        thread_id,
        codex_binary,
        &target.endpoint_url,
        &target.capability_token,
    )
}

fn resume_command_to(
    target: &JumpTarget,
    thread_id: &str,
    codex_binary: PathBuf,
    endpoint: &str,
    capability_token: &str,
) -> tokio::process::Command {
    let mut command = base_command(target, codex_binary);
    command
        .arg("resume")
        .arg(thread_id)
        .args(["--remote", endpoint])
        .args(["--remote-auth-token-env", REMOTE_TOKEN_ENV])
        .arg("-C")
        .arg(&target.worktree)
        .env(REMOTE_TOKEN_ENV, capability_token);
    command
}

pub(super) fn fresh_command(
    target: &JumpTarget,
    codex_binary: PathBuf,
    relay_endpoint: &str,
    relay_token: &str,
) -> tokio::process::Command {
    let mut command = base_command(target, codex_binary);
    command
        .args(["--remote", relay_endpoint])
        .args(["--remote-auth-token-env", REMOTE_TOKEN_ENV])
        .arg("-C")
        .arg(&target.worktree);
    if target.profile != "default" {
        command.args(["--profile", &target.profile]);
    }
    if let Some(model) = target.model.as_deref() {
        command.args(["--model", model]);
    }
    command.env(REMOTE_TOKEN_ENV, relay_token);
    command
}

fn base_command(target: &JumpTarget, codex_binary: PathBuf) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(codex_binary);
    command
        .current_dir(&target.worktree)
        .env("CODEX_HOME", &target.codex_home)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    command
}
