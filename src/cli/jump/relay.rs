use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio::time::{Instant, MissedTickBehavior, interval, timeout};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::handshake::server::{
    Callback, ErrorResponse, Request, Response,
};
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;
use tokio_tungstenite::tungstenite::{Message, http};
use tokio_tungstenite::{WebSocketStream, accept_hdr_async, client_async};
use uuid::Uuid;

use crate::protocol::{
    WorkspaceAttachAdoptParams, WorkspaceAttachAdoptResult, WorkspaceAttachRenewParams,
};
use crate::rpc::RpcClient;

const ADOPTION_POLL_INTERVAL: Duration = Duration::from_millis(250);
const LEASE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
const FINAL_ADOPTION_TIMEOUT: Duration = Duration::from_secs(5);
const RELAY_COMPLETION_TIMEOUT: Duration = Duration::from_secs(7);

pub(super) struct PreparedRelay {
    endpoint_url: String,
    capability_token: String,
    task: JoinHandle<Result<()>>,
}

impl PreparedRelay {
    pub(super) async fn start(
        client: RpcClient,
        workspace_id: String,
        lease_id: String,
        upstream_endpoint: &str,
        upstream_token: &str,
    ) -> Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .context("could not bind the one-use Codex TUI relay")?;
        let address = listener
            .local_addr()
            .context("could not inspect the one-use Codex TUI relay")?;
        let upstream = connect_upstream(upstream_endpoint, upstream_token).await?;
        let capability_token = new_capability_token();
        let expected_authorization = format!("Bearer {capability_token}");
        let task = tokio::spawn(run_relay(
            listener,
            upstream,
            expected_authorization,
            client,
            workspace_id,
            lease_id,
        ));
        Ok(Self {
            endpoint_url: format!("ws://{address}"),
            capability_token,
            task,
        })
    }

    pub(super) fn endpoint_url(&self) -> &str {
        &self.endpoint_url
    }

    pub(super) fn capability_token(&self) -> &str {
        &self.capability_token
    }

    pub(super) async fn finish(self) -> Result<()> {
        let mut task = self.task;
        match timeout(RELAY_COMPLETION_TIMEOUT, &mut task).await {
            Ok(result) => result.context("the Codex TUI relay task failed")?,
            Err(_) => {
                task.abort();
                let _ = task.await;
                bail!("the Codex TUI relay did not close after the terminal UI exited")
            }
        }
    }

    pub(super) async fn abort(self) {
        self.task.abort();
        let _ = self.task.await;
    }
}

async fn connect_upstream(endpoint: &str, token: &str) -> Result<WebSocketStream<TcpStream>> {
    let address = websocket_address(endpoint)?;
    let stream = TcpStream::connect(address)
        .await
        .context("could not connect the TUI relay to cocod's App Server")?;
    let mut request = endpoint
        .into_client_request()
        .context("cocod published an invalid App Server WebSocket URL")?;
    request.headers_mut().insert(
        AUTHORIZATION,
        format!("Bearer {token}")
            .parse()
            .context("could not encode App Server authorization")?,
    );
    client_async(request, stream)
        .await
        .map(|(websocket, _)| websocket)
        .context("the App Server rejected the TUI relay")
}

fn websocket_address(endpoint: &str) -> Result<SocketAddr> {
    let address = endpoint
        .strip_prefix("ws://")
        .context("the App Server relay only supports loopback ws:// endpoints")?
        .parse::<SocketAddr>()
        .context("the App Server endpoint has an invalid socket address")?;
    ensure!(
        address.ip().is_loopback() && address.port() != 0,
        "the App Server relay only supports nonzero loopback endpoints"
    );
    Ok(address)
}

async fn run_relay(
    listener: TcpListener,
    mut upstream: WebSocketStream<TcpStream>,
    expected_authorization: String,
    client: RpcClient,
    workspace_id: String,
    lease_id: String,
) -> Result<()> {
    let (stream, peer) = accept_downstream(&listener, &client, &workspace_id, &lease_id).await?;
    ensure!(
        peer.ip().is_loopback(),
        "the TUI relay rejected a non-loopback client"
    );
    let mut downstream = accept_hdr_async(stream, RequireAuthorization(expected_authorization))
        .await
        .context("the Codex terminal UI failed to authenticate to its relay")?;
    let (mut state, transport_error) = proxy_session(
        &mut downstream,
        &mut upstream,
        &client,
        &workspace_id,
        &lease_id,
    )
    .await;
    complete_adoption(&client, &workspace_id, &lease_id, &mut state).await?;
    if let Some(error) = transport_error {
        bail!("the Codex TUI relay disconnected unexpectedly: {error}");
    }
    Ok(())
}

async fn accept_downstream(
    listener: &TcpListener,
    client: &RpcClient,
    workspace_id: &str,
    lease_id: &str,
) -> Result<(TcpStream, SocketAddr)> {
    let mut heartbeat = interval(LEASE_HEARTBEAT_INTERVAL);
    heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                return accepted.context("could not accept the Codex terminal UI");
            }
            _ = heartbeat.tick() => {
                renew_lease(client, workspace_id, lease_id).await?;
            }
        }
    }
}

