use std::collections::HashMap;
use std::io;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, Weak};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Map, Value, json};
use thiserror::Error;
use tokio::io::{
    AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader, DuplexStream,
};
use tokio::net::UnixStream;
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{WebSocketStream, client_async};

pub const DEFAULT_MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;
pub const DEFAULT_EVENT_BUFFER: usize = 256;

const STDERR_TAIL_BYTES: usize = 64 * 1024;
const APP_SERVER_STARTUP_TIMEOUT: Duration = Duration::from_secs(10);
const APP_SERVER_CONNECT_RETRY: Duration = Duration::from_millis(25);

pub type RequestId = Value;

#[derive(Debug, Clone)]
pub struct CodexClientOptions {
    pub codex_binary: PathBuf,
    /// When set, spawn one shared App Server on this private Unix socket and
    /// connect this client to it. Other trusted local clients, such as the
    /// Codex TUI opened by `coco jump`, can subscribe to the same threads.
    pub app_server_socket: Option<PathBuf>,
    /// Explicit Codex home for the spawned server. `None` inherits the current
    /// process environment.
    pub codex_home: Option<PathBuf>,
    pub max_message_bytes: usize,
    pub event_buffer: usize,
    pub client_name: String,
    pub client_version: String,
}

impl Default for CodexClientOptions {
    fn default() -> Self {
        Self {
            codex_binary: PathBuf::from("codex"),
            app_server_socket: None,
            codex_home: None,
            max_message_bytes: DEFAULT_MAX_MESSAGE_BYTES,
            event_buffer: DEFAULT_EVENT_BUFFER,
            client_name: "coco".to_owned(),
            client_version: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CodexEvent {
    Notification {
        method: String,
        params: Value,
    },
    ServerRequest {
        id: RequestId,
        method: String,
        params: Value,
    },
}

#[derive(Debug, Clone, Error, PartialEq)]
pub enum CodexError {
    #[error("invalid Codex client options: {0}")]
    InvalidOptions(String),

    #[error("failed to spawn Codex app-server: {message}")]
    Spawn { message: String },

    #[error("Codex app-server connection is closed: {reason}; stderr: {stderr}")]
    Closed { reason: String, stderr: String },

    #[error("Codex app-server I/O failed while {operation}: {message}; stderr: {stderr}")]
    Io {
        operation: &'static str,
        message: String,
        stderr: String,
    },

    #[error("Codex app-server protocol error: {message}; stderr: {stderr}")]
    Protocol { message: String, stderr: String },

    #[error("Codex app-server message is {observed} bytes, exceeding the {limit}-byte limit")]
    MessageTooLarge { observed: usize, limit: usize },

    #[error("Codex app-server request failed ({code}): {message}")]
    Rpc {
        code: i64,
        message: String,
        data: Option<Value>,
    },
}

#[derive(Clone)]
pub struct CodexClient {
    inner: Arc<Inner>,
    initialize_result: Arc<OnceLock<Value>>,
}

impl std::fmt::Debug for CodexClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CodexClient")
            .field("initialize_result", &self.initialize_result.get())
            .finish_non_exhaustive()
    }
}

struct Inner {
    writer: Mutex<Pin<Box<dyn AsyncWrite + Send>>>,
    state: Mutex<ConnectionState>,
    stderr_tail: Arc<Mutex<StderrTail>>,
    max_message_bytes: usize,
    next_id: AtomicU64,
    shutdown: watch::Sender<bool>,
    tasks: Mutex<TaskHandles>,
}

struct ConnectionState {
    failure: Option<CodexError>,
    pending: HashMap<String, oneshot::Sender<Result<Value, CodexError>>>,
}

#[derive(Default)]
struct TaskHandles {
    reader: Option<JoinHandle<()>>,
    stderr: Option<JoinHandle<()>>,
    process: Option<JoinHandle<()>>,
    bridge: Option<JoinHandle<()>>,
}

struct StderrTail {
    bytes: Vec<u8>,
    limit: usize,
}

impl StderrTail {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
        }
    }

    fn push(&mut self, chunk: &[u8]) {
        if chunk.len() >= self.limit {
            self.bytes.clear();
            self.bytes
                .extend_from_slice(&chunk[chunk.len() - self.limit..]);
            return;
        }

        let overflow = self
            .bytes
            .len()
            .saturating_add(chunk.len())
            .saturating_sub(self.limit);
        if overflow > 0 {
            self.bytes.drain(..overflow);
        }
        self.bytes.extend_from_slice(chunk);
    }

    fn display(&self) -> String {
        if self.bytes.is_empty() {
            "(no stderr captured)".to_owned()
        } else {
            String::from_utf8_lossy(&self.bytes).trim().to_owned()
        }
    }
}

