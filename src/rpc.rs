use std::io;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::watch;
use tokio::task::JoinSet;
use uuid::Uuid;

use crate::protocol::DaemonRequest;

const MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RpcRequest {
    pub id: String,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RpcErrorPayload {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl RpcErrorPayload {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RpcResponse {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcErrorPayload>,
}

#[async_trait]
pub trait RpcHandler: Send + Sync + 'static {
    async fn handle(&self, method: &str, params: Value) -> Result<Value, RpcErrorPayload>;
}

pub struct RpcServer {
    listener: UnixListener,
    socket_path: PathBuf,
    handler: Arc<dyn RpcHandler>,
}

impl RpcServer {
    pub async fn bind(
        socket_path: impl Into<PathBuf>,
        handler: Arc<dyn RpcHandler>,
    ) -> Result<Self, RpcTransportError> {
        let socket_path = socket_path.into();
        let parent = socket_path
            .parent()
            .ok_or_else(|| RpcTransportError::InvalidSocketPath(socket_path.clone()))?;
        let parent_exists = fs::metadata(parent).await.is_ok();
        fs::create_dir_all(parent).await?;
        if !parent_exists {
            set_mode(parent, 0o700).await?;
        }
        remove_stale_socket(&socket_path).await?;

        let listener = UnixListener::bind(&socket_path)?;
        if let Err(error) = set_mode(&socket_path, 0o600).await {
            drop(listener);
            let _ = fs::remove_file(&socket_path).await;
            return Err(error);
        }
        Ok(Self {
            listener,
            socket_path,
            handler,
        })
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) -> Result<(), RpcTransportError> {
        let mut connections = JoinSet::new();
        loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
                accepted = self.listener.accept() => {
                    let (stream, _) = accepted?;
                    let handler = Arc::clone(&self.handler);
                    connections.spawn(async move {
                        let _ = serve_connection(stream, handler).await;
                    });
                }
                Some(_) = connections.join_next(), if !connections.is_empty() => {}
            }
        }

        connections.abort_all();
        while connections.join_next().await.is_some() {}
        drop(self.listener);
        match fs::remove_file(&self.socket_path).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RpcClient {
    socket_path: PathBuf,
}

impl RpcClient {
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }

    pub async fn request<R>(&self, request: R) -> Result<R::Response, RpcClientError>
    where
        R: DaemonRequest,
    {
        self.request_with_id(Uuid::new_v4().to_string(), request)
            .await
    }

    pub async fn request_with_id<R>(
        &self,
        id: impl Into<String>,
        request: R,
    ) -> Result<R::Response, RpcClientError>
    where
        R: DaemonRequest,
    {
        let params = serde_json::to_value(request)?;
        let result = self
            .request_raw_with_id(id, R::METHOD.as_str(), params)
            .await?;
        serde_json::from_value(result).map_err(RpcClientError::from)
    }

    async fn request_raw_with_id(
        &self,
        id: impl Into<String>,
        method: impl Into<String>,
        params: Value,
    ) -> Result<Value, RpcClientError> {
        let request = RpcRequest {
            id: id.into(),
            method: method.into(),
            params,
        };
        let mut stream = UnixStream::connect(&self.socket_path)
            .await
            .map_err(|source| RpcClientError::Connect {
                path: self.socket_path.clone(),
                source,
            })?;
        let mut encoded = serde_json::to_vec(&request)?;
        encoded.push(b'\n');
        stream.write_all(&encoded).await?;

        let mut response = String::new();
        BufReader::new(stream).read_line(&mut response).await?;
        if response.is_empty() {
            return Err(RpcClientError::ClosedWithoutResponse);
        }
        if response.len() > MAX_MESSAGE_BYTES {
            return Err(RpcClientError::MessageTooLarge);
        }
        let response: RpcResponse = serde_json::from_str(&response)?;
        if response.id != request.id {
            return Err(RpcClientError::MismatchedId {
                expected: request.id,
                actual: response.id,
            });
        }
        match (response.result, response.error) {
            (Some(result), None) => Ok(result),
            (None, Some(error)) => Err(RpcClientError::Remote(error)),
            _ => Err(RpcClientError::MalformedResponse),
        }
    }
}

#[derive(Debug, Error)]
pub enum RpcTransportError {
    #[error("invalid daemon socket path: {0}")]
    InvalidSocketPath(PathBuf),
    #[error("refusing to replace non-socket path: {0}")]
    SocketPathOccupied(PathBuf),
    #[error("a CoCo daemon is already listening at {0}")]
    AlreadyRunning(PathBuf),
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Debug, Error)]
pub enum RpcClientError {
    #[error("could not connect to cocod at {path}: {source}")]
    Connect { path: PathBuf, source: io::Error },
    #[error("cocod closed the connection without a response")]
    ClosedWithoutResponse,
    #[error("daemon message exceeds the configured size limit")]
    MessageTooLarge,
    #[error("daemon response id {actual:?} does not match request id {expected:?}")]
    MismatchedId { expected: String, actual: String },
    #[error("daemon response must contain exactly one of result or error")]
    MalformedResponse,
    #[error("cocod returned {0:?}")]
    Remote(RpcErrorPayload),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

async fn serve_connection(
    stream: UnixStream,
    handler: Arc<dyn RpcHandler>,
) -> Result<(), io::Error> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    loop {
        line.clear();
        let bytes = reader.read_line(&mut line).await?;
        if bytes == 0 {
            return Ok(());
        }
        let response = if bytes > MAX_MESSAGE_BYTES {
            RpcResponse {
                id: String::new(),
                result: None,
                error: Some(RpcErrorPayload::new(
                    "MESSAGE_TOO_LARGE",
                    "daemon request is too large",
                )),
            }
        } else {
            handle_line(&line, handler.as_ref()).await
        };
        let mut encoded = serde_json::to_vec(&response).map_err(io::Error::other)?;
        encoded.push(b'\n');
        writer.write_all(&encoded).await?;
        if bytes > MAX_MESSAGE_BYTES {
            return Ok(());
        }
    }
}

