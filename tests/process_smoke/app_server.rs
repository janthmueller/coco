use super::*;

pub(super) struct CaptureAuthorization(Arc<Mutex<Vec<String>>>);

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

pub(super) async fn run_fake_app_server(
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

#[expect(
    clippy::too_many_lines,
    reason = "one fake App Server loop keeps fork/read/compact/turn ordering visible"
)]
pub(super) async fn run_fake_fork_server(
    listener: TcpListener,
    observed_authorization: Arc<Mutex<Vec<String>>>,
    observed_requests: Arc<Mutex<Vec<Value>>>,
) -> Result<()> {
    let (stream, peer) = listener.accept().await?;
    ensure!(
        peer.ip().is_loopback(),
        "cocod fork connection was not local"
    );
    let mut websocket =
        accept_hdr_async(stream, CaptureAuthorization(observed_authorization)).await?;
    let mut source_cwd = Value::Null;
    let mut child_cwd = Value::Null;
    let mut child_active = false;
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
            .expect("fork request capture mutex was poisoned")
            .push(frame.clone());
        match frame.get("method").and_then(Value::as_str) {
            Some("initialize") => send_result(&mut websocket, &frame, json!({})).await?,
            Some("initialized") => {}
            Some("thread/start") => {
                source_cwd = frame.pointer("/params/cwd").cloned().unwrap_or(Value::Null);
                send_result(
                    &mut websocket,
                    &frame,
                    idle_thread_response(&frame, FORK_SOURCE_THREAD_ID, DEFAULT_MODEL),
                )
                .await?;
            }
            Some("thread/name/set") => send_result(&mut websocket, &frame, json!({})).await?,
            Some("thread/fork") => {
                child_cwd = frame.pointer("/params/cwd").cloned().unwrap_or(Value::Null);
                send_result(
                    &mut websocket,
                    &frame,
                    idle_thread_response(&frame, FORK_CHILD_THREAD_ID, MODEL_OVERRIDE),
                )
                .await?;
            }
            Some("thread/read") => {
                let thread_id = frame
                    .pointer("/params/threadId")
                    .and_then(Value::as_str)
                    .context("thread/read had no threadId")?;
                let (name, cwd, status, forked_from_id, turns) = match thread_id {
                    FORK_SOURCE_THREAD_ID => (
                        FORK_SOURCE_WORKSPACE,
                        &source_cwd,
                        json!({"type": "idle"}),
                        None,
                        Vec::new(),
                    ),
                    FORK_CHILD_THREAD_ID => (
                        FORK_CHILD_WORKSPACE,
                        &child_cwd,
                        if child_active {
                            json!({"type": "active", "activeFlags": []})
                        } else {
                            json!({"type": "idle"})
                        },
                        Some(FORK_SOURCE_THREAD_ID),
                        if child_active {
                            vec![json!({
                                "id": "turn-fork-child",
                                "status": "inProgress",
                                "items": []
                            })]
                        } else {
                            Vec::new()
                        },
                    ),
                    other => bail!("thread/read used unknown fork thread {other:?}"),
                };
                let result =
                    fake_thread_read(&frame, thread_id, name, cwd, status, &turns, forked_from_id);
                send_result(&mut websocket, &frame, result).await?;
            }
            Some("thread/compact/start") => {
                send_result(&mut websocket, &frame, json!({})).await?;
                for notification in [
                    json!({
                        "method": "turn/started",
                        "params": {
                            "threadId": FORK_CHILD_THREAD_ID,
                            "turn": {"id": "turn-fork-compact", "status": "inProgress"}
                        }
                    }),
                    json!({
                        "method": "item/completed",
                        "params": {
                            "threadId": FORK_CHILD_THREAD_ID,
                            "turnId": "turn-fork-compact",
                            "item": {"id": "item-fork-compact", "type": "contextCompaction"}
                        }
                    }),
                    json!({
                        "method": "turn/completed",
                        "params": {
                            "threadId": FORK_CHILD_THREAD_ID,
                            "turn": {"id": "turn-fork-compact", "status": "completed"}
                        }
                    }),
                ] {
                    send_json(&mut websocket, notification).await?;
                }
            }
            Some("turn/start") => {
                child_active = true;
                send_result(
                    &mut websocket,
                    &frame,
                    json!({"turn": {"id": "turn-fork-child"}}),
                )
                .await?;
            }
            Some(other) => bail!("unexpected fork App Server method {other:?}"),
            None => bail!("received a fork App Server frame without a method: {frame}"),
        }
    }
    Ok(())
}

pub(super) fn idle_thread_response(request: &Value, thread_id: &str, model: &str) -> Value {
    json!({
        "thread": {
            "id": thread_id,
            "status": {"type": "idle"}
        },
        "cwd": request.pointer("/params/cwd").cloned().unwrap_or(Value::Null),
        "model": model,
        "modelProvider": "test-provider",
    })
}