impl CodexClient {
    /// Starts the configured Codex App Server and completes the mandatory
    /// `initialize` / `initialized` handshake before returning.
    ///
    /// Without `app_server_socket` the child uses its private stdio transport.
    /// With a socket configured, the child listens on that Unix socket and the
    /// client bridges its JSONL codec to one WebSocket connection.
    pub async fn spawn(
        options: CodexClientOptions,
    ) -> Result<(Self, mpsc::Receiver<CodexEvent>), CodexError> {
        validate_options(&options)?;

        if let Some(socket_path) = options.app_server_socket.clone() {
            return Self::spawn_shared_unix(options, socket_path).await;
        }

        Self::spawn_stdio(options).await
    }

    async fn spawn_stdio(
        options: CodexClientOptions,
    ) -> Result<(Self, mpsc::Receiver<CodexEvent>), CodexError> {
        let mut command = Command::new(&options.codex_binary);
        command
            .args(["app-server", "--listen", "stdio://"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        apply_codex_home(&mut command, options.codex_home.as_ref());

        let mut child = command.spawn().map_err(|error| CodexError::Spawn {
            message: error.to_string(),
        })?;
        let stdin = child.stdin.take().ok_or_else(|| CodexError::Spawn {
            message: "spawned process did not expose stdin".to_owned(),
        })?;
        let stdout = child.stdout.take().ok_or_else(|| CodexError::Spawn {
            message: "spawned process did not expose stdout".to_owned(),
        })?;
        let stderr = child.stderr.take().ok_or_else(|| CodexError::Spawn {
            message: "spawned process did not expose stderr".to_owned(),
        })?;

        let stderr_tail = Arc::new(Mutex::new(StderrTail::new(STDERR_TAIL_BYTES)));
        let (client, events, shutdown_receiver) = Self::from_io(
            stdout,
            stdin,
            options.max_message_bytes,
            options.event_buffer,
            Arc::clone(&stderr_tail),
        )
        .await;

        let stderr_task = tokio::spawn(collect_stderr(stderr, Arc::clone(&stderr_tail)));
        let process_task = tokio::spawn(monitor_child(
            child,
            shutdown_receiver,
            Arc::downgrade(&client.inner),
        ));
        {
            let mut tasks = client.inner.tasks.lock().await;
            tasks.stderr = Some(stderr_task);
            tasks.process = Some(process_task);
        }

        initialize_client(&client, &options).await?;

        Ok((client, events))
    }

    async fn spawn_shared_unix(
        options: CodexClientOptions,
        socket_path: PathBuf,
    ) -> Result<(Self, mpsc::Receiver<CodexEvent>), CodexError> {
        prepare_app_server_socket(&socket_path).await?;
        if let Some(codex_home) = options.codex_home.as_ref() {
            tokio::fs::create_dir_all(codex_home)
                .await
                .map_err(|error| CodexError::Spawn {
                    message: format!(
                        "could not create Codex home {}: {error}",
                        codex_home.display()
                    ),
                })?;
        }

        let listen = format!("unix://{}", socket_path.display());
        let mut command = Command::new(&options.codex_binary);
        command
            .args(["app-server", "--listen"])
            .arg(&listen)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        apply_codex_home(&mut command, options.codex_home.as_ref());

        let mut child = command.spawn().map_err(|error| CodexError::Spawn {
            message: error.to_string(),
        })?;
        let stderr = child.stderr.take().ok_or_else(|| CodexError::Spawn {
            message: "spawned App Server did not expose stderr".to_owned(),
        })?;
        let stderr_tail = Arc::new(Mutex::new(StderrTail::new(STDERR_TAIL_BYTES)));
        let stderr_task = tokio::spawn(collect_stderr(stderr, Arc::clone(&stderr_tail)));

        let websocket = match connect_app_server(&socket_path).await {
            Ok(websocket) => websocket,
            Err(error) => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                let _ = stderr_task.await;
                return Err(CodexError::Spawn {
                    message: format!(
                        "could not connect to App Server socket {}: {error}; stderr: {}",
                        socket_path.display(),
                        stderr_tail.lock().await.display()
                    ),
                });
            }
        };

        let (client_io, bridge_io) = tokio::io::duplex(64 * 1024);
        let (reader, writer) = tokio::io::split(client_io);
        let (client, events, shutdown_receiver) = Self::from_io(
            reader,
            writer,
            options.max_message_bytes,
            options.event_buffer,
            Arc::clone(&stderr_tail),
        )
        .await;
        let bridge_task = tokio::spawn(bridge_jsonl_websocket(
            bridge_io,
            websocket,
            Arc::clone(&stderr_tail),
        ));
        let process_task = tokio::spawn(monitor_child(
            child,
            shutdown_receiver,
            Arc::downgrade(&client.inner),
        ));
        {
            let mut tasks = client.inner.tasks.lock().await;
            tasks.stderr = Some(stderr_task);
            tasks.process = Some(process_task);
            tasks.bridge = Some(bridge_task);
        }

        initialize_client(&client, &options).await?;

        Ok((client, events))
    }

    pub fn initialize_result(&self) -> Option<&Value> {
        self.initialize_result.get()
    }

    pub async fn request(
        &self,
        method: impl Into<String>,
        params: Value,
    ) -> Result<Value, CodexError> {
        let method = validate_method(method.into())?;
        let id = format!(
            "coco-{}",
            self.inner.next_id.fetch_add(1, Ordering::Relaxed)
        );
        let (sender, receiver) = oneshot::channel();

        {
            let mut state = self.inner.state.lock().await;
            if let Some(error) = &state.failure {
                return Err(error.clone());
            }
            state.pending.insert(id.clone(), sender);
        }

        let frame = json!({
            "id": id,
            "method": method,
            "params": params,
        });
        if let Err(error) = self.inner.write_frame(&frame).await {
            self.inner.state.lock().await.pending.remove(&id);
            return Err(error);
        }

        match receiver.await {
            Ok(result) => result,
            Err(_) => Err(self.inner.current_failure().await),
        }
    }

    pub async fn notify(
        &self,
        method: impl Into<String>,
        params: Option<Value>,
    ) -> Result<(), CodexError> {
        let method = validate_method(method.into())?;
        let mut frame = Map::new();
        frame.insert("method".to_owned(), Value::String(method));
        if let Some(params) = params {
            frame.insert("params".to_owned(), params);
        }
        self.inner.write_frame(&Value::Object(frame)).await
    }

    /// Explicitly answers a server-initiated request. Incoming requests are
    /// only emitted as events and are never answered automatically.
    pub async fn respond(&self, id: RequestId, result: Value) -> Result<(), CodexError> {
        validate_request_id(&id)?;
        self.inner
            .write_frame(&json!({ "id": id, "result": result }))
            .await
    }

    /// Explicitly rejects a server-initiated request.
    pub async fn respond_error(
        &self,
        id: RequestId,
        code: i64,
        message: impl Into<String>,
        data: Option<Value>,
    ) -> Result<(), CodexError> {
        validate_request_id(&id)?;
        let mut error = Map::new();
        error.insert("code".to_owned(), Value::from(code));
        error.insert("message".to_owned(), Value::String(message.into()));
        if let Some(data) = data {
            error.insert("data".to_owned(), data);
        }
        self.inner
            .write_frame(&json!({ "id": id, "error": error }))
            .await
    }

    pub async fn stderr_context(&self) -> String {
        self.inner.stderr_context().await
    }

    /// Closes the transport, terminates the owned App Server if it is still
    /// running, waits for its I/O tasks, and rejects outstanding requests.
    pub async fn close(&self) -> Result<(), CodexError> {
        let close_error = CodexError::Closed {
            reason: "closed by client".to_owned(),
            stderr: self.inner.stderr_context().await,
        };
        self.inner.fail(close_error, false).await;

        {
            let mut writer = self.inner.writer.lock().await;
            let _ = writer.as_mut().shutdown().await;
        }
        let _ = self.inner.shutdown.send(true);

        let mut tasks = std::mem::take(&mut *self.inner.tasks.lock().await);
        let process = tasks.process.take();
        let had_process = process.is_some();
        if let Some(process) = process {
            let _ = process.await;
        }
        if let Some(bridge) = tasks.bridge.take() {
            if !bridge.is_finished() {
                bridge.abort();
            }
            let _ = bridge.await;
        }
        if let Some(reader) = tasks.reader.take() {
            if !had_process && !reader.is_finished() {
                reader.abort();
            }
            let _ = reader.await;
        }
        if let Some(stderr) = tasks.stderr.take() {
            let _ = stderr.await;
        }
        Ok(())
    }

    async fn from_io<R, W>(
        reader: R,
        writer: W,
        max_message_bytes: usize,
        event_buffer: usize,
        stderr_tail: Arc<Mutex<StderrTail>>,
    ) -> (Self, mpsc::Receiver<CodexEvent>, watch::Receiver<bool>)
    where
        R: AsyncRead + Send + Unpin + 'static,
        W: AsyncWrite + Send + Unpin + 'static,
    {
        let (event_sender, event_receiver) = mpsc::channel(event_buffer);
        let (shutdown, shutdown_receiver) = watch::channel(false);
        let inner = Arc::new(Inner {
            writer: Mutex::new(Box::pin(writer)),
            state: Mutex::new(ConnectionState {
                failure: None,
                pending: HashMap::new(),
            }),
            stderr_tail,
            max_message_bytes,
            next_id: AtomicU64::new(1),
            shutdown,
            tasks: Mutex::new(TaskHandles::default()),
        });
        let reader_task = tokio::spawn(read_stdout(
            reader,
            Arc::downgrade(&inner),
            event_sender,
            max_message_bytes,
            event_buffer,
        ));
        inner.tasks.lock().await.reader = Some(reader_task);

        (
            Self {
                inner,
                initialize_result: Arc::new(OnceLock::new()),
            },
            event_receiver,
            shutdown_receiver,
        )
    }
}

fn apply_codex_home(command: &mut Command, codex_home: Option<&PathBuf>) {
    if let Some(codex_home) = codex_home {
        command.env("CODEX_HOME", codex_home);
    }
}

async fn prepare_app_server_socket(socket_path: &PathBuf) -> Result<(), CodexError> {
    if !socket_path.is_absolute() {
        return Err(CodexError::InvalidOptions(
            "app_server_socket must be absolute".to_owned(),
        ));
    }
    let parent = socket_path.parent().ok_or_else(|| {
        CodexError::InvalidOptions("app_server_socket has no parent directory".to_owned())
    })?;
    let parent_exists = tokio::fs::metadata(parent).await.is_ok();
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|error| CodexError::Spawn {
            message: format!(
                "could not create App Server socket directory {}: {error}",
                parent.display()
            ),
        })?;
    if !parent_exists {
        tokio::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
            .await
            .map_err(|error| CodexError::Spawn {
                message: format!(
                    "could not secure App Server socket directory {}: {error}",
                    parent.display()
                ),
            })?;
    }