async fn proxy_session(
    downstream: &mut WebSocketStream<TcpStream>,
    upstream: &mut WebSocketStream<TcpStream>,
    client: &RpcClient,
    workspace_id: &str,
    lease_id: &str,
) -> (AdoptionState, Option<String>) {
    let mut state = AdoptionState::default();
    let mut poll = interval(ADOPTION_POLL_INTERVAL);
    poll.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut heartbeat = interval(LEASE_HEARTBEAT_INTERVAL);
    heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        let outcome = tokio::select! {
            message = downstream.next() => {
                forward_downstream(message, upstream, &mut state).await
            }
            message = upstream.next() => {
                forward_upstream(message, downstream, &mut state).await
            }
            _ = poll.tick(), if state.should_poll() => {
                adopt_candidate(client, workspace_id, lease_id, &mut state)
                    .await
                    .map(|()| ForwardOutcome::Continue)
                    .map_err(|error| error.to_string())
            }
            _ = heartbeat.tick() => {
                renew_lease(client, workspace_id, lease_id)
                    .await
                    .map(|()| ForwardOutcome::Continue)
                    .map_err(|error| error.to_string())
            }
        };
        match outcome {
            Ok(ForwardOutcome::Continue) => {}
            Ok(ForwardOutcome::CandidateObserved) => {
                if let Err(error) =
                    adopt_candidate(client, workspace_id, lease_id, &mut state).await
                {
                    return (state, Some(error.to_string()));
                }
            }
            Ok(ForwardOutcome::Closed) => return (state, None),
            Err(error) => return (state, Some(error)),
        }
    }
}

async fn renew_lease(client: &RpcClient, workspace_id: &str, lease_id: &str) -> Result<()> {
    client
        .request(WorkspaceAttachRenewParams {
            workspace_id: workspace_id.to_owned(),
            lease_id: lease_id.to_owned(),
        })
        .await
        .context("cocod could not renew the temporary TUI lease")?;
    Ok(())
}

async fn forward_downstream(
    message: Option<Result<Message, tokio_tungstenite::tungstenite::Error>>,
    upstream: &mut WebSocketStream<TcpStream>,
    state: &mut AdoptionState,
) -> Result<ForwardOutcome, String> {
    let Some(message) = message else {
        return Ok(ForwardOutcome::Closed);
    };
    let message = message.map_err(|error| error.to_string())?;
    state.observe_downstream(&message);
    let closed = matches!(message, Message::Close(_));
    upstream
        .send(message)
        .await
        .map_err(|error| error.to_string())?;
    Ok(if closed {
        ForwardOutcome::Closed
    } else {
        ForwardOutcome::Continue
    })
}

async fn forward_upstream(
    message: Option<Result<Message, tokio_tungstenite::tungstenite::Error>>,
    downstream: &mut WebSocketStream<TcpStream>,
    state: &mut AdoptionState,
) -> Result<ForwardOutcome, String> {
    let Some(message) = message else {
        return Ok(ForwardOutcome::Closed);
    };
    let message = message.map_err(|error| error.to_string())?;
    let candidate_observed = state.observe_upstream(&message);
    let closed = matches!(message, Message::Close(_));
    downstream
        .send(message)
        .await
        .map_err(|error| error.to_string())?;
    Ok(if closed {
        ForwardOutcome::Closed
    } else if candidate_observed {
        ForwardOutcome::CandidateObserved
    } else {
        ForwardOutcome::Continue
    })
}

async fn complete_adoption(
    client: &RpcClient,
    workspace_id: &str,
    lease_id: &str,
    state: &mut AdoptionState,
) -> Result<()> {
    if state.candidate_thread_id.is_none() || state.bound {
        return Ok(());
    }
    adopt_candidate(client, workspace_id, lease_id, state).await?;
    if state.bound || !state.activation_requested {
        return Ok(());
    }
    let deadline = Instant::now() + FINAL_ADOPTION_TIMEOUT;
    while Instant::now() < deadline {
        tokio::time::sleep(ADOPTION_POLL_INTERVAL).await;
        adopt_candidate(client, workspace_id, lease_id, state).await?;
        if state.bound {
            return Ok(());
        }
    }
    bail!("Codex did not persist the TUI-created thread after its first action")
}

async fn adopt_candidate(
    client: &RpcClient,
    workspace_id: &str,
    lease_id: &str,
    state: &mut AdoptionState,
) -> Result<()> {
    let Some(thread_id) = state.candidate_thread_id.clone() else {
        return Ok(());
    };
    let result = client
        .request(WorkspaceAttachAdoptParams {
            workspace_id: workspace_id.to_owned(),
            lease_id: lease_id.to_owned(),
            thread_id: thread_id.clone(),
        })
        .await
        .context("cocod could not adopt the TUI-created Codex thread")?;
    if let WorkspaceAttachAdoptResult::Bound { workspace } = result {
        ensure!(
            workspace.id == workspace_id,
            "cocod adopted the wrong workspace"
        );
        ensure!(
            workspace.codex_thread_id.as_deref() == Some(thread_id.as_str()),
            "cocod adopted a different Codex thread"
        );
        state.bound = true;
    }
    Ok(())
}