pub(super) async fn run_fake_recovery_server(
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
    let mut loaded = false;
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
                loaded = true;
                send_result(
                    &mut websocket,
                    &frame,
                    json!({
                        "thread": {"id": THREAD_ID, "status": {"type": "idle"}},
                        "cwd": expected_cwd,
                        "model": MODEL_OVERRIDE,
                        "modelProvider": "test-provider",
                    }),
                )
                .await?;
            }
            Some("thread/read") => {
                let cwd = json!(&expected_cwd);
                let result = fake_thread_read(
                    &frame,
                    THREAD_ID,
                    WORKSPACE_NAME,
                    &cwd,
                    if loaded {
                        json!({"type": "idle"})
                    } else {
                        json!({"type": "notLoaded"})
                    },
                    &[],
                    None,
                );
                send_result(&mut websocket, &frame, result).await?;
            }
            Some(other) => bail!("unexpected recovery App Server method {other:?}"),
            None => bail!("received a recovery frame without a method: {frame}"),
        }
    }
    Ok(())
}

pub(super) async fn handle_daemon_connection(
    mut websocket: WebSocketStream<TcpStream>,
    observed_requests: Arc<Mutex<Vec<Value>>>,
    completion: oneshot::Receiver<()>,
) -> Result<()> {
    let mut completion = Some(completion);
    let mut thread_cwd = Value::Null;
    let mut completed_turns = Vec::new();

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
            Some("model/list") => {
                send_result(&mut websocket, &frame, fake_model_page(&frame)?).await?;
            }
            Some("thread/start") => {
                let cwd = frame.pointer("/params/cwd").cloned().unwrap_or(Value::Null);
                thread_cwd.clone_from(&cwd);
                send_result(
                    &mut websocket,
                    &frame,
                    json!({
                        "thread": {"id": THREAD_ID, "status": {"type": "idle"}},
                        "cwd": cwd,
                        "model": MODEL_OVERRIDE,
                        "modelProvider": "test-provider",
                    }),
                )
                .await?;
            }
            Some("thread/read") => {
                send_result(
                    &mut websocket,
                    &frame,
                    fake_thread_read(
                        &frame,
                        THREAD_ID,
                        WORKSPACE_NAME,
                        &thread_cwd,
                        json!({"type": "idle"}),
                        &completed_turns,
                        None,
                    ),
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
                completed_turns = complete_fake_turn(
                    &mut websocket,
                    &frame,
                    completion,
                    &observed_requests,
                    &thread_cwd,
                )
                .await?;
            }
            Some(other) => bail!("unexpected App Server method {other:?}"),
            None => bail!("received an App Server frame without a method: {frame}"),
        }
    }
    Ok(())
}

pub(super) async fn complete_fake_turn(
    websocket: &mut WebSocketStream<TcpStream>,
    request: &Value,
    completion: oneshot::Receiver<()>,
    observed_requests: &Arc<Mutex<Vec<Value>>>,
    thread_cwd: &Value,
) -> Result<Vec<Value>> {
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
    complete_fake_approval(websocket, request, observed_requests, thread_cwd).await?;
    await_completion_while_serving_reads(websocket, completion, observed_requests, thread_cwd)
        .await?;
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
    .await?;
    Ok(vec![json!({
        "id": TURN_ID,
        "status": "completed",
        "items": [{
            "id": "message-process-smoke",
            "type": "agentMessage",
            "text": "Fake Codex completed the turn."
        }]
    })])
}