    let metadata = match tokio::fs::symlink_metadata(socket_path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(CodexError::Spawn {
                message: format!(
                    "could not inspect App Server socket {}: {error}",
                    socket_path.display()
                ),
            });
        }
    };
    if !metadata.file_type().is_socket() {
        return Err(CodexError::Spawn {
            message: format!(
                "refusing to replace non-socket App Server path {}",
                socket_path.display()
            ),
        });
    }
    match UnixStream::connect(socket_path).await {
        Ok(_) => Err(CodexError::Spawn {
            message: format!(
                "an App Server is already listening at {}",
                socket_path.display()
            ),
        }),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound
            ) =>
        {
            tokio::fs::remove_file(socket_path)
                .await
                .map_err(|error| CodexError::Spawn {
                    message: format!(
                        "could not remove stale App Server socket {}: {error}",
                        socket_path.display()
                    ),
                })
        }
        Err(error) => Err(CodexError::Spawn {
            message: format!(
                "could not validate App Server socket {}: {error}",
                socket_path.display()
            ),
        }),
    }
}

async fn connect_app_server(socket_path: &PathBuf) -> Result<WebSocketStream<UnixStream>, String> {
    let deadline = Instant::now() + APP_SERVER_STARTUP_TIMEOUT;
    loop {
        let error = match UnixStream::connect(socket_path).await {
            Ok(stream) => match client_async("ws://localhost/", stream).await {
                Ok((websocket, _)) => return Ok(websocket),
                Err(error) => format!("WebSocket handshake failed: {error}"),
            },
            Err(error) => error.to_string(),
        };
        if Instant::now() >= deadline {
            return Err(error);
        }
        tokio::time::sleep(APP_SERVER_CONNECT_RETRY).await;
    }
}

