use std::future::Future;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::{Instant, Interval, MissedTickBehavior, interval, timeout};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::handshake::server::{
    Callback, ErrorResponse, Request, Response,
};
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::{Message, http};
use tokio_tungstenite::{WebSocketStream, accept_hdr_async_with_config, client_async_with_config};
use uuid::Uuid;

use crate::protocol::{
    WorkspaceAttachAdoptParams, WorkspaceAttachAdoptResult, WorkspaceAttachRenewParams,
    WorkspaceExecutionEnvironment,
};
use crate::rpc::RpcClient;

const ADOPTION_POLL_INTERVAL: Duration = Duration::from_millis(250);
const LEASE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
const FINAL_ADOPTION_TIMEOUT: Duration = Duration::from_secs(5);
const RELAY_COMPLETION_TIMEOUT: Duration = Duration::from_secs(7);
const MAX_TRANSPORT_REASON_CHARS: usize = 512;
// Keep the session relay compatible with Codex's remote App Server client while
// retaining a finite bound for authenticated loopback traffic.
const CODEX_REMOTE_MAX_WEBSOCKET_MESSAGE_SIZE: usize = 128 << 20;

pub(super) struct PreparedRelay {
    endpoint_url: String,
    capability_token: String,
    shutdown: watch::Sender<bool>,
    task: JoinHandle<Result<()>>,
}

#[derive(Clone, Copy)]
pub(super) enum ThreadBinding {
    AwaitFresh,
    AlreadyBound,
}