async fn handle_line(line: &str, handler: &dyn RpcHandler) -> RpcResponse {
    let parsed = serde_json::from_str::<RpcRequest>(line);
    match parsed {
        Ok(request) if !request.id.is_empty() && !request.method.is_empty() => {
            match handler.handle(&request.method, request.params).await {
                Ok(result) => RpcResponse {
                    id: request.id,
                    result: Some(result),
                    error: None,
                },
                Err(error) => RpcResponse {
                    id: request.id,
                    result: None,
                    error: Some(error),
                },
            }
        }
        Ok(request) => RpcResponse {
            id: request.id,
            result: None,
            error: Some(RpcErrorPayload::new(
                "INVALID_REQUEST",
                "request id and method must not be empty",
            )),
        },
        Err(error) => RpcResponse {
            id: String::new(),
            result: None,
            error: Some(RpcErrorPayload::new("INVALID_REQUEST", error.to_string())),
        },
    }
}

async fn remove_stale_socket(path: &Path) -> Result<(), RpcTransportError> {
    let metadata = match fs::symlink_metadata(path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if !metadata.file_type().is_socket() {
        return Err(RpcTransportError::SocketPathOccupied(path.to_path_buf()));
    }

    match UnixStream::connect(path).await {
        Ok(_) => Err(RpcTransportError::AlreadyRunning(path.to_path_buf())),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound
            ) =>
        {
            fs::remove_file(path).await?;
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

async fn set_mode(path: &Path, mode: u32) -> Result<(), RpcTransportError> {
    let mut permissions = fs::metadata(path).await?.permissions();
    permissions.set_mode(mode);
    fs::set_permissions(path, permissions).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    struct Echo;

    #[async_trait]
    impl RpcHandler for Echo {
        async fn handle(&self, method: &str, params: Value) -> Result<Value, RpcErrorPayload> {
            if method == "echo" {
                Ok(params)
            } else {
                Err(RpcErrorPayload::new("METHOD_NOT_FOUND", method))
            }
        }
    }

    #[tokio::test]
    async fn exchanges_correlated_requests_over_a_private_unix_socket() {
        let temporary = tempdir().unwrap();
        let socket = temporary.path().join("cocod.sock");
        let server = RpcServer::bind(&socket, Arc::new(Echo)).await.unwrap();
        assert_eq!(
            fs::metadata(&socket).await.unwrap().permissions().mode() & 0o777,
            0o600
        );
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let server_task = tokio::spawn(server.run(shutdown_rx));

        let client = RpcClient::new(&socket);
        let value = client
            .request_raw_with_id("request-1", "echo", serde_json::json!({"ok": true}))
            .await
            .unwrap();
        assert_eq!(value, serde_json::json!({"ok": true}));

        shutdown_tx.send(true).unwrap();
        server_task.await.unwrap().unwrap();
        assert!(!socket.exists());
    }

    #[tokio::test]
    async fn refuses_to_replace_a_regular_file() {
        let temporary = tempdir().unwrap();
        let socket = temporary.path().join("cocod.sock");
        fs::write(&socket, b"not a socket").await.unwrap();

        let error = match RpcServer::bind(&socket, Arc::new(Echo)).await {
            Ok(_) => panic!("regular file must not be replaced"),
            Err(error) => error,
        };
        assert!(matches!(error, RpcTransportError::SocketPathOccupied(_)));
        assert_eq!(fs::read(&socket).await.unwrap(), b"not a socket");
    }
}
