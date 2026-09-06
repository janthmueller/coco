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
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::handshake::server::Callback;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::{Message, http::header::AUTHORIZATION};
use tokio_tungstenite::{WebSocketStream, accept_hdr_async, client_async};

const PROCESS_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(25);
const THREAD_ID: &str = "thread-process-smoke";
const TURN_ID: &str = "turn-process-smoke";

struct CaptureAuthorization(Arc<Mutex<Vec<String>>>);

impl Callback for CaptureAuthorization {
    fn on_request(self, request: &Request, response: Response) -> Result<Response, ErrorResponse> {
        let value = request
            .headers()
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);
        if let Some(value) = value {
            self.0
                .lock()
                .expect("authorization capture mutex was poisoned")
                .push(value);
        }
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
    jump_args: PathBuf,
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
            jump_args: root.join("fake-jump.args"),
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
            .env("COCO_TEST_JUMP_ARGS", &self.jump_args)
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
    let recovery_log = temporary.path().join("cocod-recovery.log");

    prepare_repository(&repository)?;
    write_fake_codex(&paths.fake_codex)?;

    let mut daemon = spawn_daemon(&paths, &daemon_log)?;

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
    let observed_authorization = Arc::new(Mutex::new(Vec::new()));
    let observed_requests = Arc::new(Mutex::new(Vec::new()));
    let observed_remote_requests = Arc::new(Mutex::new(Vec::new()));
    let (complete_sender, complete_receiver) = oneshot::channel();
    let app_server = tokio::spawn(run_fake_app_server(
        listener,
        Arc::clone(&observed_authorization),
        Arc::clone(&observed_requests),
        Arc::clone(&observed_remote_requests),
        complete_receiver,
    ));

    wait_for_file(&paths.socket, &mut daemon, &daemon_log).await?;
    wait_for_file(&paths.endpoint, &mut daemon, &daemon_log).await?;
    assert_mode(&paths.socket, 0o600)?;
    assert_mode(&paths.endpoint, 0o600)?;
    assert_mode(&paths.database, 0o600)?;

    run_cli(&paths, &repository, &["repo", "add", "."]).await?;
    let failed_jump = run_cli_with_jump_exit(
        &paths,
        &repository,
        &[
            "create",
            "process-smoke",
            "--base",
            "HEAD",
            "--send",
            "Complete the process smoke test",
            "--jump",
        ],
        23,
    )
    .await?;
    let failed_jump_error = String::from_utf8_lossy(&failed_jump.stderr);
    ensure!(
        failed_jump_error.contains(
            "workspace \"process-smoke\" was created and its initial turn was accepted, but the Codex terminal UI did not open"
        ),
        "create did not explain its retained state after jump failure: {failed_jump_error}"
    );

    let listed = cli_json(&run_cli(&paths, &repository, &["ls", "--json"]).await?)?;
    assert_eq!(listed["schemaVersion"], 3);
    let workspaces = listed["workspaces"]
        .as_array()
        .context("coco ls did not return a workspaces array")?;
    ensure!(
        workspaces.len() == 1,
        "coco ls returned an unexpected workspace count"
    );
    let workspace = &workspaces[0];
    assert_eq!(workspace["name"], "process-smoke");
    assert_eq!(workspace["lifecycle"], "ready");
    assert_eq!(workspace["phase"], "active");
    assert_eq!(workspace["waitReasons"], json!([]));
    ensure!(
        matches!(
            workspace["threadRuntime"]["status"]["type"].as_str(),
            Some("idle" | "active")
        ),
        "workspace exposed an unexpected native thread status"
    );
    assert_eq!(workspace["threadRuntime"]["isFresh"], true);
    assert_eq!(workspace["codexThreadId"], THREAD_ID);
    ensure!(
        workspace.get("goal").is_none(),
        "retired goal field was exposed"
    );
    let worktree = PathBuf::from(
        workspace["worktreePath"]
            .as_str()
            .context("prepared workspace had no worktree path")?,
    );
    ensure!(
        worktree.is_dir(),
        "prepared workspace worktree does not exist"
    );