#[derive(Default)]
struct AdoptionState {
    thread_start_request_ids: Vec<Value>,
    candidate_thread_id: Option<String>,
    activation_requested: bool,
    bound: bool,
}

impl AdoptionState {
    fn should_poll(&self) -> bool {
        self.candidate_thread_id.is_some() && self.activation_requested && !self.bound
    }

    fn observe_downstream(&mut self, message: &Message) {
        let Some(frame) = message_json(message) else {
            return;
        };
        let method = frame.get("method").and_then(Value::as_str);
        if method == Some("thread/start")
            && self.candidate_thread_id.is_none()
            && let Some(id) = frame.get("id")
        {
            self.thread_start_request_ids.push(id.clone());
        }
        let Some(candidate) = self.candidate_thread_id.as_deref() else {
            return;
        };
        let targets_candidate =
            frame.pointer("/params/threadId").and_then(Value::as_str) == Some(candidate);
        if targets_candidate
            && matches!(
                method,
                Some("turn/start" | "thread/shellCommand" | "review/start")
            )
        {
            self.activation_requested = true;
        }
    }

    fn observe_upstream(&mut self, message: &Message) -> bool {
        if self.candidate_thread_id.is_some() {
            return false;
        }
        let Some(frame) = message_json(message) else {
            return false;
        };
        let Some(id) = frame.get("id") else {
            return false;
        };
        let Some(position) = self
            .thread_start_request_ids
            .iter()
            .position(|request_id| request_id == id)
        else {
            return false;
        };
        self.thread_start_request_ids.swap_remove(position);
        let Some(thread_id) = frame.pointer("/result/thread/id").and_then(Value::as_str) else {
            return false;
        };
        self.candidate_thread_id = Some(thread_id.to_owned());
        true
    }
}

enum ForwardOutcome {
    Continue,
    CandidateObserved,
    Closed,
}

fn message_json(message: &Message) -> Option<Value> {
    match message {
        Message::Text(text) => serde_json::from_str(text).ok(),
        Message::Binary(bytes) => serde_json::from_slice(bytes).ok(),
        Message::Ping(_) | Message::Pong(_) | Message::Close(_) | Message::Frame(_) => None,
    }
}

fn new_capability_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

struct RequireAuthorization(String);

impl Callback for RequireAuthorization {
    fn on_request(self, request: &Request, response: Response) -> Result<Response, ErrorResponse> {
        let authorization = request
            .headers()
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok());
        if authorization == Some(self.0.as_str()) {
            return Ok(response);
        }
        Err(http::Response::builder()
            .status(http::StatusCode::UNAUTHORIZED)
            .body(Some("unauthorized".to_owned()))
            .expect("static unauthorized response must be valid"))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn correlates_only_the_exact_thread_start_response() {
        let mut state = AdoptionState::default();
        state.observe_downstream(&Message::Text(
            json!({"id": 7, "method": "thread/start", "params": {"cwd": "/repo"}})
                .to_string()
                .into(),
        ));

        assert!(
            !state.observe_upstream(&Message::Text(
                json!({"method": "thread/started", "params": {"thread": {"id": "wrong"}}})
                    .to_string()
                    .into(),
            ))
        );
        assert!(
            !state.observe_upstream(&Message::Text(
                json!({"id": 8, "result": {"thread": {"id": "wrong"}}})
                    .to_string()
                    .into(),
            ))
        );
        assert!(
            state.observe_upstream(&Message::Text(
                json!({"id": 7, "result": {"thread": {"id": "thread-exact"}}})
                    .to_string()
                    .into(),
            ))
        );
        assert_eq!(state.candidate_thread_id.as_deref(), Some("thread-exact"));
    }

    #[test]
    fn detects_only_materializing_requests_for_the_candidate() {
        let mut state = AdoptionState {
            candidate_thread_id: Some("thread-exact".to_owned()),
            ..AdoptionState::default()
        };
        assert!(
            !state.should_poll(),
            "an empty TUI must not run the fast adoption poll"
        );
        state.observe_downstream(&Message::Text(
            json!({"id": 1, "method": "thread/read", "params": {"threadId": "thread-exact"}})
                .to_string()
                .into(),
        ));
        state.observe_downstream(&Message::Text(
            json!({"id": 2, "method": "turn/start", "params": {"threadId": "thread-other"}})
                .to_string()
                .into(),
        ));
        assert!(!state.activation_requested);

        state.observe_downstream(&Message::Text(
            json!({"id": 3, "method": "turn/start", "params": {"threadId": "thread-exact"}})
                .to_string()
                .into(),
        ));
        assert!(state.activation_requested);
        assert!(state.should_poll());
    }
}
