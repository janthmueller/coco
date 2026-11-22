use super::*;
use crate::mcp::tests::RecordingDaemon;

fn meta(thread: &str) -> RequestMetaObject {
    json!({STATE_META: {"thread":{"id":thread, "model":{"display_name":"Native Model", "reasoning_level":"max"},
        "usage":{"context":{"used_percentage":25}}}}}).as_object().unwrap().clone().into()
}

fn response(name: &str) -> Value {
    json!({"workspace":{"workspaceId":"workspace", "name":name, "resources":null}})
}

fn body(result: ReadResourceResult) -> String {
    let value = serde_json::to_value(result).unwrap();
    assert_eq!(value["contents"].as_array().unwrap().len(), 1);
    assert_eq!(value["contents"][0]["uri"], URI);
    assert_eq!(value["contents"][0]["mimeType"], "application/json");
    value["contents"][0]["text"].as_str().unwrap().into()
}

#[test]
fn discovery_reuses_a_read_only_tool_without_any_mutation_grant() {
    let server = McpServer::new(
        "/fixed/repo".into(),
        ServePermissions::default(),
        "/unused.sock".into(),
        vec![],
    );
    let tools = server.tool_router.list_all();
    let status = tools
        .iter()
        .find(|tool| tool.name == WORKSPACES_STATUS)
        .unwrap();
    let status = serde_json::to_value(status).unwrap();
    assert_eq!(
        status["_meta"]["openai/ui"]["entrypoints"][0]["resourceUri"],
        URI
    );
    assert_eq!(status["annotations"]["readOnlyHint"], true);
    assert!(!tools.iter().any(|tool| tool.name == WORKSPACES_SEND));
    assert!(server.get_info().capabilities.resources.is_some());
    assert_eq!(resources().resources[0].uri, URI);
}

#[tokio::test]
async fn missing_or_invalid_metadata_never_contacts_the_daemon_or_accepts_an_arbitrary_uri() {
    let rpc = RecordingDaemon::default();
    let dispatcher = Dispatcher::new(rpc.clone(), "/fixed/repo".into());
    let provider = Provider::default();
    for value in [
        json!({}),
        json!({STATE_META:{"thread":{"id":""}}}),
        json!({STATE_META:{"thread":{"id":7}}}),
        json!({STATE_META:{"thread":{"id":"x".repeat(257)}}}),
    ] {
        let meta: RequestMetaObject = value.as_object().unwrap().clone().into();
        assert!(
            body(provider.read(&dispatcher, URI, &meta).await.unwrap())
                .contains("CoCo unavailable")
        );
    }
    assert!(
        provider
            .read(&dispatcher, "ui://coco/../../other", &meta("thread"))
            .await
            .is_err()
    );
    assert!(rpc.calls().is_empty());
}

#[tokio::test]
async fn reads_pin_host_scope_cache_exact_threads_and_never_audit_or_reuse_a_foreign_result() {
    let rpc = RecordingDaemon::default();
    rpc.respond("workspace.statusline", response("feat/one"));
    let dispatcher = Dispatcher::new(rpc.clone(), "/fixed/repo".into());
    let provider = Provider::default();
    let mut first = meta("thread-one");
    first.get_mut(STATE_META).unwrap()["repositoryId"] = json!("foreign");
    first.get_mut(STATE_META).unwrap()["workspace"] = json!("foreign");
    let rendered = body(provider.read(&dispatcher, URI, &first).await.unwrap());
    assert!(
        rendered.contains("feat/one")
            && rendered.contains("Native Model max")
            && rendered.contains("75% left")
    );
    assert_eq!(
        rpc.calls(),
        [
            ("repository.resolve".into(), json!({"path":"/fixed/repo"})),
            (
                "workspace.statusline".into(),
                json!({"repositoryId":"repo-test", "threadId":"thread-one"})
            ),
        ]
    );
    rpc.respond("workspace.statusline", response("feat/two"));
    assert!(body(provider.read(&dispatcher, URI, &first).await.unwrap()).contains("feat/one"));
    assert_eq!(rpc.calls().len(), 2);
    let second = body(
        provider
            .read(&dispatcher, URI, &meta("thread-two"))
            .await
            .unwrap(),
    );
    assert!(second.contains("feat/two") && !second.contains("feat/one"));
    assert_eq!(rpc.calls().len(), 3);
    assert!(
        rpc.calls()
            .iter()
            .all(|(method, _)| method != "audit.record")
    );
}

