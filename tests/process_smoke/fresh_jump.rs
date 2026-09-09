use std::env;
use std::net::Ipv4Addr;

use super::app_server::*;
use super::support::*;
use super::*;

#[derive(Clone)]
struct FreshThread {
    id: String,
    cwd: String,
    rollout_path: Option<String>,
    name: String,
    materialized: bool,
    active: bool,
}

#[derive(Default)]
struct FreshServerState {
    threads: Vec<FreshThread>,
    daemon_subscribed: bool,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fresh_jump_adopts_only_after_an_action_and_detaches_without_interrupting() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let paths = TestPaths::new(temporary.path());
    let repository = temporary.path().join("repository");
    let daemon_log = temporary.path().join("cocod-fresh-jump.log");
    prepare_repository(&repository)?;
    prepare_codex_profile(&paths)?;
    write_fake_codex(&paths.fake_codex)?;

    let mut daemon = spawn_daemon(&paths, &daemon_log)?;
    wait_for_file(&paths.codex_args, &mut daemon, &daemon_log).await?;
    let arguments = read_arguments(&paths.codex_args)?;
    let endpoint = verify_app_server_arguments(&arguments, &paths.token)?;
    let capability_token = fs::read_to_string(&paths.token)?;
    let address = endpoint
        .strip_prefix("ws://")
        .context("fresh-jump App Server endpoint was not ws://")?
        .parse::<SocketAddr>()?;
    let listener = TcpListener::bind(address).await?;
    let observed_authorization = Arc::new(Mutex::new(Vec::new()));
    let observed_daemon_requests = Arc::new(Mutex::new(Vec::new()));
    let observed_tui_requests = Arc::new(Mutex::new(Vec::new()));
    let state = Arc::new(Mutex::new(FreshServerState::default()));
    let (completion_tx, completion_rx) = oneshot::channel();
    let app_server = tokio::spawn(run_fake_fresh_jump_server(
        listener,
        Arc::clone(&observed_authorization),
        Arc::clone(&observed_daemon_requests),
        Arc::clone(&observed_tui_requests),
        Arc::clone(&state),
        completion_rx,
    ));

    let worktree =
        prepare_fresh_jump_workspace(&paths, &repository, &mut daemon, &daemon_log).await?;
    prove_relay_failure_releases_lease(&paths, &repository).await?;

    run_cli_with_fresh_tui(
        &paths,
        &repository,
        &["jump", FRESH_JUMP_WORKSPACE],
        "empty",
    )
    .await?;
    verify_fresh_jump_arguments(&read_arguments(&paths.jump_args)?, &endpoint, &worktree)?;
    let still_prepared = named_workspace_status(&paths, &repository, FRESH_JUMP_WORKSPACE).await?;
    assert_eq!(still_prepared["workspace"]["phase"], "prepared");
    assert!(still_prepared["workspace"]["codexThreadId"].is_null());

    run_cli_with_fresh_tui(
        &paths,
        &repository,
        &["jump", FRESH_JUMP_WORKSPACE],
        "active",
    )
    .await?;
    verify_fresh_jump_arguments(&read_arguments(&paths.jump_args)?, &endpoint, &worktree)?;
    let active =
        wait_for_named_workspace_phase(&paths, &repository, FRESH_JUMP_WORKSPACE, "active").await?;
    assert_eq!(active["workspace"]["codexThreadId"], FRESH_ACTIVE_THREAD_ID);

    verify_fresh_jump_requests(
        &observed_daemon_requests
            .lock()
            .expect("fresh daemon request mutex was poisoned"),
        &observed_tui_requests
            .lock()
            .expect("fresh TUI request mutex was poisoned"),
        &worktree,
    )?;

    completion_tx
        .send(())
        .map_err(|()| anyhow::anyhow!("fresh App Server completion receiver disappeared"))?;
    wait_for_named_workspace_phase(&paths, &repository, FRESH_JUMP_WORKSPACE, "idle").await?;

    interrupt(&daemon).await?;
    let daemon_status = timeout(PROCESS_TIMEOUT, daemon.wait()).await??;
    ensure!(
        daemon_status.success(),
        "cocod exited with {daemon_status}: {}",
        read_log(&daemon_log)
    );
    timeout(PROCESS_TIMEOUT, app_server)
        .await
        .context("fresh-jump App Server did not stop")?
        .context("fresh-jump App Server panicked")??;