impl PreparedRelay {
    pub(super) async fn start(
        client: RpcClient,
        workspace_id: String,
        lease_id: String,
        thread_binding: ThreadBinding,
        upstream_endpoint: &str,
        upstream_token: &str,
        execution_environment: Option<WorkspaceExecutionEnvironment>,
    ) -> Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .context("could not bind the Codex TUI relay")?;
        let address = listener
            .local_addr()
            .context("could not inspect the Codex TUI relay")?;
        let first_upstream = connect_upstream(upstream_endpoint, upstream_token).await?;
        let capability_token = new_capability_token();
        let expected_authorization = format!("Bearer {capability_token}");
        let (shutdown, shutdown_receiver) = watch::channel(false);
        let task = tokio::spawn(
            Relay::new(
                listener,
                first_upstream,
                RelayConfig {
                    upstream_endpoint: upstream_endpoint.to_owned(),
                    upstream_token: upstream_token.to_owned(),
                    expected_authorization,
                    client,
                    workspace_id,
                    lease_id,
                    thread_binding,
                    execution_environment,
                    shutdown: shutdown_receiver,
                },
            )
            .run(),
        );
        Ok(Self {
            endpoint_url: format!("ws://{address}"),
            capability_token,
            shutdown,
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
        let Self {
            mut task, shutdown, ..
        } = self;
        let _ = shutdown.send(true);
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
        let _ = self.shutdown.send(true);
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
    client_async_with_config(request, stream, Some(relay_websocket_config()))
        .await
        .map(|(websocket, _)| websocket)
        .context("the App Server rejected the TUI relay")
}

fn relay_websocket_config() -> WebSocketConfig {
    WebSocketConfig::default()
        .max_frame_size(Some(CODEX_REMOTE_MAX_WEBSOCKET_MESSAGE_SIZE))
        .max_message_size(Some(CODEX_REMOTE_MAX_WEBSOCKET_MESSAGE_SIZE))
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

enum PhaseOutcome<T> {
    Ready(T),
    Shutdown,
}

#[derive(Clone, Copy)]
enum RelayLeg {
    TerminalUi,
    AppServer,
}

impl RelayLeg {
    fn as_str(self) -> &'static str {
        match self {
            Self::TerminalUi => "terminal UI",
            Self::AppServer => "App Server",
        }
    }
}

struct TransportFailure {
    leg: RelayLeg,
    reason: String,
}

impl TransportFailure {
    fn new(leg: RelayLeg, reason: impl Into<String>) -> Self {
        Self {
            leg,
            reason: bounded_reason(reason.into()),
        }
    }
}

enum SessionEnd {
    ClientClosed,
    Reconnect(TransportFailure),
    Shutdown,
}

fn bounded_reason(reason: String) -> String {
    let mut bounded = reason
        .trim()
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .take(MAX_TRANSPORT_REASON_CHARS + 1)
        .collect::<String>();
    if bounded.chars().count() > MAX_TRANSPORT_REASON_CHARS {
        bounded = bounded
            .chars()
            .take(MAX_TRANSPORT_REASON_CHARS.saturating_sub(1))
            .collect();
        bounded.push('…');
    }
    let bounded = bounded.trim().to_owned();
    if bounded.is_empty() {
        "connection failed without a reason".to_owned()
    } else {
        bounded
    }
}

struct RelayConfig {
    upstream_endpoint: String,
    upstream_token: String,
    expected_authorization: String,
    client: RpcClient,
    workspace_id: String,
    lease_id: String,
    thread_binding: ThreadBinding,
    execution_environment: Option<WorkspaceExecutionEnvironment>,
    shutdown: watch::Receiver<bool>,
}

struct RelayControl {
    client: RpcClient,
    workspace_id: String,
    lease_id: String,
    heartbeat: Interval,
    shutdown: watch::Receiver<bool>,
}

struct Relay {
    listener: TcpListener,
    first_upstream: Option<WebSocketStream<TcpStream>>,
    upstream_endpoint: String,
    upstream_token: String,
    expected_authorization: String,
    execution_environment: Option<WorkspaceExecutionEnvironment>,
    control: RelayControl,
    adoption: Option<AdoptionState>,
    adoption_task: Option<JoinHandle<Result<bool>>>,
    generation: u64,
    last_disconnect: Option<TransportFailure>,
}

impl Relay {
    fn new(
        listener: TcpListener,
        first_upstream: WebSocketStream<TcpStream>,
        config: RelayConfig,
    ) -> Self {
        let mut heartbeat = interval(LEASE_HEARTBEAT_INTERVAL);
        heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);
        Self {
            listener,
            first_upstream: Some(first_upstream),
            upstream_endpoint: config.upstream_endpoint,
            upstream_token: config.upstream_token,
            expected_authorization: config.expected_authorization,
            execution_environment: config.execution_environment,
            control: RelayControl {
                client: config.client,
                workspace_id: config.workspace_id,
                lease_id: config.lease_id,
                heartbeat,
                shutdown: config.shutdown,
            },
            adoption: match config.thread_binding {
                ThreadBinding::AwaitFresh => Some(AdoptionState::default()),
                ThreadBinding::AlreadyBound => None,
            },
            adoption_task: None,
            generation: 0,
            last_disconnect: None,
        }
    }

    async fn run(mut self) -> Result<()> {
        loop {
            self.harvest_ready_adoption().await?;
            let Some(mut downstream) = self.accept_authenticated().await? else {
                return self.finish().await;
            };
            let Some(upstream) = self.next_upstream().await? else {
                return self.finish().await;
            };
            let mut upstream = match upstream {
                Ok(upstream) => upstream,
                Err(error) => {
                    self.note_upstream_connect_failure(&mut downstream, error)
                        .await;
                    continue;
                }
            };

            self.generation += 1;
            self.last_disconnect = None;
            match self
                .proxy_generation(&mut downstream, &mut upstream)
                .await?
            {
                SessionEnd::ClientClosed => {
                    self.complete_adoption().await?;
                    return Ok(());
                }
                SessionEnd::Shutdown => return self.finish().await,
                SessionEnd::Reconnect(failure) => {
                    self.prepare_reconnect(failure).await?;
                }
            }
        }
    }

    async fn accept_authenticated(&mut self) -> Result<Option<WebSocketStream<TcpStream>>> {
        loop {
            let accepted = await_relay_phase(self.listener.accept(), &mut self.control).await?;
            let PhaseOutcome::Ready(accepted) = accepted else {
                return Ok(None);
            };
            let (stream, peer) = accepted.context("could not accept the Codex terminal UI")?;
            ensure!(
                peer.ip().is_loopback(),
                "the TUI relay rejected a non-loopback client"
            );

            let handshake = accept_hdr_async_with_config(
                stream,
                RequireAuthorization(self.expected_authorization.clone()),
                Some(relay_websocket_config()),
            );
            let authenticated = await_relay_phase(handshake, &mut self.control).await?;
            let PhaseOutcome::Ready(authenticated) = authenticated else {
                return Ok(None);
            };
            match authenticated {
                Ok(downstream) => return Ok(Some(downstream)),
                Err(error) => tracing::warn!(
                    workspace_id = self.control.workspace_id,
                    peer = %peer,
                    reason = %bounded_reason(error.to_string()),
                    "rejected a Codex TUI relay connection"
                ),
            }
        }
    }

    async fn next_upstream(&mut self) -> Result<Option<Result<WebSocketStream<TcpStream>>>> {
        if let Some(upstream) = self.first_upstream.take() {
            return Ok(Some(Ok(upstream)));
        }
        let connected = await_relay_phase(
            connect_upstream(&self.upstream_endpoint, &self.upstream_token),
            &mut self.control,
        )
        .await?;
        Ok(match connected {
            PhaseOutcome::Ready(connected) => Some(connected),
            PhaseOutcome::Shutdown => None,
        })
    }

    async fn note_upstream_connect_failure(
        &mut self,
        downstream: &mut WebSocketStream<TcpStream>,
        error: anyhow::Error,
    ) {
        let failure = TransportFailure::new(RelayLeg::AppServer, error.to_string());
        tracing::warn!(
            workspace_id = self.control.workspace_id,
            leg = failure.leg.as_str(),
            reason = %failure.reason,
            "Codex TUI relay could not restore its upstream connection"
        );
        close_for_reconnect(downstream).await;
        self.last_disconnect = Some(failure);
    }

    async fn prepare_reconnect(&mut self, failure: TransportFailure) -> Result<()> {
        if let Some(adoption) = &mut self.adoption {
            adoption.finish_generation();
        }
        self.schedule_adoption();
        tracing::warn!(
            workspace_id = self.control.workspace_id,
            generation = self.generation,
            leg = failure.leg.as_str(),
            reason = %failure.reason,
            "Codex TUI relay connection lost; waiting for reconnect"
        );
        self.last_disconnect = Some(failure);
        Ok(())
    }

    async fn proxy_generation(
        &mut self,
        downstream: &mut WebSocketStream<TcpStream>,
        upstream: &mut WebSocketStream<TcpStream>,
    ) -> Result<SessionEnd> {
        let mut poll = interval(ADOPTION_POLL_INTERVAL);
        poll.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            let adoption_pending = self.adoption_task.is_some();
            let should_poll = self
                .adoption
                .as_ref()
                .is_some_and(AdoptionState::should_poll);
            let outcome = tokio::select! {
                changed = self.control.shutdown.changed() => {
                    if changed.is_err() || *self.control.shutdown.borrow() {
                        return Ok(SessionEnd::Shutdown);
                    }
                    continue;
                }
                message = downstream.next() => {
                    forward_downstream(
                        message,
                        upstream,
                        self.adoption.as_mut(),
                        self.execution_environment.as_ref(),
                    ).await?
                }
                message = upstream.next() => {
                    forward_upstream(message, downstream, self.adoption.as_mut()).await
                }
                _ = poll.tick(), if should_poll => {
                    self.schedule_adoption();
                    ForwardOutcome::Continue
                }
                _ = self.control.heartbeat.tick() => {
                    self.control
                        .renew()
                        .await
                        .map(|()| ForwardOutcome::Continue)
                        .context("could not keep the reconnecting TUI lease alive")?
                }
                result = await_adoption_task(&mut self.adoption_task), if adoption_pending => {
                    self.adoption
                        .as_mut()
                        .expect("an adoption task requires fresh binding state")
                        .bound = result?;
                    ForwardOutcome::Continue
                }
            };
            match outcome {
                ForwardOutcome::Continue => {}
                ForwardOutcome::CandidateObserved => {
                    self.schedule_adoption();
                }
                ForwardOutcome::ClientClosed => return Ok(SessionEnd::ClientClosed),
                ForwardOutcome::Reconnect(failure) => {
                    return Ok(SessionEnd::Reconnect(failure));
                }
            }
        }
    }