#[tokio::test]
async fn cached_failures_hide_raw_errors_and_refresh_after_expiry() {
    let rpc = RecordingDaemon::default();
    rpc.fail(
        "repository.resolve",
        DaemonFailure::new("PRIVATE", "secret-provider-recipe"),
    );
    let dispatcher = Dispatcher::new(rpc.clone(), "/fixed/repo".into());
    let provider = Provider::default();
    for thread in ["first", "second"] {
        let tree = body(
            provider
                .read(&dispatcher, URI, &meta(thread))
                .await
                .unwrap(),
        );
        assert!(tree.contains("CoCo unavailable") && !tree.contains("secret-provider-recipe"));
        assert!(tree.contains("Native Model"));
    }
    assert_eq!(rpc.calls().len(), 1);
    provider.cache.lock().await.retry_after = Some(Instant::now() - Duration::from_secs(1));
    provider
        .read(&dispatcher, URI, &meta("second"))
        .await
        .unwrap();
    assert_eq!(rpc.calls().len(), 2);
}

#[tokio::test]
async fn cached_workspace_expiry_and_query_failure_cannot_keep_showing_an_old_binding() {
    let rpc = RecordingDaemon::default();
    rpc.respond("workspace.statusline", response("feat/old"));
    let dispatcher = Dispatcher::new(rpc.clone(), "/fixed/repo".into());
    let provider = Provider::default();
    let metadata = meta("thread");
    assert!(body(provider.read(&dispatcher, URI, &metadata).await.unwrap()).contains("feat/old"));
    provider.cache.lock().await.last.as_mut().unwrap().at = Instant::now() - Duration::from_secs(2);
    rpc.fail(
        "workspace.statusline",
        DaemonFailure::new("PRIVATE", "hidden-details"),
    );
    let result = body(provider.read(&dispatcher, URI, &metadata).await.unwrap());
    assert!(result.contains("CoCo unavailable"));
    assert!(!result.contains("feat/old") && !result.contains("hidden-details"));
    let count = rpc.calls().len();
    provider.read(&dispatcher, URI, &metadata).await.unwrap();
    assert_eq!(rpc.calls().len(), count);
}

#[derive(Clone)]
struct Slow {
    rpc: RecordingDaemon,
    queries: Arc<std::sync::atomic::AtomicUsize>,
}

#[async_trait]
impl DaemonRpc for Slow {
    async fn request<R>(&self, request: R) -> Result<R::Response, DaemonFailure>
    where
        R: DaemonRequest + Send,
        R::Response: Send,
    {
        if R::METHOD == crate::protocol::DaemonMethod::WorkspaceStatusline {
            self.queries
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::future::pending::<()>().await;
        }
        self.rpc.request(request).await
    }
}

#[tokio::test]
async fn a_stalled_read_uses_a_bounded_fallback_and_does_not_immediately_retry() {
    let queries = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let dispatcher = Dispatcher::new(
        Slow {
            rpc: RecordingDaemon::default(),
            queries: queries.clone(),
        },
        "/fixed/repo".into(),
    );
    let provider = Provider::default();
    let start = Instant::now();
    let tree = body(
        provider
            .read(&dispatcher, URI, &meta("thread"))
            .await
            .unwrap(),
    );
    // Allow scheduler jitter in ordinary parallel CI. The actual timeout
    // budget is below the native deadline; the real TUI proof checks rendering
    // instead of making host load a strict wall-clock unit gate.
    assert!(READ_BUDGET < Duration::from_secs(1));
    assert!(start.elapsed() < Duration::from_secs(3));
    assert!(tree.contains("CoCo unavailable") && tree.contains("Native Model"));
    assert_eq!(queries.load(std::sync::atomic::Ordering::SeqCst), 1);
    provider
        .read(&dispatcher, URI, &meta("thread"))
        .await
        .unwrap();
    assert_eq!(queries.load(std::sync::atomic::Ordering::SeqCst), 1);
}
