use std::path::PathBuf;
use std::process::Stdio;

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::paths::CocoPaths;
use crate::protocol::AppServerEndpoint;

pub(super) async fn jump(paths: &CocoPaths, result: &Value) -> Result<()> {
    let target = load_jump_target(paths, result).await?;
    let codex_binary = std::env::var_os("COCO_CODEX_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("codex"));
    let status = jump_command(&target, codex_binary)
        .status()
        .await
        .context("could not start the Codex terminal UI")?;
    if !status.success() {
        bail!("Codex terminal UI exited with {status}");
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct JumpTarget {
    pub(super) worktree: PathBuf,
    thread_id: String,
    endpoint_url: String,
    capability_token: String,
}

pub(super) async fn load_jump_target(paths: &CocoPaths, result: &Value) -> Result<JumpTarget> {
    let task = result
        .get("task")
        .context("cocod returned task.get without a task")?;
    let worktree = task
        .get("worktreePath")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .context("task has no managed worktree yet")?;
    let thread_id = task
        .get("codexThreadId")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .context("task has no Codex thread yet")?;
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

    Ok(JumpTarget {
        worktree,
        thread_id,
        endpoint_url: descriptor.url,
        capability_token,
    })
}

pub(super) fn jump_command(target: &JumpTarget, codex_binary: PathBuf) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(codex_binary);
    command
        .arg("resume")
        .arg(&target.thread_id)
        .args(["--remote", &target.endpoint_url])
        .args([
            "--remote-auth-token-env",
            "COCO_CODEX_REMOTE_CAPABILITY_TOKEN",
        ])
        .arg("-C")
        .arg(&target.worktree)
        .current_dir(&target.worktree)
        .env(
            "COCO_CODEX_REMOTE_CAPABILITY_TOKEN",
            &target.capability_token,
        )
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    command
}