    let active = workspace_status(&paths, &repository).await?;
    assert_eq!(active["workspace"]["phase"], "active");

    run_cli(&paths, &repository, &["jump", "process-smoke"]).await?;
    let jump_arguments = read_arguments(&paths.jump_args)?;
    verify_jump_arguments(&jump_arguments, &endpoint, &worktree)?;
    ensure!(
        !jump_arguments
            .iter()
            .any(|argument| argument == capability_token.trim()),
        "jump exposed the capability token in its arguments"
    );

    remote_tui_session(&endpoint, capability_token.trim(), true).await?;
    assert_eq!(
        workspace_status(&paths, &repository).await?["workspace"]["phase"],
        "active"
    );
    remote_tui_session(&endpoint, capability_token.trim(), false).await?;
    assert_eq!(
        workspace_status(&paths, &repository).await?["workspace"]["phase"],
        "active"
    );

    complete_sender
        .send(())
        .map_err(|_| anyhow::anyhow!("fake App Server stopped before turn completion"))?;
    let completed = wait_for_workspace_phase(&paths, &repository, "idle").await?;
    assert_eq!(completed["workspace"]["activeTurnId"], Value::Null);
    let initial_generation = completed["workspace"]["threadRuntime"]["runtimeGeneration"]
        .as_str()
        .context("workspace had no initial runtime generation")?
        .to_owned();

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
        .context("fake App Server workspace panicked")?;
    server_result?;

    let expected_authorization = format!("Bearer {capability_token}");
    {
        let authorizations = observed_authorization
            .lock()
            .expect("authorization capture mutex was poisoned");
        assert_eq!(authorizations.len(), 3);
        assert!(
            authorizations
                .iter()
                .all(|authorization| authorization == &expected_authorization)
        );
    }
    verify_codex_requests(
        &observed_requests
            .lock()
            .expect("request capture mutex was poisoned"),
        &worktree,
    )?;
    verify_remote_tui_requests(
        &observed_remote_requests
            .lock()
            .expect("remote request capture mutex was poisoned"),
    )?;
    for runtime_file in [&paths.socket, &paths.endpoint, &paths.token] {
        ensure!(
            !runtime_file.exists(),
            "runtime file was not removed: {}",
            runtime_file.display()
        );
    }

    fs::remove_file(&paths.codex_args)?;
    let mut recovered_daemon = spawn_daemon(&paths, &recovery_log)?;
    wait_for_file(&paths.codex_args, &mut recovered_daemon, &recovery_log).await?;
    let recovery_arguments = read_arguments(&paths.codex_args)?;
    let recovery_endpoint = verify_app_server_arguments(&recovery_arguments, &paths.token)?;
    let recovery_capability_token = fs::read_to_string(&paths.token)?;
    ensure!(
        recovery_capability_token.len() == 64,
        "recovery App Server capability token had an unexpected length"
    );
    assert_mode(&paths.token, 0o600)?;
    let recovery_address = recovery_endpoint
        .strip_prefix("ws://")
        .context("recovery App Server endpoint was not a ws:// URL")?
        .parse::<SocketAddr>()
        .context("recovery App Server endpoint had an invalid socket address")?;
    let recovery_listener = TcpListener::bind(recovery_address)
        .await
        .context("could not bind the recovery App Server")?;
    let recovery_authorization = Arc::new(Mutex::new(Vec::new()));
    let recovery_requests = Arc::new(Mutex::new(Vec::new()));
    let recovery_server = tokio::spawn(run_fake_recovery_server(
        recovery_listener,
        Arc::clone(&recovery_authorization),
        Arc::clone(&recovery_requests),
        worktree.clone(),
    ));

