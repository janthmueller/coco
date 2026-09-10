#![cfg(unix)]

use std::collections::VecDeque;
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::{TcpStream, UnixStream};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::time::{sleep, timeout};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;
use tokio_tungstenite::{WebSocketStream, client_async};

const COMPATIBILITY_TIMEOUT: Duration = Duration::from_secs(60);
const POLL_INTERVAL: Duration = Duration::from_millis(25);
const SUPPORTED_CODEX_VERSION: &str = "codex-cli 0.154.0";
const OPT_IN_ENV: &str = "COCO_RUN_REAL_CODEX_COMPAT";
const CODEX_BINARY_ENV: &str = "COCO_REAL_CODEX_BINARY";
const WORKSPACE_NAME: &str = "real-codex-compat";
const FORK_WORKSPACE_NAME: &str = "real-codex-context-fork";
const HISTORY_COMMAND: &str = "sleep 1; printf coco-native-history";

#[path = "real_codex_compat/mcp.rs"]
mod mcp;

#[path = "real_codex_compat/retirement.rs"]
mod retirement;

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

struct AttachLease {
    workspace_id: String,
    lease_id: String,
    cwd: PathBuf,
    execution_environment: Value,
}

struct RealAppServer {
    child: Child,
    stdin: ChildStdin,
    stdout: Lines<BufReader<ChildStdout>>,
    pending_frames: VecDeque<Value>,
    next_id: u64,
    log: PathBuf,
}

struct RemoteAppServer {
    websocket: WebSocketStream<TcpStream>,
    pending_frames: VecDeque<Value>,
    next_id: u64,
}

impl RemoteAppServer {
    async fn connect(paths: &TestPaths) -> Result<Self> {
        Self::connect_with_capabilities(paths, false).await
    }

    async fn connect_with_capabilities(paths: &TestPaths, experimental: bool) -> Result<Self> {
        let descriptor: Value = serde_json::from_slice(&fs::read(&paths.endpoint)?)?;
        let endpoint = descriptor["url"]
            .as_str()
            .context("cocod endpoint descriptor had no URL")?;
        let address = endpoint
            .strip_prefix("ws://")
            .context("cocod endpoint was not ws://")?
            .parse::<std::net::SocketAddr>()?;
        let token = fs::read_to_string(&paths.token)?;
        let mut request = endpoint.into_client_request()?;
        request
            .headers_mut()
            .insert(AUTHORIZATION, format!("Bearer {}", token.trim()).parse()?);
        let stream = TcpStream::connect(address).await?;
        let (websocket, _) = client_async(request, stream)
            .await
            .context("cocod's App Server rejected the compatibility client")?;
        let mut server = Self {
            websocket,
            pending_frames: VecDeque::new(),
            next_id: 1,
        };
        server
            .request(
                "initialize",
                json!({
                    "capabilities": {"experimentalApi": experimental},
                    "clientInfo": {
                        "name": "coco-real-remote-compat",
                        "version": env!("CARGO_PKG_VERSION"),
                    }
                }),
            )
            .await?;
        server.notify("initialized", json!({})).await?;
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
                    "remote App Server rejected {method}: {frame}"
                );
                return frame
                    .get("result")
                    .cloned()
                    .with_context(|| format!("remote App Server returned no result for {method}"));
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
                .context("queued remote App Server notification disappeared");
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
        self.websocket
            .send(Message::Text(serde_json::to_string(&frame)?.into()))
            .await?;
        Ok(())
    }

    async fn next_frame(&mut self, operation: &str) -> Result<Value> {
        loop {
            let message = timeout(COMPATIBILITY_TIMEOUT, self.websocket.next())
                .await
                .with_context(|| format!("timed out waiting for remote App Server {operation}"))?
                .context("remote App Server disconnected")??;
            match message {
                Message::Text(text) => return Ok(serde_json::from_str(&text)?),
                Message::Binary(bytes) => return Ok(serde_json::from_slice(&bytes)?),
                Message::Ping(payload) => self.websocket.send(Message::Pong(payload)).await?,
                Message::Pong(_) | Message::Frame(_) => {}
                Message::Close(_) => bail!("remote App Server closed during {operation}"),
            }
        }
    }

    async fn close(mut self, thread_id: &str) -> Result<()> {
        self.request("thread/unsubscribe", json!({"threadId": thread_id}))
            .await?;
        self.websocket.close(None).await?;
        Ok(())
    }
}