async fn bridge_jsonl_websocket(
    io: DuplexStream,
    websocket: WebSocketStream<UnixStream>,
    stderr_tail: Arc<Mutex<StderrTail>>,
) {
    let (json_reader, mut json_writer) = tokio::io::split(io);
    let mut json_reader = BufReader::new(json_reader);
    let (mut websocket_writer, mut websocket_reader) = websocket.split();

    let outbound = async {
        let mut frame = Vec::new();
        loop {
            frame.clear();
            let read = json_reader
                .read_until(b'\n', &mut frame)
                .await
                .map_err(|error| format!("could not read JSONL frame: {error}"))?;
            if read == 0 {
                let _ = websocket_writer.close().await;
                return Ok::<(), String>(());
            }
            if frame.last() == Some(&b'\n') {
                frame.pop();
            }
            if frame.last() == Some(&b'\r') {
                frame.pop();
            }
            if frame.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            let text = String::from_utf8(frame.clone())
                .map_err(|_| "outbound JSONL frame is not UTF-8".to_owned())?;
            websocket_writer
                .send(Message::Text(text.into()))
                .await
                .map_err(|error| format!("could not write WebSocket frame: {error}"))?;
        }
    };

    let inbound = async {
        while let Some(message) = websocket_reader.next().await {
            let message =
                message.map_err(|error| format!("could not read WebSocket frame: {error}"))?;
            match message {
                Message::Text(text) => {
                    json_writer
                        .write_all(text.as_bytes())
                        .await
                        .map_err(|error| format!("could not write JSONL frame: {error}"))?;
                    json_writer
                        .write_all(b"\n")
                        .await
                        .map_err(|error| format!("could not terminate JSONL frame: {error}"))?;
                }
                Message::Binary(bytes) => {
                    json_writer
                        .write_all(&bytes)
                        .await
                        .map_err(|error| format!("could not write binary JSON frame: {error}"))?;
                    json_writer
                        .write_all(b"\n")
                        .await
                        .map_err(|error| format!("could not terminate JSONL frame: {error}"))?;
                }
                Message::Close(_) => return Ok::<(), String>(()),
                Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {}
            }
        }
        Ok::<(), String>(())
    };

    let result = tokio::select! {
        result = outbound => result,
        result = inbound => result,
    };
    if let Err(error) = result {
        stderr_tail
            .lock()
            .await
            .push(format!("\n[App Server socket bridge failed: {error}]").as_bytes());
    }
}

