use super::*;
use crate::cli::jump::relay::{
    AdoptionState, ForwardOutcome, connect_upstream, forward_downstream,
};
use crate::protocol::WorkspaceExecutionEnvironment;
use crate::rpc::{RpcClient, RpcErrorPayload, RpcHandler, RpcServer};
use async_trait::async_trait;
use futures_util::StreamExt;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tempfile::TempDir;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Notify, watch};
use tokio::task::JoinHandle;
use tokio::time::{interval, timeout};
use tokio_tungstenite::{WebSocketStream, accept_async};

struct DisarmRpc {
    calls: Arc<AtomicUsize>,
    fail: bool,
    gate: Option<(Arc<Notify>, Arc<Notify>)>,
}

#[async_trait]
impl RpcHandler for DisarmRpc {
    async fn handle(&self, method: &str, params: Value) -> Result<Value, RpcErrorPayload> {
        assert_eq!(method, "workspace.attach.disarmCleanup");
        assert_eq!(
            params,
            json!({"workspaceId": "workspace", "leaseId": "lease"})
        );
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some((entered, release)) = &self.gate {
            entered.notify_one();
            release.notified().await;
        }
        if self.fail {
            Err(RpcErrorPayload::new(
                "DISARM_FAILED",
                "persistence not confirmed",
            ))
        } else {
            Ok(json!({}))
        }
    }
}

struct Transport {
    _temp: TempDir,
    upstream: WebSocketStream<TcpStream>,
    native: WebSocketStream<TcpStream>,
    control: RelayControl,
    calls: Arc<AtomicUsize>,
    shutdown: watch::Sender<bool>,
    server: JoinHandle<Result<(), crate::rpc::RpcTransportError>>,
}

impl Transport {
    async fn new(fail: bool) -> Self {
        Self::with_gate(fail, None).await
    }

    async fn with_gate(fail: bool, gate: Option<(Arc<Notify>, Arc<Notify>)>) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("rpc.sock");
        let calls = Arc::new(AtomicUsize::new(0));
        let rpc = RpcServer::bind(
            &path,
            Arc::new(DisarmRpc {
                calls: calls.clone(),
                fail,
                gate,
            }),
        )
        .await
        .unwrap();
        let (shutdown, receiver) = watch::channel(false);
        let server = tokio::spawn(rpc.run(receiver));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let accept = tokio::spawn(async move {
            accept_async(listener.accept().await.unwrap().0)
                .await
                .unwrap()
        });
        let upstream = connect_upstream(&format!("ws://{address}"), "fixture-token")
            .await
            .unwrap();
        let native = accept.await.unwrap();
        let (_, receiver) = watch::channel(false);
        Self {
            _temp: temp,
            upstream,
            native,
            calls,
            shutdown,
            server,
            control: RelayControl {
                client: RpcClient::new(&path),
                workspace_id: "workspace".into(),
                lease_id: "lease".into(),
                heartbeat: interval(Duration::from_secs(10)),
                shutdown: receiver,
            },
        }
    }

    async fn received(&mut self) -> Value {
        let frame = timeout(Duration::from_secs(2), self.native.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        message_json(&frame).unwrap()
    }

    async fn stop(self) {
        self.shutdown.send(true).unwrap();
        self.server.await.unwrap().unwrap();
    }
}

#[tokio::test]
async fn native_forwarding_waits_for_the_disarm_acknowledgement() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let mut transport = Transport::with_gate(false, Some((entered.clone(), release.clone()))).await;
    let mut guard = guard(Some("original"), true);
    let request = Message::Text(
        json!({"id": 1, "method": "thread/start", "params": {}})
            .to_string()
            .into(),
    );
    let mut forward = Box::pin(forward_downstream(
        Some(Ok(request)),
        &mut transport.upstream,
        None,
        None,
        &mut guard,
        &transport.control,
    ));
    tokio::select! {
        _ = entered.notified() => {},
        _ = &mut forward => panic!("native request escaped before the disarm reply"),
        _ = tokio::time::sleep(Duration::from_secs(2)) => panic!("disarm was not requested"),
    }
    assert!(
        timeout(Duration::from_millis(50), transport.native.next())
            .await
            .is_err()
    );
    release.notify_one();
    assert!(matches!(
        forward.as_mut().await.unwrap(),
        ForwardOutcome::Continue
    ));
    drop(forward);
    assert_eq!(transport.received().await["method"], "thread/start");
    assert!(guard.disarmed());
    transport.stop().await;
}