fn is_thread_notification(frame: &Value, method: &str, thread_id: &str) -> bool {
    frame["method"] == method
        && frame.pointer("/params/threadId").and_then(Value::as_str) == Some(thread_id)
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
async fn installed_codex_runs_native_session_hooks_through_app_server() -> Result<()> {
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

    let repository = temporary.path().join("repository");
    prepare_repository(&repository)?;
    let capture = temporary.path().join("native-session-start.json");
    let script = temporary.path().join("native-session-start-hook");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\nset -eu\nIFS= read -r payload || true\nprintf '%s\\n' \"$payload\" > {}\n",
            shell_word(&capture)
        ),
    )?;
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700))?;
    fs::write(
        paths.codex_home.join("hooks.json"),
        serde_json::to_vec(&json!({
            "hooks": {
                "SessionStart": [{
                    "matcher": "startup",
                    "hooks": [{
                        "type": "command",
                        "command": shell_word(&script),
                        "timeout": 5,
                    }],
                }],
            },
        }))?,
    )?;

    let mut server =
        RealAppServer::spawn(&paths, &codex_binary, &repository, "native-session-hook").await?;
    let started = server
        .request(
            "thread/start",
            json!({
                "cwd": &repository,
                "config": {
                    "bypass_hook_trust": true,
                    "model_provider": "hook-proof",
                    "model_providers": {
                        "hook-proof": {
                            "name": "Hook proof",
                            "base_url": "http://127.0.0.1:1/v1",
                            "wire_api": "responses",
                            "requires_openai_auth": false,
                            "supports_websockets": false,
                        },
                    },
                },
                "ephemeral": false,
                "model": "hook-proof-model",
            }),
        )
        .await?;
    let thread_id = started
        .pointer("/thread/id")
        .and_then(Value::as_str)
        .context("thread/start returned no thread id")?;
    server
        .request(
            "turn/start",
            json!({
                "threadId": thread_id,
                "cwd": &repository,
                "input": [{"type": "text", "text": "Run the hook compatibility proof."}],
            }),
        )
        .await?;
    let deadline = Instant::now() + COMPATIBILITY_TIMEOUT;
    while !capture.exists() && Instant::now() < deadline {
        sleep(POLL_INTERVAL).await;
    }
    let event: Value = serde_json::from_slice(&fs::read(&capture).with_context(|| {
        format!(
            "native SessionStart hook did not write {}; App Server log:\n{}",
            capture.display(),
            read_log(&server.log)
        )
    })?)?;
    ensure!(
        event["hook_event_name"] == "SessionStart"
            && event["source"] == "startup"
            && event["session_id"] == thread_id
            && event["cwd"].as_str() == repository.to_str(),
        "native SessionStart hook received an unexpected event: {event}"
    );
    server.stop().await
}

