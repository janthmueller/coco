#![cfg(unix)]

use std::fs;
use std::net::SocketAddr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::{Child, Command};
use tokio::sync::oneshot;
use tokio::time::{sleep, timeout};
use tokio_tungstenite::tungstenite::handshake::server::Callback;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::{Message, http::header::AUTHORIZATION};
use tokio_tungstenite::{WebSocketStream, accept_hdr_async};

const PROCESS_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(25);
const THREAD_ID: &str = "thread-process-smoke";
const TURN_ID: &str = "turn-process-smoke";

struct CaptureAuthorization(Arc<Mutex<Option<String>>>);

impl Callback for CaptureAuthorization {
    fn on_request(self, request: &Request, response: Response) -> Result<Response, ErrorResponse> {
        let value = request
            .headers()
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);
        *self
            .0
            .lock()
            .expect("authorization capture mutex was poisoned") = value;
        Ok(response)
    }
}

#[derive(Debug)]
struct TestPaths {
    home: PathBuf,
    data_dir: PathBuf,
    database: PathBuf,
    socket: PathBuf,
    endpoint: PathBuf,
    token: PathBuf,
    worktrees: PathBuf,
    codex_home: PathBuf,
    fake_codex: PathBuf,
    codex_args: PathBuf,
}

impl TestPaths {
    fn new(root: &Path) -> Self {
        let runtime = root.join("runtime");
        let data_dir = root.join("data");
        Self {
            home: root.join("home"),
            database: data_dir.join("coco.db"),
            socket: runtime.join("cocod.sock"),
            endpoint: runtime.join("codex-app-server.json"),
            token: runtime.join("codex-app-server.token"),
            worktrees: root.join("worktrees"),
            codex_home: root.join("codex-home"),
            fake_codex: root.join("fake-codex"),
            codex_args: root.join("fake-codex.args"),
            data_dir,
        }
    }

    fn apply(&self, command: &mut Command) {
        command
            .env("HOME", &self.home)
            .env("COCO_DATA_DIR", &self.data_dir)
            .env("COCO_DATABASE_PATH", &self.database)
            .env("COCO_SOCKET_PATH", &self.socket)
            .env("COCO_CODEX_ENDPOINT_PATH", &self.endpoint)
            .env("COCO_CODEX_TOKEN_PATH", &self.token)
            .env("COCO_WORKTREES_DIR", &self.worktrees)
            .env("CODEX_HOME", &self.codex_home)
            .env("COCO_CODEX_BINARY", &self.fake_codex)
            .env("COCO_TEST_CODEX_ARGS", &self.codex_args)
            .env("RUST_LOG", "warn");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[expect(
    clippy::too_many_lines,
    reason = "one end-to-end scenario keeps daemon, CLI, App Server, and cleanup assertions ordered"
)]
async fn real_daemon_and_cli_complete_a_fake_codex_turn() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let paths = TestPaths::new(temporary.path());
    let repository = temporary.path().join("repository");
    let daemon_log = temporary.path().join("cocod.log");

    prepare_repository(&repository)?;
    write_fake_codex(&paths.fake_codex)?;

    let mut daemon_command = Command::new(env!("CARGO_BIN_EXE_cocod"));
    paths.apply(&mut daemon_command);
    daemon_command
        .stdout(Stdio::null())
        .stderr(Stdio::from(fs::File::create(&daemon_log)?))
        .kill_on_drop(true);
    let mut daemon = daemon_command.spawn().context("could not start cocod")?;

    wait_for_file(&paths.codex_args, &mut daemon, &daemon_log).await?;
    let arguments = read_arguments(&paths.codex_args)?;
    let endpoint = verify_app_server_arguments(&arguments, &paths.token)?;
    let address = endpoint
        .strip_prefix("ws://")
        .context("App Server endpoint was not a ws:// URL")?
        .parse::<SocketAddr>()
        .context("App Server endpoint had an invalid socket address")?;
    ensure!(
        address.ip().is_loopback(),
        "App Server was not loopback-only"
    );

    let capability_token = fs::read_to_string(&paths.token)?;
    ensure!(
        capability_token.len() == 64
            && capability_token
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit()),
        "App Server capability token was not a 64-character hex value"
    );
    assert_mode(&paths.token, 0o600)?;