    let authorizations = observed_authorization
        .lock()
        .expect("fresh authorization mutex was poisoned");
    assert_eq!(authorizations.len(), 3);
    assert!(
        authorizations
            .iter()
            .all(|authorization| authorization == &format!("Bearer {capability_token}"))
    );
    Ok(())
}

async fn prepare_fresh_jump_workspace(
    paths: &TestPaths,
    repository: &Path,
    daemon: &mut Child,
    daemon_log: &Path,
) -> Result<PathBuf> {
    wait_for_file(&paths.socket, daemon, daemon_log).await?;
    run_cli(paths, repository, &["repo", "add", "."]).await?;
    run_cli(
        paths,
        repository,
        &[
            "create",
            FRESH_JUMP_WORKSPACE,
            "--profile",
            PROFILE_NAME,
            "--model",
            MODEL_OVERRIDE,
        ],
    )
    .await?;
    let prepared = named_workspace_status(paths, repository, FRESH_JUMP_WORKSPACE).await?;
    assert_eq!(prepared["workspace"]["phase"], "prepared");
    assert!(prepared["workspace"]["codexThreadId"].is_null());
    prepared["workspace"]["worktreePath"]
        .as_str()
        .map(PathBuf::from)
        .context("prepared fresh-jump workspace had no worktree")
}

async fn prove_relay_failure_releases_lease(paths: &TestPaths, repository: &Path) -> Result<()> {
    let descriptor = fs::read(&paths.endpoint)?;
    let unavailable = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
    let unavailable_port = unavailable.local_addr()?.port();
    drop(unavailable);
    fs::write(
        &paths.endpoint,
        serde_json::to_vec(&json!({
            "schemaVersion": 1,
            "url": format!("ws://127.0.0.1:{unavailable_port}"),
        }))?,
    )?;

    for _ in 0..2 {
        let output = capture_cli(paths, repository, &["jump", FRESH_JUMP_WORKSPACE], None).await?;
        ensure!(
            !output.status.success(),
            "unreachable relay unexpectedly worked"
        );
        let error = String::from_utf8_lossy(&output.stderr);
        ensure!(
            error.contains("could not connect the TUI relay"),
            "jump failed for the wrong reason: {error}"
        );
        ensure!(
            !error.contains("already being opened"),
            "a failed relay leaked its temporary workspace lease: {error}"
        );
    }
    fs::write(&paths.endpoint, descriptor)?;
    Ok(())
}

fn verify_fresh_jump_arguments(
    arguments: &[String],
    daemon_endpoint: &str,
    worktree: &Path,
) -> Result<()> {
    ensure!(
        arguments.len() == 10,
        "unexpected fresh jump: {arguments:?}"
    );
    ensure!(
        arguments[0] == "--remote",
        "fresh jump did not use remote start"
    );
    ensure!(
        arguments[1].starts_with("ws://127.0.0.1:") && arguments[1] != daemon_endpoint,
        "fresh jump did not use its one-use loopback relay: {arguments:?}"
    );
    ensure!(
        arguments[2..]
            == [
                "--remote-auth-token-env",
                "COCO_CODEX_REMOTE_CAPABILITY_TOKEN",
                "-C",
                worktree
                    .to_str()
                    .context("fresh-jump worktree was not UTF-8")?,
                "--profile",
                PROFILE_NAME,
                "--model",
                MODEL_OVERRIDE,
            ],
        "fresh jump did not propagate cwd/profile/model: {arguments:?}"
    );
    Ok(())
}

fn verify_fresh_jump_requests(daemon: &[Value], tui: &[Value], worktree: &Path) -> Result<()> {
    let starts = tui
        .iter()
        .filter(|request| request["method"] == "thread/start")
        .collect::<Vec<_>>();
    ensure!(
        starts.len() == 2,
        "fresh TUI did not start two candidates: {tui:?}"
    );
    for start in starts {
        ensure!(
            start.pointer("/params/cwd").and_then(Value::as_str) == worktree.to_str(),
            "fresh TUI used the wrong cwd: {start}"
        );
    }
    ensure!(
        tui.iter().any(|request| {
            request["method"] == "turn/start"
                && request.pointer("/params/threadId") == Some(&json!(FRESH_ACTIVE_THREAD_ID))
        }),
        "fresh TUI did not materialize the selected candidate"
    );
    ensure!(
        daemon
            .iter()
            .all(|request| request["method"] != "thread/start"),
        "cocod fabricated a fresh thread instead of adopting the TUI thread"
    );
    ensure!(
        daemon.iter().any(|request| {
            request["method"] == "thread/read"
                && request.pointer("/params/threadId") == Some(&json!(FRESH_ACTIVE_THREAD_ID))
        }),
        "cocod never read the exact candidate to verify native materialization"
    );
    ensure!(
        daemon.iter().any(|request| {
            request["method"] == "thread/resume"
                && request.pointer("/params/threadId") == Some(&json!(FRESH_ACTIVE_THREAD_ID))
        }),
        "cocod did not subscribe after adopting the TUI thread"
    );
    ensure!(
        daemon
            .iter()
            .chain(tui)
            .all(|request| request["method"] != "turn/interrupt"),
        "leaving the fresh TUI interrupted its active turn"
    );
    Ok(())
}