    wait_for_file(&paths.socket, &mut recovered_daemon, &recovery_log).await?;
    let recovered = workspace_status(&paths, &repository).await?;
    assert_eq!(recovered["workspace"]["phase"], "idle");
    assert_eq!(recovered["workspace"]["threadRuntime"]["isFresh"], true);
    assert_ne!(
        recovered["workspace"]["threadRuntime"]["runtimeGeneration"],
        initial_generation
    );
    assert_eq!(recovered["workspace"]["codexThreadId"], THREAD_ID);

    interrupt(&recovered_daemon).await?;
    let recovered_daemon_status = timeout(PROCESS_TIMEOUT, recovered_daemon.wait())
        .await
        .context("recovered cocod did not stop after SIGINT")??;
    ensure!(
        recovered_daemon_status.success(),
        "recovered cocod exited with {recovered_daemon_status}: {}",
        read_log(&recovery_log)
    );
    let recovery_server_result = timeout(PROCESS_TIMEOUT, recovery_server)
        .await
        .context("recovery App Server did not stop")?
        .context("recovery App Server workspace panicked")?;
    recovery_server_result?;
    verify_recovery_requests(
        &recovery_requests
            .lock()
            .expect("recovery request capture mutex was poisoned"),
        &worktree,
    )?;
    let recovery_authorizations = recovery_authorization
        .lock()
        .expect("recovery authorization capture mutex was poisoned");
    assert_eq!(recovery_authorizations.len(), 1);
    assert_eq!(
        recovery_authorizations[0],
        format!("Bearer {recovery_capability_token}")
    );
    for runtime_file in [&paths.socket, &paths.endpoint, &paths.token] {
        ensure!(
            !runtime_file.exists(),
            "recovery runtime file was not removed: {}",
            runtime_file.display()
        );
    }
    Ok(())
}

async fn run_fake_app_server(
    listener: TcpListener,
    observed_authorization: Arc<Mutex<Vec<String>>>,
    observed_requests: Arc<Mutex<Vec<Value>>>,
    observed_remote_requests: Arc<Mutex<Vec<Value>>>,
    completion: oneshot::Receiver<()>,
) -> Result<()> {
    let (stream, peer) = listener.accept().await?;
    ensure!(
        peer.ip().is_loopback(),
        "cocod connected from a non-loopback peer"
    );
    let websocket = accept_hdr_async(
        stream,
        CaptureAuthorization(Arc::clone(&observed_authorization)),
    )
    .await?;

    let daemon = handle_daemon_connection(websocket, observed_requests, completion);
    let remote_clients = async {
        for _ in 0..2 {
            let (stream, peer) = listener.accept().await?;
            ensure!(
                peer.ip().is_loopback(),
                "remote TUI connected from a non-loopback peer"
            );
            let websocket = accept_hdr_async(
                stream,
                CaptureAuthorization(Arc::clone(&observed_authorization)),
            )
            .await?;
            handle_remote_connection(websocket, Arc::clone(&observed_remote_requests)).await?;
        }
        Ok::<(), anyhow::Error>(())
    };

    tokio::try_join!(daemon, remote_clients)?;
    Ok(())
}

