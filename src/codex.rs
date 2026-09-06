use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use serde_json::Value;
use thiserror::Error;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::sync::{Mutex, oneshot, watch};
use tokio::task::JoinHandle;

pub const DEFAULT_MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;
pub const DEFAULT_EVENT_BUFFER: usize = 256;

const STDERR_TAIL_BYTES: usize = 64 * 1024;

mod jsonl;
mod process;
mod websocket;

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
}

impl std::fmt::Debug for CodexClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CodexClient")
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
}

#[cfg(test)]
mod tests;