    async fn finish(&mut self) -> Result<()> {
        self.complete_adoption().await?;
        if let Some(failure) = self.last_disconnect.take() {
            bail!(
                "the Codex TUI relay lost its {} connection and did not reconnect: {}",
                failure.leg.as_str(),
                failure.reason
            );
        }
        Ok(())
    }

    fn schedule_adoption(&mut self) {
        let Some(adoption) = self.adoption.as_ref() else {
            return;
        };
        if self.adoption_task.is_some() || adoption.bound {
            return;
        }
        let Some(thread_id) = adoption.candidate_thread_id.clone() else {
            return;
        };
        let client = self.control.client.clone();
        let workspace_id = self.control.workspace_id.clone();
        let lease_id = self.control.lease_id.clone();
        self.adoption_task = Some(tokio::spawn(async move {
            request_adoption(&client, &workspace_id, &lease_id, &thread_id).await
        }));
    }

    async fn harvest_ready_adoption(&mut self) -> Result<()> {
        if self
            .adoption_task
            .as_ref()
            .is_some_and(JoinHandle::is_finished)
        {
            self.adoption
                .as_mut()
                .expect("an adoption task requires fresh binding state")
                .bound = await_adoption_task(&mut self.adoption_task).await?;
        }
        Ok(())
    }