async fn run_fake_recovery_server(
    listener: TcpListener,
    observed_authorization: Arc<Mutex<Vec<String>>>,
    observed_requests: Arc<Mutex<Vec<Value>>>,
    expected_cwd: PathBuf,
) -> Result<()> {
    let (stream, peer) = listener.accept().await?;
    ensure!(
        peer.ip().is_loopback(),
        "recovered cocod connected from a non-loopback peer"
    );
    let mut websocket =
        accept_hdr_async(stream, CaptureAuthorization(observed_authorization)).await?;
    while let Some(message) = websocket.next().await {
        let frame = match message? {
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
            .expect("recovery request capture mutex was poisoned")
            .push(frame.clone());
        match frame.get("method").and_then(Value::as_str) {
            Some("initialize") => send_result(&mut websocket, &frame, json!({})).await?,
            Some("initialized") => {}
            Some("thread/resume") => {
                send_result(
                    &mut websocket,
                    &frame,
                    json!({
                        "thread": {"id": THREAD_ID, "status": {"type": "idle"}},
                        "cwd": expected_cwd,
                    }),
                )
                .await?;
            }
            Some(other) => bail!("unexpected recovery App Server method {other:?}"),
            None => bail!("received a recovery frame without a method: {frame}"),
        }
    }
    Ok(())
}

async fn handle_daemon_connection(
    mut websocket: WebSocketStream<TcpStream>,
    observed_requests: Arc<Mutex<Vec<Value>>>,
    completion: oneshot::Receiver<()>,
) -> Result<()> {
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
                    json!({
                        "thread": {"id": THREAD_ID, "status": {"type": "idle"}},
                        "cwd": cwd
                    }),
                )
                .await?;
            }
            Some("thread/name/set") => {
                send_result(&mut websocket, &frame, json!({})).await?;
            }
            Some("turn/start") => {
                let completion = completion
                    .take()
                    .context("received more than one turn/start request")?;
                complete_fake_turn(&mut websocket, &frame, completion).await?;
            }
            Some(other) => bail!("unexpected App Server method {other:?}"),
            None => bail!("received an App Server frame without a method: {frame}"),
        }
    }
    Ok(())
}

async fn complete_fake_turn(
    websocket: &mut WebSocketStream<TcpStream>,
    request: &Value,
    completion: oneshot::Receiver<()>,
) -> Result<()> {
    send_result(websocket, request, json!({"turn": {"id": TURN_ID}})).await?;
    send_json(
        websocket,
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
        websocket,
        json!({
            "method": "thread/status/changed",
            "params": {
                "threadId": THREAD_ID,
                "status": {"type": "active", "activeFlags": []}
            }
        }),
    )
    .await?;
    completion
        .await
        .context("test stopped before allowing turn completion")?;
    send_json(
        websocket,
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
        websocket,
        json!({
            "method": "thread/status/changed",
            "params": {
                "threadId": THREAD_ID,
                "status": {"type": "idle"}
            }
        }),
    )
    .await?;
    send_json(
        websocket,
        json!({
            "method": "turn/completed",
            "params": {
                "threadId": THREAD_ID,
                "turn": {"id": TURN_ID, "status": "completed"}
            }
        }),
    )
    .await
}