fn shell_word(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires COCO_RUN_REAL_CODEX_COMPAT=1 and the pinned local Codex executable"]
async fn installed_codex_matches_the_pinned_preparation_adoption_and_resume_contract() -> Result<()>
{
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

    let repository = temporary.path().join("repository");
    prepare_repository(&repository)?;
    let (first, _) =
        run_daemon_lifecycle(&paths, &codex_binary, &repository, "first", false).await?;
    assert_workspace_executor_stopped(&first)?;
    verify_native_read_contracts(&paths, &codex_binary, &repository, &first).await?;
    let (second, loaded) =
        run_daemon_lifecycle(&paths, &codex_binary, &repository, "second", true).await?;
    let loaded = loaded.context("restart lifecycle did not exercise on-demand loading")?;
    assert_workspace_executor_stopped(&loaded)?;

    assert_same_persisted_thread(&first, &second, &loaded)?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn assert_workspace_executor_stopped(status: &Value) -> Result<()> {
    let Some(pid) = status
        .pointer("/runtimeResources/processId")
        .and_then(Value::as_u64)
    else {
        return Ok(());
    };
    ensure!(
        !Path::new("/proc").join(pid.to_string()).exists(),
        "workspace exec-server process {pid} survived cocod shutdown"
    );
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn assert_workspace_executor_stopped(_status: &Value) -> Result<()> {
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
    assert_empty_turn_projection(&summary)?;
    assert_loaded_state(&mut first, &bound.id, false).await?;

    let history = read_thread(&mut first, &bound.id, true).await?;
    let turn_id = assert_persisted_shell_turn(&history)?;

    first
        .request("thread/resume", json!({"threadId": &bound.id}))
        .await?;
    assert_loaded_state(&mut first, &bound.id, true).await?;

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

fn assert_persisted_shell_turn(thread: &Value) -> Result<String> {
    let turns = thread["turns"]
        .as_array()
        .context("thread/read(includeTurns=true) returned no turns array")?;
    ensure!(
        turns.len() == 1,
        "thread/read did not hydrate the single model-free turn: {thread}"
    );
    let turn = &turns[0];
    let turn_id = turn["id"]
        .as_str()
        .context("persisted model-free turn had no id")?
        .to_owned();
    assert_persisted_turn(thread, &turn_id)?;
    Ok(turn_id)
}

fn assert_persisted_turn(thread: &Value, expected_turn_id: &str) -> Result<()> {
    let turns = thread["turns"]
        .as_array()
        .context("thread/read(includeTurns=true) returned no turns array")?;
    ensure!(
        turns.len() == 1,
        "thread/read did not retain exactly one model-free turn: {thread}"
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
        let prepared = workspace_status(paths, codex_binary, repository).await?;
        assert_prepared_workspace(&prepared)?;
        materialize_workspace_through_remote_action(paths, codex_binary, repository, &model)
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
    if label == "second" {
        verify_context_fork_activation(
            paths,
            codex_binary,
            repository,
            loaded.as_ref().context("source workspace was not loaded")?,
        )
        .await
        .with_context(|| format!("cocod log:\n{}", read_log(&log)))?;
        retirement::verify_workspace_retirement(
            paths,
            codex_binary,
            repository,
            loaded.as_ref().context("source workspace was not loaded")?,
        )
        .await
        .with_context(|| format!("cocod log:\n{}", read_log(&log)))?;
    }
    stop_daemon(&mut daemon, &log).await?;
    verify_runtime_cleanup(paths)?;
    Ok((status, loaded))
}

async fn verify_context_fork_activation(
    paths: &TestPaths,
    codex_binary: &Path,
    repository: &Path,
    source: &Value,
) -> Result<()> {
    let source_thread_id = source
        .pointer("/workspace/codexThreadId")
        .and_then(Value::as_str)
        .context("source workspace had no Codex thread")?;
    run_cli(
        paths,
        codex_binary,
        repository,
        &["create", FORK_WORKSPACE_NAME, "--context", WORKSPACE_NAME],
    )
    .await?;
    let prepared =
        workspace_status_for(paths, codex_binary, repository, FORK_WORKSPACE_NAME).await?;
    assert_prepared_workspace(&prepared)?;
    ensure!(
        prepared.pointer("/workspace/contextMode") == Some(&json!("fork")),
        "context child did not remain a prepared fork: {prepared}"
    );

    let attached = daemon_request(
        paths,
        "workspace.attach",
        json!({
            "scope": {"kind": "repository", "path": repository},
            "workspace": FORK_WORKSPACE_NAME,
        }),
    )
    .await?;
    let child_thread_id = attached
        .pointer("/workspace/codexThreadId")
        .and_then(Value::as_str)
        .context("context fork did not bind a Codex thread")?;
    ensure!(
        child_thread_id != source_thread_id
            && attached.pointer("/launch/kind") == Some(&json!("resume"))
            && attached.pointer("/launch/threadId") == Some(&json!(child_thread_id))
            && attached.pointer("/workspace/parentThreadId") == Some(&json!(source_thread_id)),
        "workspace.attach did not return the exact native context fork: {attached}"
    );
    ensure!(
        attached.pointer("/workspace/phase") == Some(&json!("idle")),
        "context fork was not idle before the terminal UI opened: {attached}"
    );
    assert_distinct_workspace_executors(
        source,
        &workspace_status_for(paths, codex_binary, repository, FORK_WORKSPACE_NAME).await?,
    )?;
    release_returned_attach(paths, &attached).await?;
    Ok(())
}

fn assert_distinct_workspace_executors(source: &Value, child: &Value) -> Result<()> {
    let source_pid = source
        .pointer("/runtimeResources/processId")
        .and_then(Value::as_u64)
        .context("source workspace had no executor process")?;
    let child_pid = child
        .pointer("/runtimeResources/processId")
        .and_then(Value::as_u64)
        .context("context child had no executor process")?;
    ensure!(
        source_pid != child_pid,
        "source and context child unexpectedly shared executor process {source_pid}"
    );
    ensure!(
        child.pointer("/runtimeResources/state") == Some(&json!("running")),
        "context child executor was not running: {child}"
    );
    Ok(())
}

fn assert_prepared_workspace(status: &Value) -> Result<()> {
    ensure!(
        status.pointer("/workspace/lifecycle") == Some(&json!("ready")),
        "new workspace was not ready: {status}"
    );
    ensure!(
        status.pointer("/workspace/phase") == Some(&json!("prepared")),
        "create fabricated a native thread: {status}"
    );
    ensure!(
        status
            .pointer("/workspace/codexThreadId")
            .is_none_or(Value::is_null),
        "prepared workspace already had a native thread: {status}"
    );
    ensure!(
        status
            .pointer("/workspace/threadRuntime")
            .is_none_or(Value::is_null),
        "prepared workspace exposed invented native runtime state: {status}"
    );
    Ok(())
}

async fn materialize_workspace_through_remote_action(
    paths: &TestPaths,
    codex_binary: &Path,
    repository: &Path,
    model: &str,
) -> Result<()> {
    let empty_lease = begin_fresh_attach(paths, repository).await?;
    verify_workspace_resources(paths, codex_binary, repository).await?;
    let mut empty_remote = RemoteAppServer::connect_with_capabilities(paths, true).await?;
    verify_workspace_environment(&mut empty_remote, &empty_lease).await?;
    let empty_thread = start_remote_thread(&mut empty_remote, &empty_lease, model, true).await?;
    let pending = adopt_remote_thread(paths, &empty_lease, &empty_thread).await?;
    ensure!(
        pending["state"] == "pending",
        "an empty native thread unexpectedly materialized: {pending}"
    );
    empty_remote.close(&empty_thread).await?;
    release_attach(paths, &empty_lease).await?;
    assert_prepared_workspace(&workspace_status(paths, codex_binary, repository).await?)?;

    let active_lease = begin_fresh_attach(paths, repository).await?;
    let mut active_remote = RemoteAppServer::connect_with_capabilities(paths, true).await?;
    verify_workspace_environment(&mut active_remote, &active_lease).await?;
    // Keep the model-free shell materialization on Codex's local environment.
    // `thread/shellCommand` is intentionally host-local upstream; ordinary
    // model tool calls use the workspace environment selected on turn/start.
    let active_thread =
        start_remote_thread(&mut active_remote, &active_lease, model, false).await?;
    active_remote
        .request(
            "thread/shellCommand",
            json!({"threadId": &active_thread, "command": HISTORY_COMMAND}),
        )
        .await?;
    let completed = active_remote
        .wait_for_thread_notification("turn/completed", &active_thread)
        .await?;
    ensure!(
        completed.pointer("/params/turn/status") == Some(&json!("completed")),
        "model-free materialization action did not complete: {completed}"
    );
    let bound = wait_for_remote_adoption(paths, &active_lease, &active_thread).await?;
    ensure!(
        bound["state"] == "bound"
            && bound.pointer("/workspace/codexThreadId") == Some(&json!(&active_thread)),
        "cocod did not bind the exact materialized remote thread: {bound}"
    );
    ensure!(
        bound.pointer("/workspace/phase") == Some(&json!("idle")),
        "completed model-free remote action did not bind idle: {bound}"
    );
    active_remote.close(&active_thread).await?;
    release_attach(paths, &active_lease).await?;

    let detached = workspace_status(paths, codex_binary, repository).await?;
    ensure!(
        detached.pointer("/workspace/phase") == Some(&json!("idle")),
        "closing the completed remote session changed native state: {detached}"
    );
    Ok(())
}

async fn begin_fresh_attach(paths: &TestPaths, repository: &Path) -> Result<AttachLease> {
    let result = daemon_request(
        paths,
        "workspace.attach",
        json!({
            "scope": {"kind": "repository", "path": repository},
            "workspace": WORKSPACE_NAME,
        }),
    )
    .await?;
    ensure!(
        result.pointer("/launch/kind") == Some(&json!("start")),
        "prepared workspace did not return a fresh attach lease: {result}"
    );
    Ok(AttachLease {
        workspace_id: result["workspace"]["id"]
            .as_str()
            .context("attach result had no workspace id")?
            .to_owned(),
        lease_id: result["launch"]["leaseId"]
            .as_str()
            .context("fresh attach result had no lease id")?
            .to_owned(),
        cwd: PathBuf::from(
            result["workspace"]["worktreePath"]
                .as_str()
                .context("fresh attach result had no worktree")?,
        ),
        execution_environment: result
            .get("executionEnvironment")
            .cloned()
            .context("fresh attach result had no workspace execution environment")?,
    })
}

async fn start_remote_thread(
    remote: &mut RemoteAppServer,
    lease: &AttachLease,
    model: &str,
    select_workspace_environment: bool,
) -> Result<String> {
    let cwd = &lease.cwd;
    let mut params = json!({
        "cwd": cwd,
        "config": {},
        "ephemeral": false,
        "model": model,
    });
    if select_workspace_environment {
        params["environments"] = json!([lease.execution_environment.clone()]);
    }
    let result = remote.request("thread/start", params).await?;
    ensure!(
        result["cwd"].as_str() == cwd.to_str(),
        "remote thread/start changed cwd: {result}"
    );
    ensure!(
        result["model"].as_str() == Some(model),
        "remote thread/start did not make the requested model effective: {result}"
    );
    if select_workspace_environment {
        ensure!(
            result.pointer("/thread/environments/0/environmentId")
                == lease.execution_environment.get("environmentId"),
            "remote thread/start did not bind the workspace executor: {result}"
        );
    }
    result
        .pointer("/thread/id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(ToOwned::to_owned)
        .context("remote thread/start returned no thread id")
}

async fn verify_workspace_environment(
    remote: &mut RemoteAppServer,
    lease: &AttachLease,
) -> Result<()> {
    ensure!(
        lease
            .execution_environment
            .get("cwd")
            .and_then(Value::as_str)
            == lease.cwd.to_str(),
        "attach returned an execution cwd that differs from the managed worktree"
    );
    let environment_id = lease
        .execution_environment
        .get("environmentId")
        .and_then(Value::as_str)
        .context("attach execution environment had no id")?;
    let info = remote
        .request("environment/info", json!({"environmentId": environment_id}))
        .await?;
    ensure!(
        info.pointer("/shell/name")
            .and_then(Value::as_str)
            .is_some(),
        "workspace exec-server returned no shell information: {info}"
    );
    let status = remote
        .request(
            "environment/status",
            json!({"environmentId": environment_id}),
        )
        .await?;
    ensure!(
        status.get("status") == Some(&json!("ready")),
        "workspace exec-server was not ready after registration: {status}"
    );
    Ok(())
}

async fn verify_workspace_resources(
    paths: &TestPaths,
    codex_binary: &Path,
    repository: &Path,
) -> Result<()> {
    let status = workspace_status(paths, codex_binary, repository).await?;
    let resources = status
        .get("runtimeResources")
        .context("workspace status did not include runtime resources")?;
    ensure!(
        resources.get("backend") == Some(&json!("exec_server"))
            && resources.get("state") == Some(&json!("running"))
            && resources
                .get("processId")
                .and_then(Value::as_u64)
                .is_some_and(|pid| pid > 0),
        "workspace status did not identify its running exec-server: {resources}"
    );
    #[cfg(target_os = "linux")]
    ensure!(
        resources
            .get("processCount")
            .and_then(Value::as_u64)
            .is_some_and(|count| count >= 1)
            && resources
                .get("residentMemoryBytes")
                .and_then(Value::as_u64)
                .is_some_and(|bytes| bytes > 0),
        "workspace status did not measure its Linux process tree: {resources}"
    );
    Ok(())
}

async fn wait_for_remote_adoption(
    paths: &TestPaths,
    lease: &AttachLease,
    thread_id: &str,
) -> Result<Value> {
    let deadline = Instant::now() + COMPATIBILITY_TIMEOUT;
    loop {
        let result = adopt_remote_thread(paths, lease, thread_id).await?;
        if result["state"] == "bound" {
            return Ok(result);
        }
        ensure!(
            result["state"] == "pending",
            "invalid adoption result: {result}"
        );
        if Instant::now() >= deadline {
            bail!("materialized remote thread was never adopted: {result}");
        }
        sleep(POLL_INTERVAL).await;
    }
}

async fn adopt_remote_thread(
    paths: &TestPaths,
    lease: &AttachLease,
    thread_id: &str,
) -> Result<Value> {
    daemon_request(
        paths,
        "workspace.attach.adopt",
        json!({
            "workspaceId": lease.workspace_id,
            "leaseId": lease.lease_id,
            "threadId": thread_id,
        }),
    )
    .await
}

async fn release_attach(paths: &TestPaths, lease: &AttachLease) -> Result<()> {
    daemon_request(
        paths,
        "workspace.attach.release",
        json!({"workspaceId": lease.workspace_id, "leaseId": lease.lease_id}),
    )
    .await?;
    Ok(())
}

async fn attach_workspace(paths: &TestPaths, repository: &Path) -> Result<()> {
    let result = daemon_request(
        paths,
        "workspace.attach",
        json!({
            "scope": {"kind": "repository", "path": repository},
            "workspace": WORKSPACE_NAME,
        }),
    )
    .await?;
    ensure!(
        result.pointer("/launch/kind") == Some(&json!("resume")),
        "bound workspace did not return a resume launch: {result}"
    );
    ensure!(
        result.pointer("/launch/threadId") == result.pointer("/workspace/codexThreadId"),
        "workspace.attach returned conflicting native thread IDs: {result}"
    );
    ensure!(
        result.pointer("/workspace/phase") == Some(&json!("idle")),
        "workspace.attach did not load the native thread: {result}"
    );
    release_returned_attach(paths, &result).await?;
    Ok(())
}

async fn release_returned_attach(paths: &TestPaths, attached: &Value) -> Result<()> {
    let workspace_id = attached
        .pointer("/workspace/id")
        .and_then(Value::as_str)
        .context("workspace.attach returned no workspace ID")?;
    let lease_id = attached
        .pointer("/launch/leaseId")
        .and_then(Value::as_str)
        .context("workspace.attach returned no lease ID")?;
    daemon_request(
        paths,
        "workspace.attach.release",
        json!({"workspaceId": workspace_id, "leaseId": lease_id}),
    )
    .await?;
    Ok(())
}

async fn daemon_request(paths: &TestPaths, method: &str, params: Value) -> Result<Value> {
    let mut stream = UnixStream::connect(&paths.socket)
        .await
        .with_context(|| format!("could not connect to cocod for {method}"))?;
    let mut request = serde_json::to_vec(&json!({
        "id": format!("real-codex-{method}"),
        "method": method,
        "params": params,
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
    .with_context(|| format!("{method} timed out"))??;
    let response: Value = serde_json::from_str(&response)
        .with_context(|| format!("{method} returned invalid daemon JSON"))?;
    ensure!(
        response.get("error").is_none(),
        "daemon method {method} failed: {response}"
    );
    response
        .get("result")
        .cloned()
        .with_context(|| format!("daemon method {method} returned no result"))
}

fn select_default_model(response: &Value) -> Result<String> {
    ensure!(
        response["schemaVersion"] == 9,
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
    workspace_status_for(paths, codex_binary, repository, WORKSPACE_NAME).await
}

async fn workspace_status_for(
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
        ensure!(
            status["workspace"]["profile"]["modelOverride"]
                .as_str()
                .is_some_and(|model| !model.is_empty()),
            "workspace did not retain its explicit model override: {status}"
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