    async fn complete_adoption(&mut self) -> Result<()> {
        let Some(adoption) = self.adoption.as_ref() else {
            return Ok(());
        };
        if adoption.candidate_thread_id.is_none() || adoption.bound {
            return Ok(());
        }
        let activation_requested = adoption.activation_requested;
        let deadline = Instant::now() + FINAL_ADOPTION_TIMEOUT;
        let mut poll = interval(ADOPTION_POLL_INTERVAL);
        poll.set_missed_tick_behavior(MissedTickBehavior::Delay);
        self.schedule_adoption();
        loop {
            let adoption_pending = self.adoption_task.is_some();
            tokio::select! {
                result = await_adoption_task(&mut self.adoption_task), if adoption_pending => {
                    let bound = result?;
                    self.adoption
                        .as_mut()
                        .expect("an adoption task requires fresh binding state")
                        .bound = bound;
                    if bound || !activation_requested {
                        return Ok(());
                    }
                }
                _ = poll.tick(), if !adoption_pending => self.schedule_adoption(),
                _ = self.control.heartbeat.tick() => self.control.renew().await?,
                _ = tokio::time::sleep_until(deadline) => {
                    bail!("Codex did not persist the TUI-created thread after its first action")
                }
            }
        }
    }
}

impl RelayControl {
    async fn renew(&self) -> Result<()> {
        renew_lease(&self.client, &self.workspace_id, &self.lease_id).await
    }
}

async fn await_relay_phase<F, T>(future: F, control: &mut RelayControl) -> Result<PhaseOutcome<T>>
where
    F: Future<Output = T>,
{
    if *control.shutdown.borrow() {
        return Ok(PhaseOutcome::Shutdown);
    }
    tokio::pin!(future);
    loop {
        tokio::select! {
            changed = control.shutdown.changed() => {
                if changed.is_err() || *control.shutdown.borrow() {
                    return Ok(PhaseOutcome::Shutdown);
                }
            }
            result = &mut future => return Ok(PhaseOutcome::Ready(result)),
            _ = control.heartbeat.tick() => control.renew().await?,
        }
    }
}