async fn handle_remote_connection(
    mut websocket: WebSocketStream<TcpStream>,
    observed_requests: Arc<Mutex<Vec<Value>>>,
) -> Result<()> {
    while let Some(message) = websocket.next().await {
        let message = match message {
            Ok(message) => message,
            Err(_) => return Ok(()),
        };
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
            .expect("remote request capture mutex was poisoned")
            .push(frame.clone());
        match frame.get("method").and_then(Value::as_str) {
            Some("initialize") => send_result(&mut websocket, &frame, json!({})).await?,
            Some("initialized") => {}
            Some("thread/resume") => {
                send_result(
                    &mut websocket,
                    &frame,
                    json!({
                        "thread": {
                            "id": THREAD_ID,
                            "status": {"type": "active", "activeFlags": []}
                        }
                    }),
                )
                .await?;
            }
            Some("thread/unsubscribe") => {
                send_result(&mut websocket, &frame, json!({})).await?;
            }
            Some(other) => bail!("unexpected remote TUI method {other:?}"),
            None => bail!("received a remote TUI frame without a method: {frame}"),
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

async fn remote_tui_session(endpoint: &str, token: &str, graceful: bool) -> Result<()> {
    let address = endpoint
        .strip_prefix("ws://")
        .context("remote TUI endpoint was not a ws:// URL")?
        .parse::<SocketAddr>()
        .context("remote TUI endpoint had an invalid socket address")?;
    let mut request = endpoint.into_client_request()?;
    request.headers_mut().insert(
        AUTHORIZATION,
        format!("Bearer {token}")
            .parse()
            .context("capability token was not a valid authorization header")?,
    );
    let stream = TcpStream::connect(address).await?;
    let (mut websocket, _) = client_async(request, stream).await?;

    send_json(
        &mut websocket,
        json!({
            "id": 1,
            "method": "initialize",
            "params": {
                "clientInfo": {"name": "coco-remote-contract", "version": "0.0.0"}
            }
        }),
    )
    .await?;
    await_result(&mut websocket, json!(1)).await?;
    send_json(
        &mut websocket,
        json!({"method": "initialized", "params": {}}),
    )
    .await?;
    send_json(
        &mut websocket,
        json!({
            "id": 2,
            "method": "thread/resume",
            "params": {"threadId": THREAD_ID}
        }),
    )
    .await?;
    await_result(&mut websocket, json!(2)).await?;

    if graceful {
        send_json(
            &mut websocket,
            json!({
                "id": 3,
                "method": "thread/unsubscribe",
                "params": {"threadId": THREAD_ID}
            }),
        )
        .await?;
        await_result(&mut websocket, json!(3)).await?;
        websocket.close(None).await?;
    }
    Ok(())
}

async fn await_result(
    websocket: &mut WebSocketStream<TcpStream>,
    expected_id: Value,
) -> Result<()> {
    while let Some(message) = websocket.next().await {
        let frame = match message? {
            Message::Text(text) => serde_json::from_str::<Value>(&text)?,
            Message::Binary(bytes) => serde_json::from_slice::<Value>(&bytes)?,
            Message::Ping(payload) => {
                websocket.send(Message::Pong(payload)).await?;
                continue;
            }
            Message::Pong(_) | Message::Frame(_) => continue,
            Message::Close(_) => bail!("remote App Server closed before responding"),
        };
        if frame.get("id") == Some(&expected_id) {
            ensure!(
                frame.get("error").is_none(),
                "remote App Server returned an error: {frame}"
            );
            ensure!(
                frame.get("result").is_some(),
                "remote App Server response had no result: {frame}"
            );
            return Ok(());
        }
    }
    bail!("remote App Server disconnected before responding")
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
case "${1:-}" in
  app-server)
    : "${COCO_TEST_CODEX_ARGS:?}"
    destination="${COCO_TEST_CODEX_ARGS}"
    ;;
  resume)
    : "${COCO_TEST_JUMP_ARGS:?}"
    destination="${COCO_TEST_JUMP_ARGS}"
    ;;
  *)
    exit 64
    ;;
esac
arguments_tmp="${destination}.tmp"
printf '%s\n' "$@" > "$arguments_tmp"
mv "$arguments_tmp" "$destination"
if [ "$1" = "app-server" ]; then
  exec sleep 3600
fi
if [ "$1" = "resume" ]; then
  exit "${COCO_TEST_JUMP_EXIT:-0}"
fi
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

fn verify_jump_arguments(arguments: &[String], endpoint: &str, worktree: &Path) -> Result<()> {
    let expected = [
        "resume".to_owned(),
        THREAD_ID.to_owned(),
        "--remote".to_owned(),
        endpoint.to_owned(),
        "--remote-auth-token-env".to_owned(),
        "COCO_CODEX_REMOTE_CAPABILITY_TOKEN".to_owned(),
        "-C".to_owned(),
        worktree.to_string_lossy().into_owned(),
    ];
    ensure!(
        arguments == expected,
        "unexpected fake jump arguments: {arguments:?}"
    );
    Ok(())
}

fn spawn_daemon(paths: &TestPaths, log: &Path) -> Result<Child> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cocod"));
    paths.apply(&mut command);
    command
        .stdout(Stdio::null())
        .stderr(Stdio::from(fs::File::create(log)?))
        .kill_on_drop(true)
        .spawn()
        .context("could not start cocod")
}

async fn run_cli(paths: &TestPaths, repository: &Path, arguments: &[&str]) -> Result<Output> {
    let output = capture_cli(paths, repository, arguments, None).await?;
    ensure!(
        output.status.success(),
        "coco {} failed:\nstdout: {}\nstderr: {}",
        arguments.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output)
}

