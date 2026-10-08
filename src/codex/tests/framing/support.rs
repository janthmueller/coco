use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, BufReader, DuplexStream};
use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;

use crate::codex::{CodexClient, CodexEvent, STDERR_TAIL_BYTES, StderrTail};

pub(super) const TEST_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) fn large_payload() -> Value {
    json!({"padding": "x".repeat(128 * 1024)})
}

pub(super) async fn first_byte<R: AsyncRead + Unpin>(reader: &mut R) -> u8 {
    let mut prefix = [0_u8; 1];
    timeout(TEST_TIMEOUT, reader.read_exact(&mut prefix))
        .await
        .unwrap()
        .unwrap();
    prefix[0]
}

pub(super) async fn next_frame<R: AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
    prefix: Option<u8>,
) -> Value {
    use tokio::io::AsyncBufReadExt;

    let mut line = prefix
        .map(|byte| char::from(byte).to_string())
        .unwrap_or_default();
    assert_ne!(
        timeout(TEST_TIMEOUT, reader.read_line(&mut line))
            .await
            .unwrap()
            .unwrap(),
        0
    );
    serde_json::from_str(&line).expect("outbound frame was truncated or interleaved")
}

#[derive(Default)]
pub(super) struct WriteEvidence {
    pub(super) prefix: Vec<u8>,
    pub(super) writes_after_failure: usize,
}

struct FailingWriter {
    evidence: Arc<Mutex<WriteEvidence>>,
    started: Option<oneshot::Sender<()>>,
    fail: oneshot::Receiver<()>,
    failed: bool,
}

impl AsyncWrite for FailingWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        if self.failed {
            self.evidence.lock().unwrap().writes_after_failure += 1;
            return Poll::Ready(Err(std::io::Error::other("write after terminal failure")));
        }
        if let Some(started) = self.started.take() {
            let length = bytes.len().min(8);
            self.evidence
                .lock()
                .unwrap()
                .prefix
                .extend_from_slice(&bytes[..length]);
            let _ = started.send(());
            return Poll::Ready(Ok(length));
        }
        match Pin::new(&mut self.fail).poll(context) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(_) => {
                self.failed = true;
                Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "test partial write failed",
                )))
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        _context: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

type FailureFixture = (
    CodexClient,
    mpsc::Receiver<CodexEvent>,
    oneshot::Receiver<()>,
    oneshot::Sender<()>,
    Arc<Mutex<WriteEvidence>>,
    DuplexStream,
);

pub(super) async fn failing_client() -> FailureFixture {
    let (reader, peer) = tokio::io::duplex(1);
    let (started, observed) = oneshot::channel();
    let (fail, failure) = oneshot::channel();
    let evidence = Arc::new(Mutex::new(WriteEvidence::default()));
    let writer = FailingWriter {
        evidence: Arc::clone(&evidence),
        started: Some(started),
        fail: failure,
        failed: false,
    };
    let stderr = Arc::new(tokio::sync::Mutex::new(StderrTail::new(STDERR_TAIL_BYTES)));
    let (client, events, _) = CodexClient::from_io(reader, writer, 4096, 8, stderr).await;
    (client, events, observed, fail, evidence, peer)
}
