use std::path::PathBuf;
use std::sync::{Arc, Weak};

use serde_json::json;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, mpsc, watch};

use crate::protocol::AppServerEndpoint;

use super::websocket::{
    bridge_jsonl_websocket, connect_app_server, new_capability_token, prepare_shared_runtime,
    remove_runtime_file, reserve_loopback_address, write_private_file,
};
use super::{
    CodexClient, CodexClientOptions, CodexError, CodexEvent, Inner, STDERR_TAIL_BYTES,
    SharedAppServerOptions, StderrTail,
};

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
}

fn apply_codex_home(command: &mut Command, codex_home: Option<&PathBuf>) {
    if let Some(codex_home) = codex_home {
        command.env("CODEX_HOME", codex_home);
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
