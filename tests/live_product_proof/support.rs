use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use tokio::process::{Child, Command};
use tokio::time::{sleep, timeout};

use crate::mcp_client::McpClient;

const SUPPORTED_CODEX_VERSION: &str = "codex-cli 0.154.0";
const PROOF_TIMEOUT: Duration = Duration::from_secs(240);
const POLL_INTERVAL: Duration = Duration::from_millis(250);

pub(super) struct TestPaths {
    home: PathBuf,
    codex_home: PathBuf,
    pub(super) data_dir: PathBuf,
    database: PathBuf,
    socket: PathBuf,
    endpoint: PathBuf,
    token: PathBuf,
    worktrees: PathBuf,
}

impl TestPaths {
    pub(super) fn new(root: &Path) -> Self {
        let data_dir = root.join("data");
        let runtime = root.join("runtime");
        Self {
            home: root.join("home"),
            codex_home: root.join("codex-home"),
            database: data_dir.join("coco.db"),
            socket: runtime.join("cocod.sock"),
            endpoint: runtime.join("codex-app-server.json"),
            token: runtime.join("codex-app-server.token"),
            worktrees: root.join("worktrees"),
            data_dir,
        }
    }

    fn apply(&self, command: &mut Command, codex_binary: &Path) {
        command
            .env("HOME", &self.home)
            .env("CODEX_HOME", &self.codex_home)
            .env("COCO_CODEX_BINARY", codex_binary)
            .env("COCO_DATA_DIR", &self.data_dir)
            .env("COCO_DATABASE_PATH", &self.database)
            .env("COCO_SOCKET_PATH", &self.socket)
            .env("COCO_CODEX_ENDPOINT_PATH", &self.endpoint)
            .env("COCO_CODEX_TOKEN_PATH", &self.token)
            .env("COCO_WORKTREES_DIR", &self.worktrees)
            .env("RUST_LOG", "warn");
    }
}