async fn initialize_client(
    client: &CodexClient,
    options: &CodexClientOptions,
) -> Result<(), CodexError> {
    let initialize = client
        .request(
            "initialize",
            json!({
                "clientInfo": {
                    "name": options.client_name,
                    "version": options.client_version,
                }
            }),
        )
        .await;
    let initialize = match initialize {
        Ok(value) => value,
        Err(error) => {
            let _ = client.close().await;
            return Err(error);
        }
    };
    let _ = client.initialize_result.set(initialize);

    if let Err(error) = client.notify("initialized", None).await {
        let _ = client.close().await;
        return Err(error);
    }
    Ok(())
}

impl Inner {
    async fn write_frame(&self, value: &Value) -> Result<(), CodexError> {
        let mut encoded = serde_json::to_vec(value).map_err(|error| CodexError::Protocol {
            message: format!("could not encode outbound message: {error}"),
            stderr: "(not applicable)".to_owned(),
        })?;
        if encoded.len() > self.max_message_bytes {
            return Err(CodexError::MessageTooLarge {
                observed: encoded.len(),
                limit: self.max_message_bytes,
            });
        }
        encoded.push(b'\n');

        {
            let state = self.state.lock().await;
            if let Some(error) = &state.failure {
                return Err(error.clone());
            }
        }

        let result = {
            let mut writer = self.writer.lock().await;
            writer.as_mut().write_all(&encoded).await
        };
        if let Err(error) = result {
            let failure = CodexError::Io {
                operation: "writing stdin",
                message: error.to_string(),
                stderr: self.stderr_context().await,
            };
            self.fail(failure.clone(), true).await;
            return Err(failure);
        }
        Ok(())
    }