    let listener = TcpListener::bind(address)
        .await
        .context("could not bind the fake App Server")?;
    let observed_authorization = Arc::new(Mutex::new(None));
    let observed_requests = Arc::new(Mutex::new(Vec::new()));
    let (complete_sender, complete_receiver) = oneshot::channel();
    let app_server = tokio::spawn(run_fake_app_server(
        listener,
        Arc::clone(&observed_authorization),
        Arc::clone(&observed_requests),
        complete_receiver,
    ));

    wait_for_file(&paths.socket, &mut daemon, &daemon_log).await?;
    wait_for_file(&paths.endpoint, &mut daemon, &daemon_log).await?;
    assert_mode(&paths.socket, 0o600)?;
    assert_mode(&paths.endpoint, 0o600)?;
    assert_mode(&paths.database, 0o600)?;

    run_cli(&paths, &repository, &["repo", "add", "."]).await?;
    run_cli(
        &paths,
        &repository,
        &["new", "process-smoke", "--base", "HEAD"],
    )
    .await?;

    let listed = cli_json(&run_cli(&paths, &repository, &["ls", "--json"]).await?)?;
    assert_eq!(listed["schemaVersion"], 1);
    let tasks = listed["tasks"]
        .as_array()
        .context("coco ls did not return a tasks array")?;
    ensure!(
        tasks.len() == 1,
        "coco ls returned an unexpected task count"
    );
    let task = &tasks[0];
    assert_eq!(task["name"], "process-smoke");
    assert_eq!(task["phase"], "idle");
    assert_eq!(task["codexThreadId"], THREAD_ID);
    ensure!(task.get("goal").is_none(), "retired goal field was exposed");
    let worktree = PathBuf::from(
        task["worktreePath"]
            .as_str()
            .context("prepared task had no worktree path")?,
    );
    ensure!(worktree.is_dir(), "prepared task worktree does not exist");

    run_cli(
        &paths,
        &repository,
        &["send", "process-smoke", "Complete the process smoke test"],
    )
    .await?;
    let active = task_status(&paths, &repository).await?;
    assert_eq!(active["task"]["phase"], "active");

    complete_sender
        .send(())
        .map_err(|_| anyhow::anyhow!("fake App Server stopped before turn completion"))?;
    let completed = wait_for_task_phase(&paths, &repository, "idle").await?;
    assert_eq!(completed["task"]["activeTurnId"], Value::Null);

    interrupt(&daemon).await?;
    let daemon_status = timeout(PROCESS_TIMEOUT, daemon.wait())
        .await
        .context("cocod did not stop after SIGINT")??;
    ensure!(
        daemon_status.success(),
        "cocod exited with {daemon_status}: {}",
        read_log(&daemon_log)
    );

    let server_result = timeout(PROCESS_TIMEOUT, app_server)
        .await
        .context("fake App Server did not stop")?
        .context("fake App Server task panicked")?;
    server_result?;

    let expected_authorization = format!("Bearer {capability_token}");
    assert_eq!(
        observed_authorization
            .lock()
            .expect("authorization capture mutex was poisoned")
            .as_deref(),
        Some(expected_authorization.as_str())
    );
    verify_codex_requests(
        &observed_requests
            .lock()
            .expect("request capture mutex was poisoned"),
        &worktree,
    )?;
    for runtime_file in [&paths.socket, &paths.endpoint, &paths.token] {
        ensure!(
            !runtime_file.exists(),
            "runtime file was not removed: {}",
            runtime_file.display()
        );
    }
    Ok(())
}