pub(super) async fn complete_fake_approval(
    websocket: &mut WebSocketStream<TcpStream>,
    turn_request: &Value,
    observed_requests: &Arc<Mutex<Vec<Value>>>,
    thread_cwd: &Value,
) -> Result<()> {
    send_json(
        websocket,
        json!({
            "id": 900,
            "method": "item/commandExecution/requestApproval",
            "params": {
                "threadId": THREAD_ID,
                "turnId": TURN_ID,
                "itemId": "command-process-smoke",
                "startedAtMs": 10,
                "command": "git status --short",
                "cwd": turn_request.pointer("/params/cwd"),
                "reason": "Verify the worktree before continuing",
                "availableDecisions": ["accept", "decline"]
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
                "status": {"type": "active", "activeFlags": ["waitingOnApproval"]}
            }
        }),
    )
    .await?;
    let response = await_daemon_response(
        websocket,
        json!(900),
        observed_requests,
        thread_cwd,
        json!({
            "type": "active",
            "activeFlags": ["waitingOnApproval"]
        }),
    )
    .await?;
    observed_requests
        .lock()
        .expect("request capture mutex was poisoned")
        .push(response.clone());
    assert_eq!(response["result"], json!({"decision": "accept"}));
    send_json(
        websocket,
        json!({
            "method": "serverRequest/resolved",
            "params": {"threadId": THREAD_ID, "requestId": 900}
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
    .await
}

pub(super) async fn await_daemon_response(
    websocket: &mut WebSocketStream<TcpStream>,
    expected_id: Value,
    observed_requests: &Arc<Mutex<Vec<Value>>>,
    thread_cwd: &Value,
    status: Value,
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
            Message::Close(_) => bail!("daemon closed before answering a server request"),
        };
        if frame.get("method") == Some(&json!("thread/read")) {
            observed_requests
                .lock()
                .expect("request capture mutex was poisoned")
                .push(frame.clone());
            let result = fake_thread_read(
                &frame,
                THREAD_ID,
                WORKSPACE_NAME,
                thread_cwd,
                status.clone(),
                &[json!({
                    "id": TURN_ID,
                    "status": "inProgress",
                    "items": []
                })],
                None,
            );
            send_result(websocket, &frame, result).await?;
        } else if frame.get("id") == Some(&expected_id) {
            return Ok(frame);
        } else {
            bail!("unexpected daemon frame while awaiting a server response: {frame}");
        }
    }
    bail!("daemon disconnected before answering a server request")
}

pub(super) async fn await_completion_while_serving_reads(
    websocket: &mut WebSocketStream<TcpStream>,
    mut completion: oneshot::Receiver<()>,
    observed_requests: &Arc<Mutex<Vec<Value>>>,
    thread_cwd: &Value,
) -> Result<()> {
    loop {
        tokio::select! {
            completed = &mut completion => {
                completed.context("test stopped before allowing turn completion")?;
                return Ok(());
            }
            message = websocket.next() => {
                let frame = match message.context("daemon disconnected before turn completion")?? {
                    Message::Text(text) => serde_json::from_str::<Value>(&text)?,
                    Message::Binary(bytes) => serde_json::from_slice::<Value>(&bytes)?,
                    Message::Close(_) => bail!("daemon closed before turn completion"),
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
                ensure!(
                    frame.get("method") == Some(&json!("thread/read")),
                    "unexpected daemon frame while awaiting turn completion: {frame}"
                );
                let result = fake_thread_read(
                    &frame,
                    THREAD_ID,
                    WORKSPACE_NAME,
                    thread_cwd,
                    json!({"type": "active", "activeFlags": []}),
                    &[json!({
                        "id": TURN_ID,
                        "status": "inProgress",
                        "items": []
                    })],
                    None,
                );
                send_result(websocket, &frame, result).await?;
            }
        }
    }
}

pub(super) fn fake_thread_read(
    request: &Value,
    thread_id: &str,
    name: &str,
    cwd: &Value,
    status: Value,
    turns: &[Value],
    forked_from_id: Option<&str>,
) -> Value {
    let turns = if request
        .pointer("/params/includeTurns")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        turns.to_vec()
    } else {
        Vec::new()
    };
    json!({
        "thread": {
            "id": thread_id,
            "cwd": cwd,
            "name": name,
            "status": status,
            "forkedFromId": forked_from_id,
            "turns": turns,
        }
    })
}

pub(super) async fn handle_remote_connection(
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

pub(super) fn fake_model_page(request: &Value) -> Result<Value> {
    match request.pointer("/params/cursor").and_then(Value::as_str) {
        None => Ok(json!({
            "data": [{
                "id": DEFAULT_MODEL,
                "model": DEFAULT_MODEL,
                "displayName": "GPT Default",
                "description": "Default test model",
                "hidden": false,
                "isDefault": true,
                "defaultReasoningEffort": "medium",
                "supportedReasoningEfforts": [{
                    "reasoningEffort": "medium",
                    "description": "Balanced",
                }],
                "inputModalities": ["text", "image"],
                "supportsPersonality": true,
            }],
            "nextCursor": "models-page-2",
        })),
        Some("models-page-2") => Ok(json!({
            "data": [{
                "id": MODEL_OVERRIDE,
                "model": MODEL_OVERRIDE,
                "displayName": "GPT Explicit",
                "description": "Explicit test model",
                "hidden": false,
                "isDefault": false,
                "defaultReasoningEffort": "high",
                "supportedReasoningEfforts": [
                    {"reasoningEffort": "medium", "description": "Balanced"},
                    {"reasoningEffort": "high", "description": "Thorough"},
                ],
                "inputModalities": ["text", "image"],
                "supportsPersonality": true,
            }],
            "nextCursor": null,
        })),
        Some(cursor) => bail!("unexpected model-list cursor {cursor:?}"),
    }
}

pub(super) async fn send_result(
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

pub(super) async fn send_json(
    websocket: &mut WebSocketStream<TcpStream>,
    value: Value,
) -> Result<()> {
    websocket
        .send(Message::Text(serde_json::to_string(&value)?.into()))
        .await?;
    Ok(())
}

pub(super) async fn remote_tui_session(endpoint: &str, token: &str, graceful: bool) -> Result<()> {
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

pub(super) async fn await_result(
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