    async fn handle_message(
        &self,
        value: Value,
        events: &mpsc::Sender<CodexEvent>,
        event_buffer: usize,
    ) -> Result<(), String> {
        let object = value
            .as_object()
            .ok_or_else(|| "top-level message is not an object".to_owned())?;
        let id = object.get("id");
        let method = object.get("method").and_then(Value::as_str);

        if let Some(method) = method {
            if object.contains_key("result") || object.contains_key("error") {
                return Err("message contains both a method and a response payload".to_owned());
            }
            let params = object.get("params").cloned().unwrap_or(Value::Null);
            let event = if let Some(id) = id {
                validate_request_id(id).map_err(|error| error.to_string())?;
                CodexEvent::ServerRequest {
                    id: id.clone(),
                    method: method.to_owned(),
                    params,
                }
            } else {
                CodexEvent::Notification {
                    method: method.to_owned(),
                    params,
                }
            };
            return events.try_send(event).map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => {
                    format!("event queue reached its capacity of {event_buffer}")
                }
                mpsc::error::TrySendError::Closed(_) => "event receiver was dropped".to_owned(),
            });
        }

        let id = id.ok_or_else(|| "message has neither method nor id".to_owned())?;
        let key = correlation_key(id)
            .ok_or_else(|| "response id is neither a string nor an integer".to_owned())?;
        let has_result = object.contains_key("result");
        let has_error = object.contains_key("error");
        if has_result == has_error {
            return Err("response must contain exactly one of result or error".to_owned());
        }

        let response = if has_result {
            Ok(object.get("result").cloned().unwrap_or(Value::Null))
        } else {
            Err(parse_rpc_error(
                object
                    .get("error")
                    .expect("presence checked immediately above"),
            )?)
        };

        if let Some(pending) = self.state.lock().await.pending.remove(&key) {
            let _ = pending.send(response);
        }
        Ok(())
    }

    async fn fail(&self, error: CodexError, request_shutdown: bool) {
        let pending = {
            let mut state = self.state.lock().await;
            if state.failure.is_some() {
                return;
            }
            state.failure = Some(error.clone());
            std::mem::take(&mut state.pending)
        };
        for (_, sender) in pending {
            let _ = sender.send(Err(error.clone()));
        }
        if request_shutdown {
            let _ = self.shutdown.send(true);
        }
    }

    async fn current_failure(&self) -> CodexError {
        if let Some(error) = &self.state.lock().await.failure {
            return error.clone();
        }
        CodexError::Closed {
            reason: "response channel closed unexpectedly".to_owned(),
            stderr: self.stderr_context().await,
        }
    }

    async fn stderr_context(&self) -> String {
        self.stderr_tail.lock().await.display()
    }
}

fn validate_options(options: &CodexClientOptions) -> Result<(), CodexError> {
    if options.max_message_bytes == 0 {
        return Err(CodexError::InvalidOptions(
            "max_message_bytes must be greater than zero".to_owned(),
        ));
    }
    if options.event_buffer == 0 {
        return Err(CodexError::InvalidOptions(
            "event_buffer must be greater than zero".to_owned(),
        ));
    }
    if options.client_name.trim().is_empty() {
        return Err(CodexError::InvalidOptions(
            "client_name must not be empty".to_owned(),
        ));
    }
    if options.client_version.trim().is_empty() {
        return Err(CodexError::InvalidOptions(
            "client_version must not be empty".to_owned(),
        ));
    }
    Ok(())
}

fn validate_method(method: String) -> Result<String, CodexError> {
    if method.trim().is_empty() {
        return Err(CodexError::Protocol {
            message: "method must not be empty".to_owned(),
            stderr: "(not applicable)".to_owned(),
        });
    }
    Ok(method)
}

fn validate_request_id(id: &RequestId) -> Result<(), CodexError> {
    if correlation_key(id).is_none() {
        return Err(CodexError::Protocol {
            message: "request id must be a string or integer".to_owned(),
            stderr: "(not applicable)".to_owned(),
        });
    }
    Ok(())
}

fn correlation_key(id: &Value) -> Option<String> {
    match id {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) if value.is_i64() || value.is_u64() => Some(value.to_string()),
        _ => None,
    }
}

fn parse_rpc_error(value: &Value) -> Result<CodexError, String> {
    let object = value
        .as_object()
        .ok_or_else(|| "response error is not an object".to_owned())?;
    let code = object
        .get("code")
        .and_then(Value::as_i64)
        .ok_or_else(|| "response error code is not an integer".to_owned())?;
    let message = object
        .get("message")
        .and_then(Value::as_str)
        .ok_or_else(|| "response error message is not a string".to_owned())?;
    Ok(CodexError::Rpc {
        code,
        message: message.to_owned(),
        data: object.get("data").cloned(),
    })
}

