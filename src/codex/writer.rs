use std::sync::Weak;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::timeout;

use super::{CodexError, Inner};

pub(super) const OUTBOUND_FRAME_BUFFER: usize = 8;
pub(super) const FRAME_WRITE_TIMEOUT: Duration = Duration::from_secs(15);
const FRAME_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

pub(super) struct OutboundFrame {
    bytes: Vec<u8>,
    completion: oneshot::Sender<Result<(), CodexError>>,
}

impl Inner {
    pub(super) async fn write_frame(&self, value: &Value) -> Result<(), CodexError> {
        let mut bytes = serde_json::to_vec(value).map_err(|error| CodexError::Protocol {
            message: format!("could not encode outbound message: {error}"),
            stderr: "(not applicable)".to_owned(),
        })?;
        if bytes.len() > self.max_message_bytes {
            return Err(CodexError::MessageTooLarge {
                observed: bytes.len(),
                limit: self.max_message_bytes,
            });
        }
        bytes.push(b'\n');

        if let Some(error) = &self.state.lock().await.failure {
            return Err(error.clone());
        }
        let (completion, written) = oneshot::channel();
        if self
            .outbound
            .send(OutboundFrame { bytes, completion })
            .await
            .is_err()
        {
            return Err(self.current_failure().await);
        }
        match written.await {
            Ok(result) => result,
            Err(_) => Err(self.current_failure().await),
        }
    }
}

pub(super) async fn run_writer<W>(
    mut writer: W,
    mut frames: mpsc::Receiver<OutboundFrame>,
    inner: Weak<Inner>,
    mut shutdown: watch::Receiver<bool>,
) where
    W: AsyncWrite + Unpin,
{
    loop {
        let frame = tokio::select! {
            biased;
            _ = shutdown_requested(&mut shutdown) => break,
            frame = frames.recv() => match frame {
                Some(frame) => frame,
                None => break,
            },
        };
        if frame.completion.is_closed() {
            continue;
        }
        let Some(inner) = inner.upgrade() else {
            break;
        };
        if let Some(error) = &inner.state.lock().await.failure {
            let _ = frame.completion.send(Err(error.clone()));
            break;
        }

        // This task owns the write, not the RPC caller. Once bytes can be
        // written, cancellation must finish the frame or terminate transport.
        let result = tokio::select! {
            biased;
            _ = shutdown_requested(&mut shutdown) => {
                let _ = frame.completion.send(Err(inner.current_failure().await));
                break;
            },
            result = timeout(FRAME_WRITE_TIMEOUT, writer.write_all(&frame.bytes)) => {
                match result {
                    Ok(Ok(())) => Ok(()),
                    Ok(Err(error)) => Err(write_error(&inner, error.to_string()).await),
                    Err(_) => Err(write_error(
                        &inner,
                        "outbound frame write timed out after 15 seconds".to_owned(),
                    ).await),
                }
            },
        };
        if let Err(error) = result {
            inner.fail(error, true).await;
            let _ = frame.completion.send(Err(inner.current_failure().await));
            break;
        }
        let _ = frame.completion.send(Ok(()));
    }
    // A generic split write half can share its underlying stream with the
    // reader. Dropping it alone does not necessarily publish EOF to the bridge.
    let _ = timeout(FRAME_SHUTDOWN_TIMEOUT, writer.shutdown()).await;
}

async fn write_error(inner: &Inner, message: String) -> CodexError {
    CodexError::Io {
        operation: "writing stdin",
        message,
        stderr: inner.stderr_context().await,
    }
}

async fn shutdown_requested(shutdown: &mut watch::Receiver<bool>) {
    let _ = shutdown.wait_for(|closing| *closing).await;
}