async fn close_for_reconnect(downstream: &mut WebSocketStream<TcpStream>) {
    use tokio_tungstenite::tungstenite::protocol::CloseFrame;
    use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

    let _ = downstream
        .send(Message::Close(Some(CloseFrame {
            code: CloseCode::Again,
            reason: "App Server connection unavailable".into(),
        })))
        .await;
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
    adoption: Option<&mut AdoptionState>,
    execution_environment: Option<&WorkspaceExecutionEnvironment>,
) -> Result<ForwardOutcome> {
    let Some(message) = message else {
        return Ok(ForwardOutcome::Reconnect(TransportFailure::new(
            RelayLeg::TerminalUi,
            "connection ended without a close frame",
        )));
    };
    let mut message = match message {
        Ok(message) => message,
        Err(error) => {
            return Ok(ForwardOutcome::Reconnect(TransportFailure::new(
                RelayLeg::TerminalUi,
                error.to_string(),
            )));
        }
    };
    inject_execution_environment(&mut message, execution_environment)
        .map_err(anyhow::Error::msg)?;
    if let Some(adoption) = adoption {
        adoption.observe_downstream(&message);
    }
    let closed = matches!(message, Message::Close(_));
    if closed {
        let _ = upstream.send(message).await;
        return Ok(ForwardOutcome::ClientClosed);
    }
    if let Err(error) = upstream.send(message).await {
        return Ok(ForwardOutcome::Reconnect(TransportFailure::new(
            RelayLeg::AppServer,
            error.to_string(),
        )));
    }
    Ok(ForwardOutcome::Continue)
}

fn inject_execution_environment(
    message: &mut Message,
    execution_environment: Option<&WorkspaceExecutionEnvironment>,
) -> Result<(), String> {
    let Some(execution_environment) = execution_environment else {
        return Ok(());
    };
    let Some(mut frame) = message_json(message) else {
        return Ok(());
    };
    if !matches!(
        frame.get("method").and_then(Value::as_str),
        Some("thread/start" | "turn/start")
    ) {
        return Ok(());
    }
    let params = frame
        .get_mut("params")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "Codex sent an execution request without object params".to_owned())?;
    params.insert(
        "environments".to_owned(),
        serde_json::json!([{
            "environmentId": execution_environment.environment_id,
            "cwd": execution_environment.cwd,
            "runtimeWorkspaceRoots": execution_environment.runtime_workspace_roots,
        }]),
    );
    let encoded = serde_json::to_string(&frame).map_err(|error| error.to_string())?;
    *message = match message {
        Message::Text(_) => Message::Text(encoded.into()),
        Message::Binary(_) => Message::Binary(encoded.into_bytes().into()),
        Message::Ping(_) | Message::Pong(_) | Message::Close(_) | Message::Frame(_) => {
            return Ok(());
        }
    };
    Ok(())
}

async fn forward_upstream(
    message: Option<Result<Message, tokio_tungstenite::tungstenite::Error>>,
    downstream: &mut WebSocketStream<TcpStream>,
    adoption: Option<&mut AdoptionState>,
) -> ForwardOutcome {
    let Some(message) = message else {
        return ForwardOutcome::Reconnect(TransportFailure::new(
            RelayLeg::AppServer,
            "connection ended without a close frame",
        ));
    };
    let message = match message {
        Ok(message) => message,
        Err(error) => {
            return ForwardOutcome::Reconnect(TransportFailure::new(
                RelayLeg::AppServer,
                error.to_string(),
            ));
        }
    };
    let candidate_observed = adoption.is_some_and(|state| state.observe_upstream(&message));
    if let Message::Close(frame) = &message {
        let reason = frame
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "connection closed without a reason".to_owned());
        let _ = downstream.send(message).await;
        return ForwardOutcome::Reconnect(TransportFailure::new(RelayLeg::AppServer, reason));
    }
    if let Err(error) = downstream.send(message).await {
        return ForwardOutcome::Reconnect(TransportFailure::new(
            RelayLeg::TerminalUi,
            error.to_string(),
        ));
    }
    if candidate_observed {
        ForwardOutcome::CandidateObserved
    } else {
        ForwardOutcome::Continue
    }
}