async fn read_stdout<R>(
    mut reader: R,
    inner: Weak<Inner>,
    events: mpsc::Sender<CodexEvent>,
    max_message_bytes: usize,
    event_buffer: usize,
) where
    R: AsyncRead + Unpin,
{
    let mut chunk = [0_u8; 8192];
    let mut frame = Vec::new();

    loop {
        let read = match reader.read(&mut chunk).await {
            Ok(read) => read,
            Err(error) => {
                if let Some(inner) = inner.upgrade() {
                    let failure = CodexError::Io {
                        operation: "reading stdout",
                        message: error.to_string(),
                        stderr: inner.stderr_context().await,
                    };
                    inner.fail(failure, true).await;
                }
                return;
            }
        };

        if read == 0 {
            if !frame.is_empty() && !process_frame(&mut frame, &inner, &events, event_buffer).await
            {
                return;
            }
            if let Some(inner) = inner.upgrade() {
                let failure = CodexError::Closed {
                    reason: "app-server stdout reached EOF".to_owned(),
                    stderr: inner.stderr_context().await,
                };
                inner.fail(failure, true).await;
            }
            return;
        }

        for byte in &chunk[..read] {
            if *byte == b'\n' {
                if !process_frame(&mut frame, &inner, &events, event_buffer).await {
                    return;
                }
                continue;
            }
            if frame.len() == max_message_bytes {
                if let Some(inner) = inner.upgrade() {
                    inner
                        .fail(
                            CodexError::MessageTooLarge {
                                observed: frame.len() + 1,
                                limit: max_message_bytes,
                            },
                            true,
                        )
                        .await;
                }
                return;
            }
            frame.push(*byte);
        }
    }
}

async fn process_frame(
    frame: &mut Vec<u8>,
    inner: &Weak<Inner>,
    events: &mpsc::Sender<CodexEvent>,
    event_buffer: usize,
) -> bool {
    if frame.last() == Some(&b'\r') {
        frame.pop();
    }
    if frame.iter().all(u8::is_ascii_whitespace) {
        frame.clear();
        return true;
    }

    let value = match serde_json::from_slice::<Value>(frame) {
        Ok(value) => value,
        Err(error) => {
            if let Some(inner) = inner.upgrade() {
                let failure = CodexError::Protocol {
                    message: format!("invalid JSONL frame: {error}"),
                    stderr: inner.stderr_context().await,
                };
                inner.fail(failure, true).await;
            }
            frame.clear();
            return false;
        }
    };
    frame.clear();

    let Some(inner) = inner.upgrade() else {
        return false;
    };
    if let Err(message) = inner.handle_message(value, events, event_buffer).await {
        let failure = CodexError::Protocol {
            message,
            stderr: inner.stderr_context().await,
        };
        inner.fail(failure, true).await;
        return false;
    }
    true
}

async fn collect_stderr<R>(mut stderr: R, tail: Arc<Mutex<StderrTail>>)
where
    R: AsyncRead + Unpin,
{
    let mut chunk = [0_u8; 4096];
    loop {
        match stderr.read(&mut chunk).await {
            Ok(0) => return,
            Ok(read) => tail.lock().await.push(&chunk[..read]),
            Err(error) => {
                tail.lock()
                    .await
                    .push(format!("\n[stderr read failed: {error}]").as_bytes());
                return;
            }
        }
    }
}