pub(super) fn prepare_private_home(paths: &TestPaths, source_codex_home: &Path) -> Result<()> {
    fs::create_dir_all(&paths.home)?;
    fs::create_dir_all(&paths.codex_home)?;
    fs::create_dir_all(&paths.data_dir)?;
    let source = source_codex_home.join("auth.json");
    ensure!(source.is_file(), "the selected Codex home has no auth.json");
    let destination = paths.codex_home.join("auth.json");
    fs::copy(source, &destination)?;
    fs::set_permissions(destination, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

pub(super) fn prepare_repository(repository: &Path) -> Result<()> {
    fs::create_dir_all(repository)?;
    run_git(repository, &["init", "--initial-branch=main", "."])?;
    fs::write(repository.join("README.md"), "# CoCo live product proof\n")?;
    run_git(repository, &["add", "README.md"])?;
    run_git(
        repository,
        &[
            "-c",
            "user.name=CoCo Test",
            "-c",
            "user.email=coco@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "initial",
        ],
    )
}

fn run_git(repository: &Path, arguments: &[&str]) -> Result<()> {
    let output = std::process::Command::new("git")
        .args(arguments)
        .current_dir(repository)
        .output()?;
    ensure!(
        output.status.success(),
        "git command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

pub(super) async fn verify_codex_version(paths: &TestPaths, codex_binary: &Path) -> Result<()> {
    let mut command = Command::new(codex_binary);
    paths.apply(&mut command, codex_binary);
    let output = timeout(PROOF_TIMEOUT, command.arg("--version").output()).await??;
    let actual = String::from_utf8(output.stdout)?.trim().to_owned();
    ensure!(
        actual == SUPPORTED_CODEX_VERSION,
        "unsupported Codex executable: expected {SUPPORTED_CODEX_VERSION:?}, received {actual:?}"
    );
    Ok(())
}

pub(super) fn spawn_daemon(paths: &TestPaths, codex_binary: &Path, log: &Path) -> Result<Child> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cocod"));
    paths.apply(&mut command, codex_binary);
    command
        .stdout(Stdio::null())
        .stderr(Stdio::from(fs::File::create(log)?))
        .kill_on_drop(true)
        .spawn()
        .context("could not start cocod")
}

pub(super) async fn wait_for_daemon(
    paths: &TestPaths,
    daemon: &mut Child,
    log: &Path,
) -> Result<()> {
    let deadline = Instant::now() + PROOF_TIMEOUT;
    loop {
        if paths.socket.exists() && paths.endpoint.exists() {
            return Ok(());
        }
        if let Some(status) = daemon.try_wait()? {
            bail!("cocod exited with {status}: {}", read_log(log));
        }
        if Instant::now() >= deadline {
            bail!("cocod did not become ready: {}", read_log(log));
        }
        sleep(POLL_INTERVAL).await;
    }
}

pub(super) async fn run_cli(
    paths: &TestPaths,
    codex_binary: &Path,
    repository: &Path,
    arguments: &[&str],
) -> Result<Output> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_coco"));
    paths.apply(&mut command, codex_binary);
    command
        .args(arguments)
        .current_dir(repository)
        .kill_on_drop(true);
    let output = timeout(PROOF_TIMEOUT, command.output())
        .await
        .with_context(|| format!("coco {} timed out", safe_cli_label(arguments)))??;
    ensure!(
        output.status.success(),
        "coco {} failed: {}",
        safe_cli_label(arguments),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output)
}

fn safe_cli_label(arguments: &[&str]) -> String {
    arguments.first().copied().unwrap_or("command").to_owned()
}

pub(super) async fn workspace_status(
    paths: &TestPaths,
    codex_binary: &Path,
    repository: &Path,
    workspace: &str,
) -> Result<Value> {
    let output = run_cli(
        paths,
        codex_binary,
        repository,
        &["status", workspace, "--json"],
    )
    .await?;
    serde_json::from_slice(&output.stdout).context("coco status did not return JSON")
}

pub(super) async fn wait_for_idle(
    paths: &TestPaths,
    codex_binary: &Path,
    repository: &Path,
    workspace: &str,
) -> Result<Value> {
    let deadline = Instant::now() + PROOF_TIMEOUT;
    loop {
        let status = workspace_status(paths, codex_binary, repository, workspace).await?;
        match status.pointer("/workspace/phase").and_then(Value::as_str) {
            Some("idle") => return Ok(status),
            Some("waiting_for_approval" | "waiting_for_input" | "system_error" | "failed") => {
                bail!("live proof workspace needs intervention: {status}")
            }
            _ if Instant::now() >= deadline => {
                bail!("live proof workspace did not become idle: {status}")
            }
            _ => sleep(POLL_INTERVAL).await,
        }
    }
}

pub(super) fn assert_bound_workspace(status: &Value, name: &str) -> Result<()> {
    ensure!(status.pointer("/workspace/name") == Some(&json!(name)));
    ensure!(
        status
            .pointer("/workspace/codexThreadId")
            .and_then(Value::as_str)
            .is_some(),
        "workspace has no native Codex thread"
    );
    ensure!(
        status
            .pointer("/workspace/worktreePath")
            .and_then(Value::as_str)
            .is_some(),
        "workspace has no worktree"
    );
    Ok(())
}

pub(super) fn assert_same_binding(before: &Value, after: &Value) -> Result<()> {
    ensure!(
        before.pointer("/workspace/id") == after.pointer("/workspace/id")
            && before.pointer("/workspace/codexThreadId")
                == after.pointer("/workspace/codexThreadId")
            && before.pointer("/workspace/worktreePath")
                == after.pointer("/workspace/worktreePath"),
        "coordinator restart changed a workspace binding"
    );
    Ok(())
}

pub(super) fn assert_same_turn(first: &Value, second: &Value) -> Result<()> {
    ensure!(
        first
            .pointer("/codexTurnId")
            .and_then(Value::as_str)
            .is_some()
            && first.pointer("/codexTurnId") == second.pointer("/codexTurnId")
            && first.pointer("/workspace/id") == second.pointer("/workspace/id"),
        "operation replay did not resolve to the same native turn"
    );
    Ok(())
}

pub(super) async fn spawn_mcp_writer(
    paths: &TestPaths,
    codex_binary: &Path,
    repository: &Path,
) -> Result<McpClient> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_coco-mcp"));
    paths.apply(&mut command, codex_binary);
    command
        .arg("--repository")
        .arg(repository)
        .arg("--allow-send");
    McpClient::spawn(&mut command).await
}

pub(super) async fn stop_daemon(daemon: &mut Child, log: &Path) -> Result<()> {
    let pid = daemon.id().context("cocod had no process ID")?;
    let output = Command::new("kill")
        .args(["-INT", &pid.to_string()])
        .output()
        .await?;
    ensure!(output.status.success(), "could not interrupt cocod");
    let status = timeout(PROOF_TIMEOUT, daemon.wait()).await??;
    ensure!(
        status.success(),
        "cocod exited with {status}: {}",
        read_log(log)
    );
    Ok(())
}

fn read_log(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| format!("could not read log: {error}"))
}
