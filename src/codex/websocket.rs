use std::io::Write;
use std::net::{Ipv4Addr, SocketAddr};
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, DuplexStream};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::Child;
use tokio::sync::Mutex;
use tokio::time::Instant;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;
use tokio_tungstenite::tungstenite::{Message, http};
use tokio_tungstenite::{WebSocketStream, client_async};

use super::{CodexError, SharedAppServerOptions, StderrTail};

const APP_SERVER_STARTUP_TIMEOUT: Duration = Duration::from_secs(10);
const APP_SERVER_CONNECT_RETRY: Duration = Duration::from_millis(25);

pub(super) async fn prepare_shared_runtime(
    shared: &SharedAppServerOptions,
) -> Result<(), CodexError> {
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

pub(super) fn write_private_file(path: &Path, contents: &[u8]) -> std::io::Result<()> {
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

pub(super) async fn remove_runtime_file(path: &Path) {
    match tokio::fs::remove_file(path).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => {}
    }
}

pub(super) fn new_capability_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

pub(super) async fn reserve_loopback_address() -> Result<SocketAddr, CodexError> {
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

pub(super) async fn connect_app_server(
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

pub(super) fn authenticated_request(
    endpoint: &str,
    token: &str,
) -> Result<http::Request<()>, String> {
    let mut request = endpoint
        .into_client_request()
        .map_err(|error| format!("invalid App Server endpoint: {error}"))?;
    let authorization = format!("Bearer {token}")
        .parse()
        .map_err(|_| "could not encode App Server authorization header".to_owned())?;
    request.headers_mut().insert(AUTHORIZATION, authorization);
    Ok(request)
}

pub(super) async fn bridge_jsonl_websocket<S>(
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