async fn monitor_child(mut child: Child, mut shutdown: watch::Receiver<bool>, inner: Weak<Inner>) {
    loop {
        tokio::select! {
            status = child.wait() => {
                tokio::task::yield_now().await;
                if let Some(inner) = inner.upgrade() {
                    let reason = match status {
                        Ok(status) => format!("app-server exited with {status}"),
                        Err(error) => format!("could not wait for app-server: {error}"),
                    };
                    let failure = CodexError::Closed {
                        reason,
                        stderr: inner.stderr_context().await,
                    };
                    inner.fail(failure, false).await;
                }
                return;
            }
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    let _ = child.start_kill();
                    let _ = child.wait().await;
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use serde_json::{Value, json};
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, DuplexStream};
    use tokio::time::timeout;

    use super::{CodexClient, CodexError, CodexEvent, STDERR_TAIL_BYTES, StderrTail};

    async fn client_pair(
        max_message_bytes: usize,
    ) -> (
        CodexClient,
        tokio::sync::mpsc::Receiver<CodexEvent>,
        DuplexStream,
    ) {
        let (client_stream, server_stream) = tokio::io::duplex(64 * 1024);
        let (reader, writer) = tokio::io::split(client_stream);
        let stderr = Arc::new(tokio::sync::Mutex::new(StderrTail::new(STDERR_TAIL_BYTES)));
        let (client, events, _shutdown) =
            CodexClient::from_io(reader, writer, max_message_bytes, 8, stderr).await;
        (client, events, server_stream)
    }

    #[tokio::test]
    async fn decodes_a_response_split_across_arbitrary_chunks() {
        let (client, _events, server) = client_pair(4096).await;
        let (server_reader, mut server_writer) = tokio::io::split(server);
        let mut server_reader = BufReader::new(server_reader);

        let request = tokio::spawn({
            let client = client.clone();
            async move { client.request("thread/start", json!({"cwd": "/tmp"})).await }
        });

        let mut line = String::new();
        server_reader.read_line(&mut line).await.unwrap();
        let sent: Value = serde_json::from_str(&line).unwrap();
        let response = serde_json::to_vec(&json!({
            "id": sent["id"],
            "result": {"thread": {"id": "thr_1"}}
        }))
        .unwrap();
        for chunk in response.chunks(3) {
            server_writer.write_all(chunk).await.unwrap();
            tokio::task::yield_now().await;
        }
        server_writer.write_all(b"\n").await.unwrap();

        assert_eq!(
            request.await.unwrap().unwrap(),
            json!({"thread": {"id": "thr_1"}})
        );
        drop(server_writer);
        drop(server_reader);
        client.close().await.unwrap();
    }

    #[tokio::test]
    async fn correlates_out_of_order_responses() {
        let (client, _events, server) = client_pair(4096).await;
        let (server_reader, mut server_writer) = tokio::io::split(server);
        let mut server_reader = BufReader::new(server_reader);

        let first = tokio::spawn({
            let client = client.clone();
            async move { client.request("first", json!({})).await }
        });
        let second = tokio::spawn({
            let client = client.clone();
            async move { client.request("second", json!({})).await }
        });

        let mut requests = Vec::new();
        for _ in 0..2 {
            let mut line = String::new();
            server_reader.read_line(&mut line).await.unwrap();
            requests.push(serde_json::from_str::<Value>(&line).unwrap());
        }
        let first_wire = requests
            .iter()
            .find(|request| request["method"] == "first")
            .unwrap();
        let second_wire = requests
            .iter()
            .find(|request| request["method"] == "second")
            .unwrap();
        server_writer
            .write_all(
                format!(
                    "{}\n{}\n",
                    json!({"id": second_wire["id"], "result": "second result"}),
                    json!({"id": first_wire["id"], "result": "first result"}),
                )
                .as_bytes(),
            )
            .await
            .unwrap();

        assert_eq!(first.await.unwrap().unwrap(), json!("first result"));
        assert_eq!(second.await.unwrap().unwrap(), json!("second result"));
        drop(server_writer);
        drop(server_reader);
        client.close().await.unwrap();
    }

    #[tokio::test]
    async fn emits_server_requests_without_automatically_answering_them() {
        let (client, mut events, server) = client_pair(4096).await;
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        server_writer
            .write_all(
                b"{\"id\":17,\"method\":\"item/commandExecution/requestApproval\",\"params\":{\"reason\":\"network\"}}\n",
            )
            .await
            .unwrap();

        assert_eq!(
            events.recv().await,
            Some(CodexEvent::ServerRequest {
                id: json!(17),
                method: "item/commandExecution/requestApproval".to_owned(),
                params: json!({"reason": "network"}),
            })
        );

        let mut byte = [0_u8; 1];
        assert!(
            timeout(Duration::from_millis(50), server_reader.read(&mut byte))
                .await
                .is_err(),
            "a server request must remain unanswered until respond is called"
        );

        client
            .respond(json!(17), json!({"decision": "decline"}))
            .await
            .unwrap();
        let read = timeout(Duration::from_secs(1), server_reader.read(&mut byte))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(read, 1);
        drop(server_writer);
        drop(server_reader);
        client.close().await.unwrap();
    }

    #[tokio::test]
    async fn preserves_structured_rpc_errors_without_closing_the_connection() {
        let (client, _events, server) = client_pair(4096).await;
        let (server_reader, mut server_writer) = tokio::io::split(server);
        let mut server_reader = BufReader::new(server_reader);

        let request = tokio::spawn({
            let client = client.clone();
            async move { client.request("thread/start", json!({})).await }
        });
        let mut line = String::new();
        server_reader.read_line(&mut line).await.unwrap();
        let sent: Value = serde_json::from_str(&line).unwrap();
        server_writer
            .write_all(
                format!(
                    "{}\n",
                    json!({
                        "id": sent["id"],
                        "error": {"code": -32000, "message": "denied", "data": {"retry": false}}
                    })
                )
                .as_bytes(),
            )
            .await
            .unwrap();

        assert_eq!(
            request.await.unwrap().unwrap_err(),
            CodexError::Rpc {
                code: -32000,
                message: "denied".to_owned(),
                data: Some(json!({"retry": false})),
            }
        );
        drop(server_writer);
        drop(server_reader);
        client.close().await.unwrap();
    }
}
