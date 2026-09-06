#![cfg(unix)]

use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use tokio::process::{Child, Command};
use tokio::time::{sleep, timeout};

const COMPATIBILITY_TIMEOUT: Duration = Duration::from_secs(20);
const POLL_INTERVAL: Duration = Duration::from_millis(25);
const SUPPORTED_CODEX_VERSION: &str = "codex-cli 0.147.0";
const OPT_IN_ENV: &str = "COCO_RUN_REAL_CODEX_COMPAT";
const CODEX_BINARY_ENV: &str = "COCO_REAL_CODEX_BINARY";
const WORKSPACE_NAME: &str = "real-codex-compat";

struct TestPaths {
    home: PathBuf,
    codex_home: PathBuf,
    data_dir: PathBuf,
    database: PathBuf,
    socket: PathBuf,
    endpoint: PathBuf,
    token: PathBuf,
    worktrees: PathBuf,
}

impl TestPaths {
    fn new(root: &Path) -> Self {
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires COCO_RUN_REAL_CODEX_COMPAT=1 and the pinned local Codex executable"]
async fn installed_codex_matches_the_pinned_start_and_resume_contract() -> Result<()> {
    require_explicit_opt_in()?;
    let codex_binary = env::var_os(CODEX_BINARY_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("codex"));
    let temporary = tempfile::tempdir()?;
    let paths = TestPaths::new(temporary.path());
    fs::create_dir_all(&paths.home)?;
    fs::create_dir_all(&paths.codex_home)?;
    fs::create_dir_all(&paths.data_dir)?;

    verify_codex_version(&codex_binary, &paths).await?;
    verify_generated_schemas(&codex_binary, &paths, temporary.path()).await?;

    let repository = temporary.path().join("repository");
    prepare_repository(&repository)?;
    let first = run_daemon_lifecycle(&paths, &codex_binary, &repository, "first").await?;
    let second = run_daemon_lifecycle(&paths, &codex_binary, &repository, "second").await?;

    assert_same_persisted_thread(&first, &second)?;
    Ok(())
}

fn require_explicit_opt_in() -> Result<()> {
    ensure!(
        env::var(OPT_IN_ENV).as_deref() == Ok("1"),
        "set {OPT_IN_ENV}=1 in addition to passing --ignored"
    );
    Ok(())
}

async fn verify_codex_version(codex_binary: &Path, paths: &TestPaths) -> Result<()> {
    let mut command = Command::new(codex_binary);
    command
        .arg("--version")
        .env("HOME", &paths.home)
        .env("CODEX_HOME", &paths.codex_home);
    let output = run_codex_command(command, "read the Codex version").await?;
    let actual = String::from_utf8(output.stdout)?.trim().to_owned();
    ensure!(
        actual == SUPPORTED_CODEX_VERSION,
        "unsupported Codex executable: expected {SUPPORTED_CODEX_VERSION:?}, received {actual:?}"
    );
    Ok(())
}

async fn verify_generated_schemas(
    codex_binary: &Path,
    paths: &TestPaths,
    root: &Path,
) -> Result<()> {
    let generated = root.join("generated-schema");
    let mut command = Command::new(codex_binary);
    command
        .args([
            "app-server",
            "generate-json-schema",
            "--experimental",
            "--out",
        ])
        .arg(&generated)
        .env("HOME", &paths.home)
        .env("CODEX_HOME", &paths.codex_home);
    run_codex_command(command, "generate the App Server schemas").await?;

    let committed = Path::new(env!("CARGO_MANIFEST_DIR")).join("schema/codex-app-server");
    for relative in [
        "CommandExecutionRequestApprovalParams.json",
        "CommandExecutionRequestApprovalResponse.json",
        "FileChangeRequestApprovalParams.json",
        "FileChangeRequestApprovalResponse.json",
        "ToolRequestUserInputParams.json",
        "ToolRequestUserInputResponse.json",
        "v2/FileChangePatchUpdatedNotification.json",
        "v2/ItemStartedNotification.json",
        "v2/ThreadStartParams.json",
        "v2/ThreadStartResponse.json",
        "v2/ThreadForkParams.json",
        "v2/ThreadForkResponse.json",
        "v2/ThreadResumeParams.json",
        "v2/ThreadResumeResponse.json",
        "v2/ThreadSetNameParams.json",
        "v2/ThreadSetNameResponse.json",
        "v2/TurnStartParams.json",
        "v2/TurnStartResponse.json",
        "v2/ServerRequestResolvedNotification.json",
        "v2/TurnCompletedNotification.json",
    ] {
        let actual = fs::read(generated.join(relative))?;
        let expected = fs::read(committed.join(relative))?;
        ensure!(
            actual == expected,
            "installed Codex generated a different {relative} schema"
        );
    }
    Ok(())
}

async fn run_codex_command(mut command: Command, operation: &str) -> Result<Output> {
    let output = timeout(COMPATIBILITY_TIMEOUT, command.output())
        .await
        .with_context(|| format!("timed out while trying to {operation}"))??;
    ensure!(
        output.status.success(),
        "could not {operation}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output)
}

async fn run_daemon_lifecycle(
    paths: &TestPaths,
    codex_binary: &Path,
    repository: &Path,
    label: &str,
) -> Result<Value> {
    let log = paths.data_dir.join(format!("cocod-{label}.log"));
    let mut daemon = spawn_daemon(paths, codex_binary, &log)?;
    wait_for_file(&paths.socket, &mut daemon, &log).await?;
    verify_private_runtime(paths)?;

    if label == "first" {
        run_cli(paths, codex_binary, repository, &["repo", "add", "."])
            .await
            .with_context(|| format!("cocod log:\n{}", read_log(&log)))?;
        run_cli(
            paths,
            codex_binary,
            repository,
            &["create", WORKSPACE_NAME, "--base", "HEAD"],
        )
        .await
        .with_context(|| format!("cocod log:\n{}", read_log(&log)))?;
    }
    let status = workspace_status(paths, codex_binary, repository)
        .await
        .with_context(|| format!("cocod log:\n{}", read_log(&log)))?;
    stop_daemon(&mut daemon, &log).await?;
    verify_runtime_cleanup(paths)?;
    Ok(status)
}

fn spawn_daemon(paths: &TestPaths, codex_binary: &Path, log: &Path) -> Result<Child> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cocod"));
    paths.apply(&mut command, codex_binary);
    command
        .stdout(Stdio::null())
        .stderr(Stdio::from(fs::File::create(log)?))
        .kill_on_drop(true)
        .spawn()
        .context("could not start cocod")
}

async fn run_cli(
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
    let output = timeout(COMPATIBILITY_TIMEOUT, command.output())
        .await
        .with_context(|| format!("coco {} timed out", arguments.join(" ")))??;
    ensure!(
        output.status.success(),
        "coco {} failed:\nstdout: {}\nstderr: {}",
        arguments.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output)
}

async fn workspace_status(
    paths: &TestPaths,
    codex_binary: &Path,
    repository: &Path,
) -> Result<Value> {
    let output = run_cli(
        paths,
        codex_binary,
        repository,
        &["status", WORKSPACE_NAME, "--json"],
    )
    .await?;
    serde_json::from_slice(&output.stdout).context("coco status did not return JSON")
}

async fn wait_for_file(path: &Path, daemon: &mut Child, log: &Path) -> Result<()> {
    let deadline = Instant::now() + COMPATIBILITY_TIMEOUT;
    loop {
        if path.exists() {
            return Ok(());
        }
        if let Some(status) = daemon.try_wait()? {
            bail!("cocod exited with {status}: {}", read_log(log));
        }
        if Instant::now() >= deadline {
            bail!(
                "timed out waiting for {}: {}",
                path.display(),
                read_log(log)
            );
        }
        sleep(POLL_INTERVAL).await;
    }
}

async fn stop_daemon(daemon: &mut Child, log: &Path) -> Result<()> {
    let pid = daemon.id().context("cocod had no process id")?;
    let output = Command::new("kill")
        .args(["-INT", &pid.to_string()])
        .output()
        .await
        .context("could not send SIGINT to cocod")?;
    ensure!(
        output.status.success(),
        "could not interrupt cocod: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let status = timeout(COMPATIBILITY_TIMEOUT, daemon.wait())
        .await
        .context("cocod did not stop after SIGINT")??;
    ensure!(
        status.success(),
        "cocod exited with {status}: {}",
        read_log(log)
    );
    Ok(())
}

fn verify_private_runtime(paths: &TestPaths) -> Result<()> {
    let descriptor = fs::read_to_string(&paths.endpoint)?;
    let descriptor = serde_json::from_str::<Value>(&descriptor)?;
    ensure!(
        descriptor["url"]
            .as_str()
            .is_some_and(|url| url.starts_with("ws://127.0.0.1:")),
        "App Server endpoint was not IPv4-loopback WebSocket"
    );
    let token = fs::read_to_string(&paths.token)?;
    ensure!(
        token.len() == 64 && token.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "App Server token was not a 64-character hexadecimal capability"
    );
    for path in [
        &paths.endpoint,
        &paths.token,
        &paths.socket,
        &paths.database,
    ] {
        let mode = fs::metadata(path)?.permissions().mode() & 0o777;
        ensure!(mode == 0o600, "{} had mode {mode:o}", path.display());
    }
    Ok(())
}

fn verify_runtime_cleanup(paths: &TestPaths) -> Result<()> {
    for path in [&paths.endpoint, &paths.token, &paths.socket] {
        ensure!(
            !path.exists(),
            "runtime path remained after shutdown: {}",
            path.display()
        );
    }
    Ok(())
}

fn read_log(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| format!("could not read log: {error}"))
}

fn assert_same_persisted_thread(first: &Value, second: &Value) -> Result<()> {
    for status in [first, second] {
        ensure!(
            status["workspace"]["lifecycle"] == "ready",
            "workspace was not ready: {status}"
        );
        ensure!(
            status["workspace"]["phase"] == "idle",
            "thread was not idle: {status}"
        );
        ensure!(
            status["workspace"]["threadRuntime"]["isFresh"] == true,
            "thread status was stale: {status}"
        );
        ensure!(
            status["workspace"]["activeTurnId"].is_null(),
            "compatibility smoke unexpectedly created a model turn: {status}"
        );
    }
    ensure!(
        first["workspace"]["codexThreadId"] == second["workspace"]["codexThreadId"],
        "daemon restart changed the Codex thread"
    );
    ensure!(
        first["workspace"]["worktreePath"] == second["workspace"]["worktreePath"],
        "daemon restart changed the workspace worktree"
    );
    ensure!(
        first["workspace"]["threadRuntime"]["runtimeGeneration"]
            != second["workspace"]["threadRuntime"]["runtimeGeneration"],
        "daemon restart did not refresh the runtime generation"
    );
    Ok(())
}

fn prepare_repository(repository: &Path) -> Result<()> {
    fs::create_dir_all(repository)?;
    run_git(repository, &["init", "--initial-branch=main", "."])?;
    fs::write(repository.join("README.md"), "# real Codex compatibility\n")?;
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
    )?;
    Ok(())
}

fn run_git(repository: &Path, arguments: &[&str]) -> Result<()> {
    let output = std::process::Command::new("git")
        .args(arguments)
        .current_dir(repository)
        .output()
        .context("could not run Git for the compatibility test")?;
    ensure!(
        output.status.success(),
        "git {} failed: {}",
        arguments.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