async fn run_fake_fresh_jump_server(
    listener: TcpListener,
    observed_authorization: Arc<Mutex<Vec<String>>>,
    observed_daemon_requests: Arc<Mutex<Vec<Value>>>,
    observed_tui_requests: Arc<Mutex<Vec<Value>>>,
    state: Arc<Mutex<FreshServerState>>,
    completion: oneshot::Receiver<()>,
) -> Result<()> {
    let (stream, peer) = listener.accept().await?;
    ensure!(peer.ip().is_loopback(), "cocod did not connect locally");
    let websocket = accept_hdr_async(
        stream,
        CaptureAuthorization(Arc::clone(&observed_authorization)),
    )
    .await?;
    let daemon = handle_fresh_daemon_connection(
        websocket,
        observed_daemon_requests,
        Arc::clone(&state),
        completion,
    );
    let remote = async {
        for index in 0..2 {
            let (stream, peer) = listener.accept().await?;
            ensure!(peer.ip().is_loopback(), "relay did not connect locally");
            let websocket = accept_hdr_async(
                stream,
                CaptureAuthorization(Arc::clone(&observed_authorization)),
            )
            .await?;
            handle_fresh_tui_connection(
                websocket,
                index,
                Arc::clone(&observed_tui_requests),
                Arc::clone(&state),
            )
            .await?;
        }
        Ok::<(), anyhow::Error>(())
    };
    tokio::try_join!(daemon, remote)?;
    Ok(())
}

async fn handle_fresh_daemon_connection(
    mut websocket: WebSocketStream<TcpStream>,
    observed: Arc<Mutex<Vec<Value>>>,
    state: Arc<Mutex<FreshServerState>>,
    completion: oneshot::Receiver<()>,
) -> Result<()> {
    let mut completion = Box::pin(completion);
    let mut completion_sent = false;
    loop {
        tokio::select! {
            outcome = &mut completion, if !completion_sent && daemon_subscribed(&state) => {
                outcome.context("fresh completion sender disappeared")?;
                let thread_id = finish_active_thread(&state)?;
                send_json(&mut websocket, json!({
                    "method": "turn/completed",
                    "params": {
                        "threadId": thread_id,
                        "turn": {"id": FRESH_ACTIVE_TURN_ID, "status": "completed"}
                    }
                })).await?;
                completion_sent = true;
            }
            message = websocket.next() => {
                let Some(message) = message else { return Ok(()); };
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
                observed.lock().expect("fresh daemon mutex was poisoned").push(frame.clone());
                handle_fresh_daemon_request(&mut websocket, &frame, &state).await?;
            }
        }
    }
}

async fn handle_fresh_daemon_request(
    websocket: &mut WebSocketStream<TcpStream>,
    frame: &Value,
    state: &Arc<Mutex<FreshServerState>>,
) -> Result<()> {
    match frame.get("method").and_then(Value::as_str) {
        Some("initialize") => send_result(websocket, frame, json!({})).await?,
        Some("initialized") => {}
        Some("thread/list") => {
            let cwd = frame.pointer("/params/cwd").and_then(Value::as_str);
            let data = state
                .lock()
                .expect("fresh state mutex was poisoned")
                .threads
                .iter()
                .filter(|thread| thread.materialized && Some(thread.cwd.as_str()) == cwd)
                .map(thread_wire)
                .collect::<Vec<_>>();
            send_result(websocket, frame, json!({"data": data, "nextCursor": null})).await?;
        }
        Some("thread/name/set") => {
            let id = required_string(frame, "/params/threadId")?;
            let name = required_string(frame, "/params/name")?;
            set_thread_name(state, id, name)?;
            send_result(websocket, frame, json!({})).await?;
        }
        Some("thread/read") => {
            let id = required_string(frame, "/params/threadId")?;
            let thread = find_thread(state, id)?;
            send_result(websocket, frame, json!({"thread": thread_wire(&thread)})).await?;
        }
        Some("thread/resume") => {
            let id = required_string(frame, "/params/threadId")?;
            let thread = find_thread(state, id)?;
            ensure!(thread.materialized, "cocod resumed an empty TUI candidate");
            state
                .lock()
                .expect("fresh state mutex was poisoned")
                .daemon_subscribed = true;
            send_result(
                websocket,
                frame,
                json!({
                    "thread": {"id": thread.id, "status": native_status(&thread)},
                    "cwd": thread.cwd,
                    "model": MODEL_OVERRIDE,
                    "modelProvider": "test-provider"
                }),
            )
            .await?;
        }
        Some(other) => bail!("unexpected fresh daemon method {other:?}"),
        None => bail!("fresh daemon frame had no method: {frame}"),
    }
    Ok(())
}