async fn run_cli_with_jump_exit(
    paths: &TestPaths,
    repository: &Path,
    arguments: &[&str],
    exit_code: u8,
) -> Result<Output> {
    let output = capture_cli(paths, repository, arguments, Some(exit_code)).await?;
    ensure!(
        !output.status.success(),
        "coco {} unexpectedly succeeded",
        arguments.join(" ")
    );
    Ok(output)
}

async fn capture_cli(
    paths: &TestPaths,
    repository: &Path,
    arguments: &[&str],
    jump_exit: Option<u8>,
) -> Result<Output> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_coco"));
    paths.apply(&mut command);
    command
        .args(arguments)
        .current_dir(repository)
        .kill_on_drop(true);
    if let Some(exit_code) = jump_exit {
        command.env("COCO_TEST_JUMP_EXIT", exit_code.to_string());
    }
    let output = timeout(PROCESS_TIMEOUT, command.output())
        .await
        .with_context(|| format!("coco {} timed out", arguments.join(" ")))??;
    Ok(output)
}

fn cli_json(output: &Output) -> Result<Value> {
    serde_json::from_slice(&output.stdout).context("coco did not emit valid JSON")
}

async fn workspace_status(paths: &TestPaths, repository: &Path) -> Result<Value> {
    cli_json(&run_cli(paths, repository, &["status", "process-smoke", "--json"]).await?)
}

async fn wait_for_workspace_phase(
    paths: &TestPaths,
    repository: &Path,
    expected: &str,
) -> Result<Value> {
    let deadline = Instant::now() + PROCESS_TIMEOUT;
    loop {
        let status = workspace_status(paths, repository).await?;
        if status.pointer("/workspace/phase").and_then(Value::as_str) == Some(expected) {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            bail!("workspace did not reach phase {expected:?}: {status}");
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
    ensure!(
        thread_start
            .pointer("/params/runtimeWorkspaceRoots")
            .is_none(),
        "thread/start used an experimental field without negotiating the capability"
    );
    assert_eq!(thread_start.pointer("/params/config"), Some(&json!({})));
    assert_eq!(
        thread_start.pointer("/params/ephemeral"),
        Some(&json!(false))
    );

    let thread_name = request(requests, "thread/name/set")?;
    assert_eq!(
        thread_name.pointer("/params/threadId"),
        Some(&json!(THREAD_ID))
    );
    assert_eq!(
        thread_name.pointer("/params/name"),
        Some(&json!("process-smoke"))
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

fn verify_recovery_requests(requests: &[Value], worktree: &Path) -> Result<()> {
    let methods = requests
        .iter()
        .filter_map(|request| request.get("method").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert_eq!(methods, ["initialize", "initialized", "thread/resume"]);
    let resume = request(requests, "thread/resume")?;
    assert_eq!(resume.pointer("/params/threadId"), Some(&json!(THREAD_ID)));
    assert_eq!(
        resume.pointer("/params/cwd"),
        Some(&json!(worktree.to_string_lossy()))
    );
    assert_eq!(resume.pointer("/params/config"), Some(&json!({})));
    ensure!(
        request(requests, "thread/start").is_err(),
        "recovery created a replacement thread"
    );
    Ok(())
}

fn verify_remote_tui_requests(requests: &[Value]) -> Result<()> {
    let methods = requests
        .iter()
        .map(|request| {
            request
                .get("method")
                .and_then(Value::as_str)
                .context("remote TUI request had no method")
        })
        .collect::<Result<Vec<_>>>()?;
    assert_eq!(
        methods,
        [
            "initialize",
            "initialized",
            "thread/resume",
            "thread/unsubscribe",
            "initialize",
            "initialized",
            "thread/resume",
        ]
    );
    for request in requests
        .iter()
        .filter(|request| request.get("method") == Some(&json!("thread/resume")))
    {
        assert_eq!(request.pointer("/params/threadId"), Some(&json!(THREAD_ID)));
    }
    ensure!(
        !methods.contains(&"turn/interrupt"),
        "leaving a remote TUI unexpectedly interrupted the turn"
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
