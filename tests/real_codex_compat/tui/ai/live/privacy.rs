//! Fixed live-proof errors only. Never retain raw setup, protocol or log errors.

use std::error::Error;
use std::fmt;
use std::future::Future;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LiveFailure {
    Proof,
    Terminal,
    Coordinator,
}

impl fmt::Display for LiveFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Proof => "authenticated live proof failed; private diagnostics withheld",
            Self::Terminal => "live TUI cleanup failed; private terminal output withheld",
            Self::Coordinator => "live coordinator cleanup failed; private logs withheld",
        })
    }
}

impl Error for LiveFailure {}

/// Single output boundary for the complete authenticated proof, including setup.
pub(super) async fn proof<F, T>(future: F) -> anyhow::Result<T>
where
    F: Future<Output = anyhow::Result<T>>,
{
    future.await.map_err(|error| {
        error
            .downcast::<LiveFailure>()
            .unwrap_or(LiveFailure::Proof)
            .into()
    })
}

/// Await the original cleanup before dropping its potentially private error.
/// No formatting, source chaining, or logging of that error is permitted.
pub(super) async fn cleanup<F, T, E>(future: F, failure: LiveFailure) -> Result<T, LiveFailure>
where
    F: Future<Output = Result<T, E>>,
{
    future.await.map_err(|_| failure)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::pin::pin;
    use std::task::{Context, Poll, Waker};

    #[derive(Debug)]
    struct PrivateDiagnostic(&'static str);

    impl fmt::Display for PrivateDiagnostic {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str(self.0)
        }
    }

    impl Error for PrivateDiagnostic {}

    fn assert_private_error_is_dropped(failure: LiveFailure, private: &'static str) {
        let future = std::future::ready(Err::<(), _>(PrivateDiagnostic(private)));
        let mut future = pin!(cleanup(future, failure));
        let mut context = Context::from_waker(Waker::noop());
        let Poll::Ready(Err(error)) = future.as_mut().poll(&mut context) else {
            panic!("cleanup error was not returned");
        };
        assert_eq!(error, failure);
        assert!(error.source().is_none());
        assert!(!format!("{error} {error:?} {error:#?}").contains(private));
    }

    #[test]
    fn terminal_cleanup_drops_private_screen_in_display_debug_and_sources() {
        assert_private_error_is_dropped(
            LiveFailure::Terminal,
            "synthetic-private-terminal-transcript",
        );
    }

    #[test]
    fn coordinator_cleanup_drops_private_logs_in_display_debug_and_sources() {
        assert_private_error_is_dropped(
            LiveFailure::Coordinator,
            "synthetic-private-coordinator-log",
        );
    }

    #[test]
    fn cleanup_still_waits_for_completion_and_preserves_success() {
        let polls = Cell::new(0);
        let future = std::future::poll_fn(|_| {
            polls.set(polls.get() + 1);
            if polls.get() == 1 {
                Poll::Pending
            } else {
                Poll::Ready(Ok::<_, PrivateDiagnostic>(42))
            }
        });
        let mut future = pin!(cleanup(future, LiveFailure::Coordinator));
        let mut context = Context::from_waker(Waker::noop());
        assert!(future.as_mut().poll(&mut context).is_pending());
        assert_eq!(future.as_mut().poll(&mut context), Poll::Ready(Ok(42)));
        assert_eq!(polls.get(), 2);
        assert_eq!(finish_ready(proof(std::future::ready(Ok(42)))).unwrap(), 42);
    }

    fn finish_ready<F: Future>(future: F) -> F::Output {
        let mut future = pin!(future);
        match future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("fixture future unexpectedly suspended"),
        }
    }

    fn assert_safe_proof_error(error: &anyhow::Error, expected: LiveFailure, private: &str) {
        assert_eq!(error.downcast_ref::<LiveFailure>(), Some(&expected));
        assert_eq!(error.chain().count(), 1);
        assert!(!format!("{error} {error:?} {error:#} {error:#?}").contains(private));
    }

    #[test]
    fn setup_failures_cannot_escape_the_complete_proof_boundary() {
        const PRIVATE: &str = "synthetic-private-authentication-setup";
        let raw = anyhow::anyhow!(PRIVATE).context("synthetic setup context");
        let error = finish_ready(proof(std::future::ready(Err::<(), _>(raw)))).unwrap_err();
        assert_safe_proof_error(&error, LiveFailure::Proof, PRIVATE);
    }

    #[test]
    fn safe_cleanup_categories_survive_without_private_wrapper_contexts() {
        const PRIVATE: &str = "synthetic-private-wrapper";
        for stage in [LiveFailure::Terminal, LiveFailure::Coordinator] {
            let raw = anyhow::Error::new(stage).context(PRIVATE);
            let error = finish_ready(proof(std::future::ready(Err::<(), _>(raw)))).unwrap_err();
            assert_safe_proof_error(&error, stage, PRIVATE);
        }
    }

    #[tokio::test]
    async fn native_verification_error_is_hidden_after_successful_cleanup() -> anyhow::Result<()> {
        use futures_util::{SinkExt, StreamExt};
        use serde_json::{Value, json};
        use tokio_tungstenite::tungstenite::Message;

        const PRIVATE: &str = "synthetic-private-native-error-response";
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
            let address = listener.local_addr()?;
            let peer = tokio::spawn(async move {
                let (stream, _) = listener.accept().await?;
                let mut socket = tokio_tungstenite::accept_async(stream).await?;
                let message = socket
                    .next()
                    .await
                    .ok_or_else(|| anyhow::anyhow!("fixture peer closed"))??;
                let request: Value = serde_json::from_str(message.to_text()?)?;
                anyhow::ensure!(request["method"] == "thread/read", "wrong fixture request");
                socket
                    .send(Message::Text(
                        json!({"id":request["id"],"error":{
                            "code":-32603,"message":PRIVATE,"data":{"detail":PRIVATE}
                        }})
                        .to_string()
                        .into(),
                    ))
                    .await?;
                Ok::<_, anyhow::Error>(())
            });
            let stream = tokio::net::TcpStream::connect(address).await?;
            let (websocket, _) =
                tokio_tungstenite::client_async(format!("ws://{address}/"), stream).await?;
            let mut native = crate::RemoteAppServer {
                websocket,
                pending_frames: std::collections::VecDeque::new(),
                next_id: 1,
            };
            let cleanup_completed = Cell::new(false);
            let error = proof(async {
                let response = native
                    .request(
                        "thread/read",
                        json!({"threadId":"synthetic","includeTurns":false}),
                    )
                    .await;
                // Establish the actual shared helper's raw-error path with dummy data.
                assert!(response.as_ref().unwrap_err().to_string().contains(PRIVATE));
                cleanup(
                    std::future::ready(Ok::<_, anyhow::Error>(())),
                    LiveFailure::Coordinator,
                )
                .await?;
                cleanup_completed.set(true);
                response
            })
            .await
            .unwrap_err();
            peer.await??;
            assert!(cleanup_completed.get());
            assert_safe_proof_error(&error, LiveFailure::Proof, PRIVATE);
            Ok::<_, anyhow::Error>(())
        })
        .await??;
        Ok(())
    }
}