async fn await_adoption_task(task: &mut Option<JoinHandle<Result<bool>>>) -> Result<bool> {
    task.take()
        .expect("guarded adoption task")
        .await
        .context("the TUI thread adoption task failed")?
}

async fn request_adoption(
    client: &RpcClient,
    workspace_id: &str,
    lease_id: &str,
    thread_id: &str,
) -> Result<bool> {
    let result = client
        .request(WorkspaceAttachAdoptParams {
            workspace_id: workspace_id.to_owned(),
            lease_id: lease_id.to_owned(),
            thread_id: thread_id.to_owned(),
        })
        .await
        .context("cocod could not adopt the TUI-created Codex thread")?;
    if let WorkspaceAttachAdoptResult::Bound { workspace } = result {
        ensure!(
            workspace.id == workspace_id,
            "cocod adopted the wrong workspace"
        );
        ensure!(
            workspace.codex_thread_id.as_deref() == Some(thread_id),
            "cocod adopted a different Codex thread"
        );
        return Ok(true);
    }
    Ok(false)
}

#[derive(Default)]
struct AdoptionState {
    thread_start_request_ids: Vec<Value>,
    candidate_thread_id: Option<String>,
    activation_requested: bool,
    bound: bool,
}

impl AdoptionState {
    fn finish_generation(&mut self) {
        // JSON-RPC request identifiers are scoped to one transport
        // connection. Never correlate an unanswered thread/start from an old
        // connection with a response that reuses the same identifier after a
        // reconnect.
        self.thread_start_request_ids.clear();
    }

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
    ClientClosed,
    Reconnect(TransportFailure),
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
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use serde_json::json;
    use tempfile::tempdir;
    use tokio::io::duplex;
    use tokio_tungstenite::{accept_async, accept_async_with_config, client_async_with_config};

    use super::*;
    use crate::rpc::{RpcErrorPayload, RpcHandler, RpcServer};

    struct LeaseOnlyRpc {
        adoption_attempts: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl RpcHandler for LeaseOnlyRpc {
        async fn handle(&self, method: &str, _params: Value) -> Result<Value, RpcErrorPayload> {
            match method {
                "workspace.attach.renew" => Ok(json!({})),
                "workspace.attach.adopt" => {
                    self.adoption_attempts.fetch_add(1, Ordering::SeqCst);
                    Err(RpcErrorPayload::new(
                        "UNEXPECTED_ADOPTION",
                        "a bound relay must not adopt another thread",
                    ))
                }
                method => Err(RpcErrorPayload::new(
                    "UNEXPECTED_METHOD",
                    format!("unexpected test RPC method: {method}"),
                )),
            }
        }
    }

    #[tokio::test]
    async fn configured_transport_accepts_frames_above_tungstenites_default() {
        const FRAME_SIZE: usize = (16 << 20) + 1;
        let (client_io, server_io) = duplex(256 * 1024);
        let server = tokio::spawn(async move {
            let mut socket = accept_async_with_config(server_io, Some(relay_websocket_config()))
                .await
                .unwrap();
            let message = socket.next().await.unwrap().unwrap();
            assert_eq!(message.into_data().len(), FRAME_SIZE);
        });
        let request = "ws://localhost/".into_client_request().unwrap();
        let (mut client, _) =
            client_async_with_config(request, client_io, Some(relay_websocket_config()))
                .await
                .unwrap();

        client
            .send(Message::Binary(vec![b'x'; FRAME_SIZE].into()))
            .await
            .unwrap();
        server.await.unwrap();

        let config = relay_websocket_config();
        assert_eq!(
            config.max_frame_size,
            Some(CODEX_REMOTE_MAX_WEBSOCKET_MESSAGE_SIZE)
        );
        assert_eq!(
            config.max_message_size,
            Some(CODEX_REMOTE_MAX_WEBSOCKET_MESSAGE_SIZE)
        );
    }

