use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};

use serde_json::{Map, Value, json};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{Mutex, mpsc, oneshot, watch};

use super::websocket::remove_runtime_file;
use super::{
    CodexClient, CodexError, CodexEvent, ConnectionState, Inner, RequestId, StderrTail, TaskHandles,
};

impl CodexClient {
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
    #[expect(
        dead_code,
        reason = "reserved for the explicit approval-response workflow"
    )]
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

    pub(super) async fn from_io<R, W>(
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

        (Self { inner }, event_receiver, shutdown_receiver)
    }
}

impl Inner {
    pub(super) async fn write_frame(&self, value: &Value) -> Result<(), CodexError> {
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

    pub(super) async fn handle_message(
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

    pub(super) async fn fail(&self, error: CodexError, request_shutdown: bool) {
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

    pub(super) async fn current_failure(&self) -> CodexError {
        if let Some(error) = &self.state.lock().await.failure {
            return error.clone();
        }
        CodexError::Closed {
            reason: "response channel closed unexpectedly".to_owned(),
            stderr: self.stderr_context().await,
        }
    }

    pub(super) async fn stderr_context(&self) -> String {
        self.stderr_tail.lock().await.display()
    }

    pub(super) async fn cleanup_runtime_files(&self) {
        let paths = std::mem::take(&mut *self.runtime_files.lock().await);
        for path in paths {
            remove_runtime_file(&path).await;
        }
    }
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