async fn handle_fresh_tui_connection(
    mut websocket: WebSocketStream<TcpStream>,
    index: usize,
    observed: Arc<Mutex<Vec<Value>>>,
    state: Arc<Mutex<FreshServerState>>,
) -> Result<()> {
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
        observed
            .lock()
            .expect("fresh TUI mutex was poisoned")
            .push(frame.clone());
        match frame.get("method").and_then(Value::as_str) {
            Some("initialize") => send_result(&mut websocket, &frame, json!({})).await?,
            Some("initialized") => {}
            Some("thread/start") => {
                let id = if index == 0 {
                    FRESH_EMPTY_THREAD_ID
                } else {
                    FRESH_ACTIVE_THREAD_ID
                };
                let cwd = required_string(&frame, "/params/cwd")?.to_owned();
                state
                    .lock()
                    .expect("fresh state mutex was poisoned")
                    .threads
                    .push(FreshThread {
                        id: id.to_owned(),
                        cwd: cwd.clone(),
                        rollout_path: None,
                        name: String::new(),
                        materialized: false,
                        active: false,
                    });
                send_result(
                    &mut websocket,
                    &frame,
                    json!({
                        "thread": {"id": id, "status": {"type": "idle"}},
                        "cwd": cwd,
                        "model": MODEL_OVERRIDE,
                        "modelProvider": "test-provider"
                    }),
                )
                .await?;
            }
            Some("turn/start") => {
                ensure!(index == 1, "empty TUI unexpectedly started a turn");
                let id = required_string(&frame, "/params/threadId")?;
                activate_thread(&state, id)?;
                send_result(
                    &mut websocket,
                    &frame,
                    json!({"turn": {"id": FRESH_ACTIVE_TURN_ID}}),
                )
                .await?;
            }
            Some("thread/unsubscribe") => send_result(&mut websocket, &frame, json!({})).await?,
            Some(other) => bail!("unexpected fresh TUI method {other:?}"),
            None => bail!("fresh TUI frame had no method: {frame}"),
        }
    }
    Ok(())
}

fn required_string<'a>(frame: &'a Value, pointer: &str) -> Result<&'a str> {
    frame
        .pointer(pointer)
        .and_then(Value::as_str)
        .with_context(|| format!("request lacked {pointer}: {frame}"))
}

fn find_thread(state: &Arc<Mutex<FreshServerState>>, id: &str) -> Result<FreshThread> {
    state
        .lock()
        .expect("fresh state mutex was poisoned")
        .threads
        .iter()
        .find(|thread| thread.id == id)
        .cloned()
        .with_context(|| format!("fresh server did not know thread {id:?}"))
}

fn set_thread_name(state: &Arc<Mutex<FreshServerState>>, id: &str, name: &str) -> Result<()> {
    let mut state = state.lock().expect("fresh state mutex was poisoned");
    let thread = state
        .threads
        .iter_mut()
        .find(|thread| thread.id == id)
        .with_context(|| format!("fresh server did not know thread {id:?}"))?;
    thread.name = name.to_owned();
    Ok(())
}

fn activate_thread(state: &Arc<Mutex<FreshServerState>>, id: &str) -> Result<()> {
    let mut state = state.lock().expect("fresh state mutex was poisoned");
    let thread = state
        .threads
        .iter_mut()
        .find(|thread| thread.id == id)
        .with_context(|| format!("fresh server did not know thread {id:?}"))?;
    let rollout_path = Path::new(&thread.cwd).join(format!(".{id}.jsonl"));
    fs::write(&rollout_path, "{\"type\":\"session_meta\"}\n")?;
    thread.rollout_path = Some(rollout_path.to_string_lossy().into_owned());
    thread.materialized = true;
    thread.active = true;
    Ok(())
}