    #[tokio::test]
    async fn bound_relay_never_adopts_an_auxiliary_thread_start() {
        let (upstream_endpoint, upstream) = start_bound_relay_test_server().await;
        let temporary = tempdir().unwrap();
        let socket = temporary.path().join("cocod.sock");
        let adoption_attempts = Arc::new(AtomicUsize::new(0));
        let rpc_server = RpcServer::bind(
            &socket,
            Arc::new(LeaseOnlyRpc {
                adoption_attempts: Arc::clone(&adoption_attempts),
            }),
        )
        .await
        .unwrap();
        let (rpc_shutdown, rpc_shutdown_receiver) = watch::channel(false);
        let rpc_task = tokio::spawn(rpc_server.run(rpc_shutdown_receiver));
        let relay = PreparedRelay::start(
            RpcClient::new(&socket),
            "bound-workspace".to_owned(),
            "bound-lease".to_owned(),
            ThreadBinding::AlreadyBound,
            &upstream_endpoint,
            "upstream-token",
            None,
        )
        .await
        .unwrap();
        let relay_address = websocket_address(relay.endpoint_url()).unwrap();
        let stream = TcpStream::connect(relay_address).await.unwrap();
        let mut request = relay.endpoint_url().into_client_request().unwrap();
        request.headers_mut().insert(
            AUTHORIZATION,
            format!("Bearer {}", relay.capability_token())
                .parse()
                .unwrap(),
        );
        let (mut client, _) =
            client_async_with_config(request, stream, Some(relay_websocket_config()))
                .await
                .unwrap();

        assert_eq!(
            test_request(&mut client, json!(1), "initialize", json!({})).await,
            json!({})
        );
        assert_eq!(
            test_request(
                &mut client,
                json!(2),
                "thread/resume",
                json!({"threadId": "bound-thread"}),
            )
            .await,
            json!({"thread": {"id": "bound-thread"}})
        );
        assert_eq!(
            test_request(
                &mut client,
                json!(3),
                "thread/start",
                json!({"cwd": "/worktree"}),
            )
            .await,
            json!({"thread": {"id": "auxiliary-thread"}})
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
        assert_eq!(
            test_request(
                &mut client,
                json!(4),
                "thread/read",
                json!({"threadId": "bound-thread"}),
            )
            .await,
            json!({"thread": {"id": "bound-thread"}}),
            "an auxiliary start must not trigger workspace adoption or stop the relay"
        );
        client.close(None).await.unwrap();

        relay.finish().await.unwrap();
        upstream.await.unwrap();
        assert_eq!(
            adoption_attempts.load(Ordering::SeqCst),
            0,
            "a bound relay attempted to adopt an auxiliary Codex thread"
        );
        rpc_shutdown.send(true).unwrap();
        rpc_task.await.unwrap().unwrap();
    }

    async fn start_bound_relay_test_server() -> (String, JoinHandle<()>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let endpoint = format!("ws://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            while let Some(Ok(message)) = socket.next().await {
                if matches!(message, Message::Close(_)) {
                    return;
                }
                let Some(frame) = message_json(&message) else {
                    continue;
                };
                let Some(id) = frame.get("id").cloned() else {
                    continue;
                };
                let result = match frame.get("method").and_then(Value::as_str) {
                    Some("initialize" | "thread/unsubscribe") => json!({}),
                    Some("thread/resume") => json!({"thread": {"id": "bound-thread"}}),
                    Some("thread/start") => json!({"thread": {"id": "auxiliary-thread"}}),
                    Some("thread/read") => json!({"thread": {"id": "bound-thread"}}),
                    method => panic!("unexpected bound-relay request {method:?}"),
                };
                socket
                    .send(Message::Text(
                        json!({"id": id, "result": result}).to_string().into(),
                    ))
                    .await
                    .unwrap();
            }
        });
        (endpoint, task)
    }

