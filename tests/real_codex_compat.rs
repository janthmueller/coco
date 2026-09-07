#![cfg(unix)]

use std::collections::VecDeque;
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::UnixStream;
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::time::{sleep, timeout};

const COMPATIBILITY_TIMEOUT: Duration = Duration::from_secs(20);
const POLL_INTERVAL: Duration = Duration::from_millis(25);
const SUPPORTED_CODEX_VERSION: &str = "codex-cli 0.147.0";
const OPT_IN_ENV: &str = "COCO_RUN_REAL_CODEX_COMPAT";
const CODEX_BINARY_ENV: &str = "COCO_REAL_CODEX_BINARY";
const WORKSPACE_NAME: &str = "real-codex-compat";
const HISTORY_COMMAND: &str = "printf coco-native-history";

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

struct BoundThread {
    id: String,
    cwd: PathBuf,
    name: String,
}

struct RealAppServer {
    child: Child,
    stdin: ChildStdin,
    stdout: Lines<BufReader<ChildStdout>>,
    pending_frames: VecDeque<Value>,
    next_id: u64,
    log: PathBuf,
}

impl RealAppServer {
    async fn spawn(
        paths: &TestPaths,
        codex_binary: &Path,
        repository: &Path,
        label: &str,
    ) -> Result<Self> {
        let log = paths.data_dir.join(format!("app-server-{label}.log"));
        let mut command = Command::new(codex_binary);
        command
            .args(["app-server", "--listen", "stdio://"])
            .env("HOME", &paths.home)
            .env("CODEX_HOME", &paths.codex_home)
            .env("RUST_LOG", "warn")
            .current_dir(repository)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(fs::File::create(&log)?))
            .kill_on_drop(true);
        let mut child = command
            .spawn()
            .context("could not start the real App Server")?;
        let stdin = child.stdin.take().context("App Server had no stdin")?;
        let stdout = child.stdout.take().context("App Server had no stdout")?;
        let mut server = Self {
            child,
            stdin,
            stdout: BufReader::new(stdout).lines(),
            pending_frames: VecDeque::new(),
            next_id: 1,
            log,
        };
        server
            .request(
                "initialize",
                json!({
                    "clientInfo": {
                        "name": "coco-real-compat",
                        "version": env!("CARGO_PKG_VERSION"),
                    }
                }),
            )
            .await?;
        server
            .notify("initialized", json!({}))
            .await
            .context("could not finish App Server initialization")?;
        Ok(server)
    }

    async fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({"id": id, "method": method, "params": params}))
            .await?;
        loop {
            let frame = self.next_frame(method).await?;
            if frame.get("id") == Some(&json!(id)) {
                ensure!(
                    frame.get("error").is_none(),
                    "App Server rejected {method}: {frame}"
                );
                return frame
                    .get("result")
                    .cloned()
                    .with_context(|| format!("App Server returned no result for {method}"));
            }
            self.pending_frames.push_back(frame);
        }
    }

    async fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        self.send(json!({"method": method, "params": params})).await
    }

    async fn wait_for_thread_notification(
        &mut self,
        method: &str,
        thread_id: &str,
    ) -> Result<Value> {
        if let Some(index) = self
            .pending_frames
            .iter()
            .position(|frame| is_thread_notification(frame, method, thread_id))
        {
            return self
                .pending_frames
                .remove(index)
                .context("queued App Server notification disappeared");
        }
        loop {
            let frame = self.next_frame(method).await?;
            if is_thread_notification(&frame, method, thread_id) {
                return Ok(frame);
            }
            self.pending_frames.push_back(frame);
        }
    }

    async fn send(&mut self, frame: Value) -> Result<()> {
        let mut encoded = serde_json::to_vec(&frame)?;
        encoded.push(b'\n');
        self.stdin.write_all(&encoded).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    async fn next_frame(&mut self, operation: &str) -> Result<Value> {
        let line = timeout(COMPATIBILITY_TIMEOUT, self.stdout.next_line())
            .await
            .with_context(|| format!("timed out waiting for App Server {operation}"))??
            .with_context(|| {
                format!(
                    "App Server exited while waiting for {operation}: {}",
                    read_log(&self.log)
                )
            })?;
        serde_json::from_str(&line)
            .with_context(|| format!("App Server emitted invalid JSON: {line}"))
    }

    async fn stop(mut self) -> Result<()> {
        let _ = self.stdin.shutdown().await;
        drop(self.stdin);
        match timeout(Duration::from_secs(2), self.child.wait()).await {
            Ok(waited) => {
                waited.context("could not wait for the real App Server")?;
            }
            Err(_) => {
                self.child.start_kill()?;
                self.child.wait().await?;
            }
        }
        Ok(())
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
    let (first, _) =
        run_daemon_lifecycle(&paths, &codex_binary, &repository, "first", false).await?;
    verify_native_read_contracts(&paths, &codex_binary, &repository, &first).await?;
    let (second, loaded) =
        run_daemon_lifecycle(&paths, &codex_binary, &repository, "second", true).await?;
    let loaded = loaded.context("restart lifecycle did not exercise on-demand loading")?;

    assert_same_persisted_thread(&first, &second, &loaded)?;
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
        "v2/ItemCompletedNotification.json",
        "v2/ItemStartedNotification.json",
        "v2/ModelListParams.json",
        "v2/ModelListResponse.json",
        "v2/ThreadListParams.json",
        "v2/ThreadListResponse.json",
        "v2/ThreadLoadedListParams.json",
        "v2/ThreadLoadedListResponse.json",
        "v2/ThreadReadParams.json",
        "v2/ThreadReadResponse.json",
        "v2/ThreadCompactStartParams.json",
        "v2/ThreadCompactStartResponse.json",
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

async fn verify_native_read_contracts(
    paths: &TestPaths,
    codex_binary: &Path,
    repository: &Path,
    status: &Value,
) -> Result<()> {
    let bound = bound_thread(status)?;
    let mut first =
        RealAppServer::spawn(paths, codex_binary, repository, "native-read-first").await?;

    assert_loaded_state(&mut first, &bound.id, false).await?;
    let summary = read_thread(&mut first, &bound.id, false).await?;
    assert_native_projection(&summary, &bound, "notLoaded")?;
    assert_pinned_prepared_thread_source(&summary)?;
    assert_empty_turn_projection(&summary)?;
    assert_loaded_state(&mut first, &bound.id, false).await?;
    assert_thread_list_limitation(&mut first, &bound).await?;

    first
        .request("thread/resume", json!({"threadId": &bound.id}))
        .await?;
    assert_loaded_state(&mut first, &bound.id, true).await?;
    // A user shell turn gives the empty prepared thread durable turn history
    // without contacting or consuming a model.
    first
        .request(
            "thread/shellCommand",
            json!({"threadId": &bound.id, "command": HISTORY_COMMAND}),
        )
        .await?;
    let completed = first
        .wait_for_thread_notification("turn/completed", &bound.id)
        .await?;
    let turn_id = completed
        .pointer("/params/turn/id")
        .and_then(Value::as_str)
        .context("turn/completed did not contain a turn id")?
        .to_owned();
    ensure!(
        completed.pointer("/params/turn/status") == Some(&json!("completed")),
        "model-free history command did not complete: {completed}"
    );

    let summary = read_thread(&mut first, &bound.id, false).await?;
    assert_empty_turn_projection(&summary)?;
    let history = read_thread(&mut first, &bound.id, true).await?;
    assert_persisted_turn(&history, &turn_id)?;
    first.stop().await?;

    verify_read_after_app_server_restart(paths, codex_binary, repository, &bound, &turn_id).await
}

async fn verify_read_after_app_server_restart(
    paths: &TestPaths,
    codex_binary: &Path,
    repository: &Path,
    bound: &BoundThread,
    turn_id: &str,
) -> Result<()> {
    let mut restarted =
        RealAppServer::spawn(paths, codex_binary, repository, "native-read-restarted").await?;
    assert_loaded_state(&mut restarted, &bound.id, false).await?;

    let summary = read_thread(&mut restarted, &bound.id, false).await?;
    assert_native_projection(&summary, bound, "notLoaded")?;
    assert_empty_turn_projection(&summary)?;
    let history = read_thread(&mut restarted, &bound.id, true).await?;
    assert_persisted_turn(&history, turn_id)?;

    assert_loaded_state(&mut restarted, &bound.id, false).await?;
    assert_thread_list_limitation(&mut restarted, bound).await?;
    restarted.stop().await
}

fn bound_thread(status: &Value) -> Result<BoundThread> {
    let workspace = status
        .get("workspace")
        .context("coco status did not contain a workspace")?;
    Ok(BoundThread {
        id: workspace["codexThreadId"]
            .as_str()
            .context("workspace had no Codex thread id")?
            .to_owned(),
        cwd: PathBuf::from(
            workspace["worktreePath"]
                .as_str()
                .context("workspace had no worktree path")?,
        ),
        name: workspace["name"]
            .as_str()
            .context("workspace had no name")?
            .to_owned(),
    })
}

async fn read_thread(
    server: &mut RealAppServer,
    thread_id: &str,
    include_turns: bool,
) -> Result<Value> {
    let result = server
        .request(
            "thread/read",
            json!({"threadId": thread_id, "includeTurns": include_turns}),
        )
        .await?;
    result
        .get("thread")
        .cloned()
        .context("thread/read response had no thread")
}

fn assert_native_projection(thread: &Value, bound: &BoundThread, status: &str) -> Result<()> {
    ensure!(
        thread["id"].as_str() == Some(bound.id.as_str()),
        "thread/read changed the binding: {thread}"
    );
    ensure!(
        thread["cwd"].as_str() == bound.cwd.to_str(),
        "thread/read returned the wrong cwd: {thread}"
    );
    ensure!(
        thread["name"].as_str() == Some(bound.name.as_str()),
        "thread/read did not hydrate the native name: {thread}"
    );
    ensure!(
        thread.pointer("/status/type") == Some(&json!(status)),
        "thread/read returned the wrong native status: {thread}"
    );
    Ok(())
}

fn assert_empty_turn_projection(thread: &Value) -> Result<()> {
    ensure!(
        thread["turns"]
            .as_array()
            .is_some_and(|turns| turns.is_empty()),
        "thread/read without includeTurns unexpectedly hydrated history: {thread}"
    );
    Ok(())
}

fn assert_persisted_turn(thread: &Value, expected_turn_id: &str) -> Result<()> {
    let turns = thread["turns"]
        .as_array()
        .context("thread/read(includeTurns=true) returned no turns array")?;
    ensure!(
        turns.len() == 1,
        "thread/read did not hydrate the single model-free turn: {thread}"
    );
    let turn = &turns[0];
    ensure!(
        turn["id"].as_str() == Some(expected_turn_id),
        "thread/read returned the wrong persisted turn: {turn}"
    );
    ensure!(
        turn["status"] == "completed",
        "persisted model-free turn was not complete: {turn}"
    );
    ensure!(
        turn["items"].as_array().is_some(),
        "thread/read returned a turn without an items array: {turn}"
    );
    Ok(())
}

fn assert_pinned_prepared_thread_source(thread: &Value) -> Result<()> {
    ensure!(
        thread["source"] == "vscode",
        "pinned Codex changed the source assigned to an App-Server-created prepared thread: {thread}"
    );
    Ok(())
}

async fn assert_loaded_state(
    server: &mut RealAppServer,
    thread_id: &str,
    expected: bool,
) -> Result<()> {
    let result = server.request("thread/loaded/list", json!({})).await?;
    let loaded = result["data"]
        .as_array()
        .context("thread/loaded/list returned no data array")?
        .iter()
        .any(|id| id.as_str() == Some(thread_id));
    ensure!(
        loaded == expected,
        "thread/loaded/list expectation was {expected}, received {result}"
    );
    Ok(())
}

async fn assert_thread_list_limitation(
    server: &mut RealAppServer,
    bound: &BoundThread,
) -> Result<()> {
    let cwd = bound
        .cwd
        .to_str()
        .context("compatibility worktree path was not UTF-8")?;
    let app_server = server
        .request(
            "thread/list",
            json!({"cwd": cwd, "sourceKinds": ["appServer"]}),
        )
        .await?;
    ensure_thread_absent(&app_server, &bound.id, "appServer source")?;

    let reported_source = server
        .request(
            "thread/list",
            json!({"cwd": cwd, "sourceKinds": ["vscode"]}),
        )
        .await?;
    ensure_thread_absent(&reported_source, &bound.id, "exact-cwd vscode source")?;

    let unscoped = server
        .request("thread/list", json!({"sourceKinds": ["vscode"]}))
        .await?;
    ensure_thread_absent(&unscoped, &bound.id, "unscoped vscode source")
}

fn ensure_thread_absent(result: &Value, thread_id: &str, filter: &str) -> Result<()> {
    let data = result["data"]
        .as_array()
        .context("thread/list returned no data array")?;
    ensure!(
        data.iter().all(|thread| thread["id"] != thread_id),
        "pinned Codex unexpectedly listed a prepared thread for {filter}; revisit the exact-binding-first limitation: {result}"
    );
    Ok(())
}

fn is_thread_notification(frame: &Value, method: &str, thread_id: &str) -> bool {
    frame["method"] == method
        && frame.pointer("/params/threadId").and_then(Value::as_str) == Some(thread_id)
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
    attach_after_status: bool,
) -> Result<(Value, Option<Value>)> {
    let log = paths.data_dir.join(format!("cocod-{label}.log"));
    let mut daemon = spawn_daemon(paths, codex_binary, &log)?;
    wait_for_file(&paths.socket, &mut daemon, &log).await?;
    verify_private_runtime(paths)?;

    if label == "first" {
        run_cli(paths, codex_binary, repository, &["repo", "add", "."])
            .await
            .with_context(|| format!("cocod log:\n{}", read_log(&log)))?;
        let models = run_cli(
            paths,
            codex_binary,
            repository,
            &["model", "list", "--json"],
        )
        .await
        .with_context(|| format!("cocod log:\n{}", read_log(&log)))?;
        let models = serde_json::from_slice::<Value>(&models.stdout)
            .context("coco model list did not return JSON")?;
        let model = select_default_model(&models)?;
        run_cli(
            paths,
            codex_binary,
            repository,
            &[
                "create",
                WORKSPACE_NAME,
                "--base",
                "HEAD",
                "--model",
                &model,
            ],
        )
        .await
        .with_context(|| format!("cocod log:\n{}", read_log(&log)))?;
    }
    let status = workspace_status(paths, codex_binary, repository)
        .await
        .with_context(|| format!("cocod log:\n{}", read_log(&log)))?;
    let loaded = if attach_after_status {
        attach_workspace(paths, repository)
            .await
            .with_context(|| format!("cocod log:\n{}", read_log(&log)))?;
        Some(
            workspace_status(paths, codex_binary, repository)
                .await
                .with_context(|| format!("cocod log:\n{}", read_log(&log)))?,
        )
    } else {
        None
    };
    stop_daemon(&mut daemon, &log).await?;
    verify_runtime_cleanup(paths)?;
    Ok((status, loaded))
}

async fn attach_workspace(paths: &TestPaths, repository: &Path) -> Result<()> {
    let mut stream = UnixStream::connect(&paths.socket)
        .await
        .context("could not connect to cocod for workspace.attach")?;
    let mut request = serde_json::to_vec(&json!({
        "id": "real-codex-attach",
        "method": "workspace.attach",
        "params": {
            "scope": {"kind": "repository", "path": repository},
            "workspace": WORKSPACE_NAME,
        },
    }))?;
    request.push(b'\n');
    stream.write_all(&request).await?;
    stream.flush().await?;

    let mut response = String::new();
    timeout(
        COMPATIBILITY_TIMEOUT,
        BufReader::new(stream).read_line(&mut response),
    )
    .await
    .context("workspace.attach timed out")??;
    let response: Value =
        serde_json::from_str(&response).context("workspace.attach returned invalid daemon JSON")?;
    ensure!(
        response.get("error").is_none(),
        "workspace.attach failed: {response}"
    );
    ensure!(
        response.pointer("/result/workspace/phase") == Some(&json!("idle")),
        "workspace.attach did not load the native thread: {response}"
    );
    Ok(())
}

fn select_default_model(response: &Value) -> Result<String> {
    ensure!(
        response["schemaVersion"] == 5,
        "coco model list returned an unexpected schema version: {response}"
    );
    let models = response["models"]
        .as_array()
        .context("coco model list response did not contain a models array")?;
    let selected = models
        .iter()
        .find(|model| model["isDefault"] == true)
        .or_else(|| models.first())
        .context("the installed Codex executable advertised no visible models")?;
    selected["model"]
        .as_str()
        .filter(|model| !model.trim().is_empty())
        .map(ToOwned::to_owned)
        .context("the selected catalog entry did not contain a usable model value")
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

fn assert_same_persisted_thread(first: &Value, passive: &Value, loaded: &Value) -> Result<()> {
    for (status, expected_phase) in [(first, "idle"), (passive, "not_loaded"), (loaded, "idle")] {
        ensure!(
            status["workspace"]["lifecycle"] == "ready",
            "workspace was not ready: {status}"
        );
        ensure!(
            status["workspace"]["phase"] == expected_phase,
            "thread did not expose the expected native phase {expected_phase}: {status}"
        );
        ensure!(
            status["workspace"]["threadRuntime"]["isFresh"] == true,
            "thread status was stale: {status}"
        );
        ensure!(
            status["workspace"]["activeTurnId"].is_null(),
            "compatibility smoke unexpectedly created a model turn: {status}"
        );
        let model_override = status["workspace"]["profile"]["modelOverride"]
            .as_str()
            .context("workspace did not retain its explicit model override")?;
        ensure!(
            status["workspace"]["profile"]["effectiveSettings"]["model"] == model_override,
            "Codex did not report the requested model as effective: {status}"
        );
    }
    ensure!(
        first["workspace"]["codexThreadId"] == passive["workspace"]["codexThreadId"]
            && passive["workspace"]["codexThreadId"] == loaded["workspace"]["codexThreadId"],
        "daemon restart changed the Codex thread"
    );
    ensure!(
        first["workspace"]["worktreePath"] == passive["workspace"]["worktreePath"]
            && passive["workspace"]["worktreePath"] == loaded["workspace"]["worktreePath"],
        "daemon restart changed the workspace worktree"
    );
    ensure!(
        first["workspace"]["profile"]["modelOverride"]
            == passive["workspace"]["profile"]["modelOverride"]
            && passive["workspace"]["profile"]["modelOverride"]
                == loaded["workspace"]["profile"]["modelOverride"],
        "daemon restart changed the explicit model override"
    );
    ensure!(
        first["workspace"]["threadRuntime"]["runtimeGeneration"]
            != passive["workspace"]["threadRuntime"]["runtimeGeneration"],
        "daemon restart did not refresh the runtime generation"
    );
    ensure!(
        passive["workspace"]["threadRuntime"]["runtimeGeneration"]
            == loaded["workspace"]["threadRuntime"]["runtimeGeneration"],
        "on-demand load unexpectedly changed the daemon generation"
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