fn thread_wire(thread: &FreshThread) -> Value {
    json!({
        "id": thread.id,
        "cwd": thread.cwd,
        "path": thread.rollout_path,
        "name": thread.name,
        "status": native_status(thread),
        "forkedFromId": null,
        "turns": []
    })
}

fn native_status(thread: &FreshThread) -> Value {
    if thread.active {
        json!({"type": "active", "activeFlags": []})
    } else {
        json!({"type": "idle"})
    }
}

fn daemon_subscribed(state: &Arc<Mutex<FreshServerState>>) -> bool {
    state
        .lock()
        .expect("fresh state mutex was poisoned")
        .daemon_subscribed
}

fn finish_active_thread(state: &Arc<Mutex<FreshServerState>>) -> Result<String> {
    let mut state = state.lock().expect("fresh state mutex was poisoned");
    let thread = state
        .threads
        .iter_mut()
        .find(|thread| thread.id == FRESH_ACTIVE_THREAD_ID)
        .context("fresh active thread disappeared")?;
    thread.active = false;
    Ok(thread.id.clone())
}

#[tokio::test(flavor = "current_thread")]
async fn fake_tui_process() -> Result<()> {
    let Ok(endpoint) = env::var("COCO_TEST_FRESH_REMOTE") else {
        return Ok(());
    };
    let token = env::var("COCO_CODEX_REMOTE_CAPABILITY_TOKEN")?;
    let mode = env::var("COCO_TEST_FRESH_TUI_MODE")?;
    let address = endpoint
        .strip_prefix("ws://")
        .context("fake TUI remote endpoint was not ws://")?
        .parse::<SocketAddr>()?;
    let mut request = endpoint.into_client_request()?;
    request
        .headers_mut()
        .insert(AUTHORIZATION, format!("Bearer {token}").parse()?);
    let stream = TcpStream::connect(address).await?;
    let (mut websocket, _) = client_async(request, stream).await?;

    send_json(
        &mut websocket,
        json!({
            "id": 1,
            "method": "initialize",
            "params": {"clientInfo": {"name": "coco-fake-tui", "version": "0.0.0"}}
        }),
    )
    .await?;
    request_result(&mut websocket, json!(1)).await?;
    send_json(
        &mut websocket,
        json!({"method": "initialized", "params": {}}),
    )
    .await?;
    let cwd = env::current_dir()?;
    send_json(
        &mut websocket,
        json!({
            "id": 2,
            "method": "thread/start",
            "params": {"cwd": cwd, "config": {}, "ephemeral": false}
        }),
    )
    .await?;
    let started = request_result(&mut websocket, json!(2)).await?;
    let thread_id = started
        .pointer("/thread/id")
        .and_then(Value::as_str)
        .context("fake TUI thread/start returned no thread id")?
        .to_owned();

    if mode == "active" {
        send_json(
            &mut websocket,
            json!({
                "id": 3,
                "method": "turn/start",
                "params": {
                    "threadId": thread_id,
                    "cwd": cwd,
                    "clientUserMessageId": "fresh-tui-message",
                    "input": [{"type": "text", "text": "Keep working after I leave"}]
                }
            }),
        )
        .await?;
        request_result(&mut websocket, json!(3)).await?;
    } else {
        ensure!(mode == "empty", "unknown fake TUI mode {mode:?}");
    }

    send_json(
        &mut websocket,
        json!({
            "id": 4,
            "method": "thread/unsubscribe",
            "params": {"threadId": thread_id}
        }),
    )
    .await?;
    request_result(&mut websocket, json!(4)).await?;
    websocket.close(None).await?;
    Ok(())
}

async fn request_result(
    websocket: &mut WebSocketStream<TcpStream>,
    expected_id: Value,
) -> Result<Value> {
    while let Some(message) = websocket.next().await {
        let frame = match message? {
            Message::Text(text) => serde_json::from_str::<Value>(&text)?,
            Message::Binary(bytes) => serde_json::from_slice::<Value>(&bytes)?,
            Message::Ping(payload) => {
                websocket.send(Message::Pong(payload)).await?;
                continue;
            }
            Message::Pong(_) | Message::Frame(_) => continue,
            Message::Close(_) => bail!("fake TUI relay closed before responding"),
        };
        if frame.get("id") == Some(&expected_id) {
            ensure!(
                frame.get("error").is_none(),
                "fake TUI request failed: {frame}"
            );
            return frame
                .get("result")
                .cloned()
                .context("fake TUI response had no result");
        }
    }
    bail!("fake TUI relay disconnected before responding")
}