    async fn test_request(
        socket: &mut WebSocketStream<TcpStream>,
        id: Value,
        method: &str,
        params: Value,
    ) -> Value {
        socket
            .send(Message::Text(
                json!({"id": id, "method": method, "params": params})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        loop {
            let message = socket.next().await.unwrap().unwrap();
            let frame = message_json(&message).expect("test relay returned a non-JSON response");
            if frame.get("id") == Some(&id) {
                assert!(frame.get("error").is_none(), "request failed: {frame}");
                return frame.get("result").cloned().unwrap();
            }
        }
    }

    #[test]
    fn transport_reasons_are_terminal_safe_and_bounded() {
        let unsafe_reason = format!("  failed\n\u{1b}[31m{}  ", "x".repeat(600));
        let reason = bounded_reason(unsafe_reason);

        assert!(!reason.contains('\n'));
        assert!(!reason.contains('\u{1b}'));
        assert_eq!(reason.chars().count(), MAX_TRANSPORT_REASON_CHARS);
        assert!(reason.ends_with('…'));
        assert_eq!(
            bounded_reason("\n\t".to_owned()),
            "connection failed without a reason"
        );
    }

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
    fn reconnect_drops_only_generation_scoped_request_ids() {
        let mut pending = AdoptionState::default();
        pending.observe_downstream(&Message::Text(
            json!({"id": 2, "method": "thread/start", "params": {}})
                .to_string()
                .into(),
        ));
        pending.finish_generation();
        assert!(
            !pending.observe_upstream(&Message::Text(
                json!({"id": 2, "result": {"thread": {"id": "wrong"}}})
                    .to_string()
                    .into(),
            )),
            "a response on a new connection reused a stale request id"
        );

        let mut adopted = AdoptionState {
            candidate_thread_id: Some("thread-exact".to_owned()),
            activation_requested: true,
            ..AdoptionState::default()
        };
        adopted.finish_generation();
        assert_eq!(adopted.candidate_thread_id.as_deref(), Some("thread-exact"));
        assert!(adopted.should_poll());
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

    #[test]
    fn injects_the_workspace_environment_into_thread_and_turn_start() {
        let environment = test_environment();
        for method in ["thread/start", "turn/start"] {
            let mut message = Message::Text(
                json!({
                    "id": 7,
                    "method": method,
                    "params": {
                        "cwd": "/wrong",
                        "environments": [{"environmentId": "wrong", "cwd": "/wrong"}],
                    },
                })
                .to_string()
                .into(),
            );
            inject_execution_environment(&mut message, Some(&environment)).unwrap();
            let frame = message_json(&message).unwrap();
            assert_eq!(
                frame.pointer("/params/environments"),
                Some(&json!([{
                    "environmentId": "coco-environment",
                    "cwd": "/worktree",
                    "runtimeWorkspaceRoots": ["/worktree"],
                }]))
            );
        }
    }

    #[test]
    fn leaves_unrelated_frames_and_shared_execution_untouched() {
        let original = Message::Text(
            json!({"id": 7, "method": "thread/resume", "params": {"threadId": "t"}})
                .to_string()
                .into(),
        );
        let mut unrelated = original.clone();
        inject_execution_environment(&mut unrelated, Some(&test_environment())).unwrap();
        assert_eq!(unrelated, original);

        let mut shared = Message::Text(
            json!({"id": 8, "method": "turn/start", "params": {"threadId": "t"}})
                .to_string()
                .into(),
        );
        let shared_original = shared.clone();
        inject_execution_environment(&mut shared, None).unwrap();
        assert_eq!(shared, shared_original);
    }

    fn test_environment() -> WorkspaceExecutionEnvironment {
        WorkspaceExecutionEnvironment {
            environment_id: "coco-environment".to_owned(),
            cwd: "/worktree".into(),
            runtime_workspace_roots: vec!["/worktree".into()],
        }
    }
}