#[tokio::test]
async fn forward_disarms_before_native_use_in_shared_and_workspace_execution() {
    for (fresh, workspace_execution) in [(false, false), (false, true), (true, false), (true, true)]
    {
        let mut transport = Transport::new(false).await;
        let mut guard = guard(if fresh { None } else { Some("original") }, true);
        let mut adoption = fresh.then(AdoptionState::default);
        let environment = workspace_execution.then(|| WorkspaceExecutionEnvironment {
            environment_id: "workspace-environment".into(),
            cwd: "/worktree".into(),
            runtime_workspace_roots: vec!["/worktree".into()],
        });
        if fresh {
            let first = Message::Text(
                json!({"id": 1, "method": "thread/start", "params": {}})
                    .to_string()
                    .into(),
            );
            forward_downstream(
                Some(Ok(first)),
                &mut transport.upstream,
                adoption.as_mut(),
                environment.as_ref(),
                &mut guard,
                &transport.control,
            )
            .await
            .unwrap();
            assert_eq!(transport.received().await["method"], "thread/start");
            assert_eq!(transport.calls.load(Ordering::SeqCst), 0);
            adoption.as_mut().unwrap().candidate_thread_id = Some("original".into());
        }
        for id in [2, 3] {
            let additional = Message::Binary(
                serde_json::to_vec(&json!({"id": id, "method": "thread/start", "params": {}}))
                    .unwrap()
                    .into(),
            );
            assert!(matches!(
                forward_downstream(
                    Some(Ok(additional)),
                    &mut transport.upstream,
                    adoption.as_mut(),
                    environment.as_ref(),
                    &mut guard,
                    &transport.control
                )
                .await
                .unwrap(),
                ForwardOutcome::Continue
            ));
            // The native request cannot arrive until the successful RPC reply
            // has been observed; further traffic reuses the durable opt-out.
            assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
            let forwarded = transport.received().await;
            assert_eq!(forwarded["id"], id);
            assert_eq!(
                forwarded["params"].get("environments").is_some(),
                workspace_execution
            );
        }
        assert!(guard.disarmed());
        transport.stop().await;
    }
}

#[tokio::test]
async fn unconfirmed_disarm_never_forwards_the_additional_request() {
    let mut transport = Transport::new(true).await;
    let mut guard = guard(Some("original"), true);
    let request = Message::Text(
        json!({"id": 1, "method": "turn/start", "params": {"threadId": "other"}})
            .to_string()
            .into(),
    );
    let outcome = forward_downstream(
        Some(Ok(request)),
        &mut transport.upstream,
        None,
        None,
        &mut guard,
        &transport.control,
    )
    .await;
    assert!(outcome.is_err());
    assert!(!guard.disarmed());
    assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
    assert!(
        timeout(Duration::from_millis(50), transport.native.next())
            .await
            .is_err()
    );
    transport.stop().await;
}

#[tokio::test]
async fn persistent_relays_forward_without_cleanup_rpc() {
    let mut transport = Transport::new(true).await;
    let mut guard = guard(Some("original"), false);
    let request = Message::Text(
        json!({"id": 1, "method": "thread/start", "params": {}})
            .to_string()
            .into(),
    );
    forward_downstream(
        Some(Ok(request)),
        &mut transport.upstream,
        None,
        None,
        &mut guard,
        &transport.control,
    )
    .await
    .unwrap();
    assert_eq!(transport.received().await["method"], "thread/start");
    assert_eq!(transport.calls.load(Ordering::SeqCst), 0);
    assert!(!guard.disarmed());
    transport.stop().await;
}

#[tokio::test]
async fn passive_foreign_read_forwards_without_disarm_but_switching_still_requires_it() {
    let mut transport = Transport::new(true).await;
    let mut guard = guard(Some("original"), true);
    for method in ["thread/read", "thread/turns/list", "thread/items/list"] {
        let request =
            json!({"id": method, "method": method, "params": {"threadId": "context-source"}});
        let outcome = forward_downstream(
            Some(Ok(Message::Text(request.to_string().into()))),
            &mut transport.upstream,
            None,
            None,
            &mut guard,
            &transport.control,
        )
        .await
        .unwrap();
        assert!(matches!(outcome, ForwardOutcome::Continue));
        assert_eq!(transport.received().await, request);
    }
    assert_eq!(transport.calls.load(Ordering::SeqCst), 0);
    assert!(!guard.disarmed());
    let resume =
        json!({"id": 2, "method": "thread/resume", "params": {"threadId": "context-source"}});
    let result = forward_downstream(
        Some(Ok(Message::Text(resume.to_string().into()))),
        &mut transport.upstream,
        None,
        None,
        &mut guard,
        &transport.control,
    )
    .await;
    assert!(result.is_err());
    assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
    assert!(
        timeout(Duration::from_millis(50), transport.native.next())
            .await
            .is_err()
    );
    transport.stop().await;
}