async fn run_fake_app_server(
    listener: TcpListener,
    observed_authorization: Arc<Mutex<Option<String>>>,
    observed_requests: Arc<Mutex<Vec<Value>>>,
    completion: oneshot::Receiver<()>,
) -> Result<()> {
    let (stream, peer) = listener.accept().await?;
    ensure!(
        peer.ip().is_loopback(),
        "cocod connected from a non-loopback peer"
    );
    let mut websocket =
        accept_hdr_async(stream, CaptureAuthorization(observed_authorization)).await?;
    let mut completion = Some(completion);

    while let Some(message) = websocket.next().await {
        let message = message?;
        let frame = match message {
            Message::Text(text) => serde_json::from_str::<Value>(&text)?,
            Message::Binary(bytes) => serde_json::from_slice::<Value>(&bytes)?,
            Message::Close(_) => return Ok(()),
            Message::Ping(payload) => {
                websocket.send(Message::Pong(payload)).await?;
                continue;
            }
            Message::Pong(_) | Message::Frame(_) => continue,
        };
        observed_requests
            .lock()
            .expect("request capture mutex was poisoned")
            .push(frame.clone());
        let method = frame.get("method").and_then(Value::as_str);
        match method {
            Some("initialize") => {
                send_result(&mut websocket, &frame, json!({})).await?;
            }
            Some("initialized") => {}
            Some("thread/start") => {
                let cwd = frame.pointer("/params/cwd").cloned().unwrap_or(Value::Null);
                send_result(
                    &mut websocket,
                    &frame,
                    json!({"thread": {"id": THREAD_ID}, "cwd": cwd}),
                )
                .await?;
            }
            Some("turn/start") => {
                send_result(&mut websocket, &frame, json!({"turn": {"id": TURN_ID}})).await?;
                completion
                    .take()
                    .context("received more than one turn/start request")?
                    .await
                    .context("test stopped before allowing turn completion")?;
                send_json(
                    &mut websocket,
                    json!({
                        "method": "turn/started",
                        "params": {
                            "threadId": THREAD_ID,
                            "turn": {"id": TURN_ID, "status": "inProgress"}
                        }
                    }),
                )
                .await?;
                send_json(
                    &mut websocket,
                    json!({
                        "method": "item/completed",
                        "params": {
                            "threadId": THREAD_ID,
                            "turnId": TURN_ID,
                            "item": {
                                "id": "message-process-smoke",
                                "type": "agentMessage",
                                "text": "Fake Codex completed the turn."
                            }
                        }
                    }),
                )
                .await?;
                send_json(
                    &mut websocket,
                    json!({
                        "method": "turn/completed",
                        "params": {
                            "threadId": THREAD_ID,
                            "turn": {"id": TURN_ID, "status": "completed"}
                        }
                    }),
                )
                .await?;
            }
            Some(other) => bail!("unexpected App Server method {other:?}"),
            None => bail!("received an App Server frame without a method: {frame}"),
        }
    }
    Ok(())
}

async fn send_result(
    websocket: &mut WebSocketStream<TcpStream>,
    request: &Value,
    result: Value,
) -> Result<()> {
    let id = request
        .get("id")
        .cloned()
        .context("App Server request had no id")?;
    send_json(websocket, json!({"id": id, "result": result})).await
}

async fn send_json(websocket: &mut WebSocketStream<TcpStream>, value: Value) -> Result<()> {
    websocket
        .send(Message::Text(serde_json::to_string(&value)?.into()))
        .await?;
    Ok(())
}

