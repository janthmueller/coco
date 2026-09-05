use std::collections::HashMap;
use std::io::Write;
use std::net::{Ipv4Addr, SocketAddr};
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
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
use tokio::net::{TcpListener, TcpStream};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;
use tokio_tungstenite::tungstenite::{Message, http};
use tokio_tungstenite::{WebSocketStream, client_async};

use crate::protocol::AppServerEndpoint;

pub const DEFAULT_MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;
pub const DEFAULT_EVENT_BUFFER: usize = 256;

const STDERR_TAIL_BYTES: usize = 64 * 1024;
const APP_SERVER_STARTUP_TIMEOUT: Duration = Duration::from_secs(10);
const APP_SERVER_CONNECT_RETRY: Duration = Duration::from_millis(25);

pub type RequestId = Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedAppServerOptions {
    /// Private runtime file containing the active loopback WebSocket URL.
    pub endpoint_path: PathBuf,
    /// Private runtime file containing the high-entropy capability token.
    pub token_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct CodexClientOptions {
    pub codex_binary: PathBuf,
    /// When set, spawn one shared App Server on an authenticated loopback
    /// WebSocket. Other trusted local clients, such as the Codex TUI opened by
    /// `coco jump`, can subscribe to the same threads.
    pub shared_app_server: Option<SharedAppServerOptions>,
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
            shared_app_server: None,
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
    runtime_files: Mutex<Vec<PathBuf>>,
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
    /// Without `shared_app_server` the child uses its private stdio transport.
    /// With shared runtime files configured, the child listens on an
    /// authenticated loopback WebSocket and publishes its URL for trusted
    /// local clients such as `coco jump`.
    pub async fn spawn(
        options: CodexClientOptions,
    ) -> Result<(Self, mpsc::Receiver<CodexEvent>), CodexError> {
        validate_options(&options)?;

        if let Some(shared) = options.shared_app_server.clone() {
            return Self::spawn_shared(options, shared).await;
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

        if let Err(error) = initialize_client(&client, &options).await {
            let _ = client.close().await;
            return Err(error);
        }

        Ok((client, events))
    }

    async fn spawn_shared(
        options: CodexClientOptions,
        shared: SharedAppServerOptions,
    ) -> Result<(Self, mpsc::Receiver<CodexEvent>), CodexError> {
        prepare_shared_runtime(&shared).await?;
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

        let address = reserve_loopback_address().await?;
        let token = new_capability_token();
        write_private_file(&shared.token_path, token.as_bytes()).map_err(|error| {
            CodexError::Spawn {
                message: format!(
                    "could not write App Server capability token {}: {error}",
                    shared.token_path.display()
                ),
            }
        })?;
        let endpoint = format!("ws://{address}");
        let mut command = Command::new(&options.codex_binary);
        command
            .args(["app-server", "--listen"])
            .arg(&endpoint)
            .args(["--ws-auth", "capability-token", "--ws-token-file"])
            .arg(&shared.token_path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        apply_codex_home(&mut command, options.codex_home.as_ref());

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                remove_runtime_file(&shared.token_path).await;
                return Err(CodexError::Spawn {
                    message: error.to_string(),
                });
            }
        };
        let stderr = match child.stderr.take() {
            Some(stderr) => stderr,
            None => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                remove_runtime_file(&shared.token_path).await;
                return Err(CodexError::Spawn {
                    message: "spawned App Server did not expose stderr".to_owned(),
                });
            }
        };
        let stderr_tail = Arc::new(Mutex::new(StderrTail::new(STDERR_TAIL_BYTES)));
        let stderr_task = tokio::spawn(collect_stderr(stderr, Arc::clone(&stderr_tail)));

        let websocket = match connect_app_server(address, &endpoint, &token, &mut child).await {
            Ok(websocket) => websocket,
            Err(error) => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                let _ = stderr_task.await;
                remove_runtime_file(&shared.token_path).await;
                return Err(CodexError::Spawn {
                    message: format!(
                        "could not connect to App Server at {endpoint}: {error}; stderr: {}",
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
        *client.inner.runtime_files.lock().await =
            vec![shared.endpoint_path.clone(), shared.token_path.clone()];
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

        if let Err(error) = initialize_client(&client, &options).await {
            let _ = client.close().await;
            return Err(error);
        }
        let descriptor = match serde_json::to_vec(&AppServerEndpoint {
            schema_version: 1,
            url: endpoint,
        }) {
            Ok(descriptor) => descriptor,
            Err(error) => {
                let failure = CodexError::Protocol {
                    message: format!("could not encode App Server endpoint: {error}"),
                    stderr: client.inner.stderr_context().await,
                };
                let _ = client.close().await;
                return Err(failure);
            }
        };
        if let Err(error) = write_private_file(&shared.endpoint_path, &descriptor) {
            let message = format!(
                "could not publish App Server endpoint {}: {error}",
                shared.endpoint_path.display()
            );
            let _ = client.close().await;
            return Err(CodexError::Spawn { message });
        }

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
        self.inner.cleanup_runtime_files().await;
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
            runtime_files: Mutex::new(Vec::new()),
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

async fn prepare_shared_runtime(shared: &SharedAppServerOptions) -> Result<(), CodexError> {
    if shared.endpoint_path == shared.token_path {
        return Err(CodexError::InvalidOptions(
            "shared App Server endpoint and token paths must be different".to_owned(),
        ));
    }
    for (label, path) in [
        ("endpoint_path", &shared.endpoint_path),
        ("token_path", &shared.token_path),
    ] {
        if !path.is_absolute() {
            return Err(CodexError::InvalidOptions(format!(
                "shared_app_server.{label} must be absolute"
            )));
        }
        let parent = path.parent().ok_or_else(|| {
            CodexError::InvalidOptions(format!("shared_app_server.{label} has no parent directory"))
        })?;
        let parent_exists = tokio::fs::metadata(parent).await.is_ok();
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| CodexError::Spawn {
                message: format!(
                    "could not create App Server runtime directory {}: {error}",
                    parent.display()
                ),
            })?;
        #[cfg(unix)]
        if !parent_exists {
            tokio::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
                .await
                .map_err(|error| CodexError::Spawn {
                    message: format!(
                        "could not secure App Server runtime directory {}: {error}",
                        parent.display()
                    ),
                })?;
        }
        reject_unsafe_runtime_path(path)?;
        remove_runtime_file(path).await;
    }
    Ok(())
}

fn reject_unsafe_runtime_path(path: &Path) -> Result<(), CodexError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(CodexError::Spawn {
                message: format!("refusing unsafe App Server runtime path {}", path.display()),
            })
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(CodexError::Spawn {
            message: format!(
                "could not inspect App Server runtime path {}: {error}",
                path.display()
            ),
        }),
    }
}

fn write_private_file(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "path has no parent")
    })?;
    let temporary = parent.join(format!(".coco-runtime-{}.tmp", uuid::Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(&temporary)?;
    let result = (|| {
        file.write_all(contents)?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

async fn remove_runtime_file(path: &Path) {
    match tokio::fs::remove_file(path).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => {}
    }
}

fn new_capability_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

async fn reserve_loopback_address() -> Result<SocketAddr, CodexError> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(|error| CodexError::Spawn {
            message: format!("could not reserve an App Server loopback port: {error}"),
        })?;
    let address = listener.local_addr().map_err(|error| CodexError::Spawn {
        message: format!("could not inspect the reserved loopback port: {error}"),
    })?;
    drop(listener);
    Ok(address)
}

async fn connect_app_server(
    address: SocketAddr,
    endpoint: &str,
    token: &str,
    child: &mut Child,
) -> Result<WebSocketStream<TcpStream>, String> {
    let deadline = Instant::now() + APP_SERVER_STARTUP_TIMEOUT;
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("could not inspect App Server process: {error}"))?
        {
            return Err(format!(
                "App Server exited before accepting clients ({status})"
            ));
        }
        let error = match TcpStream::connect(address).await {
            Ok(stream) => match authenticated_request(endpoint, token) {
                Ok(request) => match client_async(request, stream).await {
                    Ok((websocket, _)) => return Ok(websocket),
                    Err(error) => format!("WebSocket handshake failed: {error}"),
                },
                Err(error) => return Err(error),
            },
            Err(error) => error.to_string(),
        };
        if Instant::now() >= deadline {
            return Err(error);
        }
        tokio::time::sleep(APP_SERVER_CONNECT_RETRY).await;
    }
}

fn authenticated_request(endpoint: &str, token: &str) -> Result<http::Request<()>, String> {
    let mut request = endpoint
        .into_client_request()
        .map_err(|error| format!("invalid App Server endpoint: {error}"))?;
    let authorization = format!("Bearer {token}")
        .parse()
        .map_err(|_| "could not encode App Server authorization header".to_owned())?;
    request.headers_mut().insert(AUTHORIZATION, authorization);
    Ok(request)
}

async fn bridge_jsonl_websocket<S>(
    io: DuplexStream,
    websocket: WebSocketStream<S>,
    stderr_tail: Arc<Mutex<StderrTail>>,
) where
    S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
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

    async fn cleanup_runtime_files(&self) {
        let paths = std::mem::take(&mut *self.runtime_files.lock().await);
        for path in paths {
            remove_runtime_file(&path).await;
        }
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
                    inner.cleanup_runtime_files().await;
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
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Arc;
    use std::time::Duration;

    use futures_util::{SinkExt, StreamExt};
    use serde_json::{Value, json};
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, DuplexStream};
    use tokio::time::timeout;
    use tokio_tungstenite::accept_hdr_async;
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::handshake::server::{
        Callback, ErrorResponse, Request, Response,
    };

    use super::{
        CodexClient, CodexError, CodexEvent, STDERR_TAIL_BYTES, SharedAppServerOptions, StderrTail,
        authenticated_request, bridge_jsonl_websocket, prepare_shared_runtime, write_private_file,
    };

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

    struct AssertAuthorization;

    impl Callback for AssertAuthorization {
        fn on_request(
            self,
            request: &Request,
            response: Response,
        ) -> Result<Response, ErrorResponse> {
            assert_eq!(
                request
                    .headers()
                    .get("authorization")
                    .and_then(|value| value.to_str().ok()),
                Some("Bearer test-capability")
            );
            Ok(response)
        }
    }

    #[tokio::test]
    async fn bridges_jsonl_over_an_authenticated_websocket() {
        let (client_transport, server_transport) = tokio::io::duplex(64 * 1024);
        let server = tokio::spawn(async move {
            let mut websocket = accept_hdr_async(server_transport, AssertAuthorization)
                .await
                .unwrap();
            assert_eq!(
                websocket.next().await.unwrap().unwrap(),
                Message::Text(r#"{"method":"health","params":{}}"#.into())
            );
            websocket
                .send(Message::Text(
                    r#"{"id":"coco-1","result":{"status":"ok"}}"#.into(),
                ))
                .await
                .unwrap();
        });
        let request = authenticated_request("ws://127.0.0.1:40123", "test-capability").unwrap();
        let (websocket, _) = tokio_tungstenite::client_async(request, client_transport)
            .await
            .unwrap();
        let (mut json_client, bridge_io) = tokio::io::duplex(64 * 1024);
        let stderr = Arc::new(tokio::sync::Mutex::new(StderrTail::new(STDERR_TAIL_BYTES)));
        let bridge = tokio::spawn(bridge_jsonl_websocket(bridge_io, websocket, stderr));

        json_client
            .write_all(b"{\"method\":\"health\",\"params\":{}}\n")
            .await
            .unwrap();
        let mut response = String::new();
        BufReader::new(&mut json_client)
            .read_line(&mut response)
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&response).unwrap(),
            json!({"id": "coco-1", "result": {"status": "ok"}})
        );
        server.await.unwrap();
        bridge.abort();
        let _ = bridge.await;
    }

    #[tokio::test]
    async fn rejects_overlapping_shared_runtime_files() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("shared");
        let error = prepare_shared_runtime(&SharedAppServerOptions {
            endpoint_path: path.clone(),
            token_path: path,
        })
        .await
        .unwrap_err();

        assert!(matches!(error, CodexError::InvalidOptions(_)));
    }

    #[cfg(unix)]
    #[test]
    fn writes_capability_files_with_owner_only_permissions() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("token");
        write_private_file(&path, b"secret").unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "secret");
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
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