fn prepare_repository(repository: &Path) -> Result<()> {
    fs::create_dir_all(repository)?;
    run_git(repository, &["init", "."])?;
    fs::write(repository.join("README.md"), "# process smoke test\n")?;
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
        .context("could not run Git for the process test")?;
    ensure!(
        output.status.success(),
        "git {} failed: {}",
        arguments.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn write_fake_codex(path: &Path) -> Result<()> {
    fs::write(
        path,
        r#"#!/bin/sh
set -eu
: "${COCO_TEST_CODEX_ARGS:?}"
arguments_tmp="${COCO_TEST_CODEX_ARGS}.tmp"
printf '%s\n' "$@" > "$arguments_tmp"
mv "$arguments_tmp" "$COCO_TEST_CODEX_ARGS"
exec sleep 3600
"#,
    )?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

fn read_arguments(path: &Path) -> Result<Vec<String>> {
    Ok(fs::read_to_string(path)?
        .lines()
        .map(ToOwned::to_owned)
        .collect())
}

fn verify_app_server_arguments(arguments: &[String], token_path: &Path) -> Result<String> {
    ensure!(
        arguments.len() == 7,
        "unexpected fake Codex arguments: {arguments:?}"
    );
    ensure!(arguments[0] == "app-server", "missing app-server command");
    ensure!(arguments[1] == "--listen", "missing --listen option");
    ensure!(arguments[3] == "--ws-auth", "missing --ws-auth option");
    ensure!(arguments[4] == "capability-token", "unexpected auth mode");
    ensure!(
        arguments[5] == "--ws-token-file",
        "missing token file option"
    );
    ensure!(
        Path::new(&arguments[6]) == token_path,
        "unexpected token path"
    );
    Ok(arguments[2].clone())
}

async fn run_cli(paths: &TestPaths, repository: &Path, arguments: &[&str]) -> Result<Output> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_coco"));
    paths.apply(&mut command);
    command
        .args(arguments)
        .current_dir(repository)
        .kill_on_drop(true);
    let output = timeout(PROCESS_TIMEOUT, command.output())
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

fn cli_json(output: &Output) -> Result<Value> {
    serde_json::from_slice(&output.stdout).context("coco did not emit valid JSON")
}

async fn task_status(paths: &TestPaths, repository: &Path) -> Result<Value> {
    cli_json(&run_cli(paths, repository, &["status", "process-smoke", "--json"]).await?)
}

async fn wait_for_task_phase(
    paths: &TestPaths,
    repository: &Path,
    expected: &str,
) -> Result<Value> {
    let deadline = Instant::now() + PROCESS_TIMEOUT;
    loop {
        let status = task_status(paths, repository).await?;
        if status.pointer("/task/phase").and_then(Value::as_str) == Some(expected) {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            bail!("task did not reach phase {expected:?}: {status}");
        }
        sleep(POLL_INTERVAL).await;
    }
}

async fn wait_for_file(path: &Path, daemon: &mut Child, log: &Path) -> Result<()> {
    let deadline = Instant::now() + PROCESS_TIMEOUT;
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

async fn interrupt(child: &Child) -> Result<()> {
    let pid = child.id().context("cocod had no process id")?;
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
    Ok(())
}

fn verify_codex_requests(requests: &[Value], worktree: &Path) -> Result<()> {
    let initialize = request(requests, "initialize")?;
    assert_eq!(
        initialize.pointer("/params/clientInfo/name"),
        Some(&json!("coco"))
    );

    let thread_start = request(requests, "thread/start")?;
    let worktree_value = Value::String(worktree.to_string_lossy().into_owned());
    assert_eq!(thread_start.pointer("/params/cwd"), Some(&worktree_value));
    assert_eq!(
        thread_start.pointer("/params/runtimeWorkspaceRoots/0"),
        Some(&worktree_value)
    );
    assert_eq!(thread_start.pointer("/params/config"), Some(&json!({})));
    assert_eq!(
        thread_start.pointer("/params/ephemeral"),
        Some(&json!(false))
    );

    let turn_start = request(requests, "turn/start")?;
    assert_eq!(
        turn_start.pointer("/params/threadId"),
        Some(&json!(THREAD_ID))
    );
    assert_eq!(turn_start.pointer("/params/cwd"), Some(&worktree_value));
    assert_eq!(
        turn_start.pointer("/params/input/0/text"),
        Some(&json!("Complete the process smoke test"))
    );
    ensure!(
        turn_start
            .pointer("/params/clientUserMessageId")
            .and_then(Value::as_str)
            .is_some_and(|value| value.starts_with("coco-")),
        "turn/start had no CoCo message id"
    );
    Ok(())
}

fn request<'a>(requests: &'a [Value], method: &str) -> Result<&'a Value> {
    requests
        .iter()
        .find(|request| request.get("method").and_then(Value::as_str) == Some(method))
        .with_context(|| format!("fake App Server did not receive {method}"))
}

fn assert_mode(path: &Path, expected: u32) -> Result<()> {
    let actual = fs::metadata(path)?.permissions().mode() & 0o777;
    ensure!(
        actual == expected,
        "{} had mode {actual:o}, expected {expected:o}",
        path.display()
    );
    Ok(())
}

fn read_log(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| format!("could not read daemon log: {error}"))
}
