use super::*;
use crate::protocol::WorkspaceSetupContext;

fn fork(fixture: &Fixture, source: &Workspace) -> WorkspaceSetupParams {
    let mut request = params(fixture);
    request.request.name = "public-child".into();
    request.request.context = WorkspaceSetupContext::Fork {
        workspace: source.id.clone(),
        compact: false,
    };
    request
}

#[tokio::test]
async fn public_fork_captures_busy_source_now_without_changing_the_independent_code_base() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let source = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let source_path = source.worktree_path.as_deref().unwrap();
    fs::write(source_path.join("source-only.txt"), "source commit\n").unwrap();
    run_git(source_path, &["add", "source-only.txt"]);
    run_git(source_path, &["commit", "-m", "source changes"]);
    fixture
        .coordinator
        .start_turn(TurnStartParams {
            scope: RepositoryScope::repository(&fixture.source),
            workspace: source.id.clone(),
            message: "keep working".into(),
            operation_id: "busy-source".into(),
        })
        .await
        .unwrap();
    let request = fork(&fixture, &source);
    let created = fixture
        .coordinator
        .setup_workspace(request.clone())
        .await
        .unwrap();
    assert_eq!(created.state, WorkspaceCreationState::Configured);
    assert_eq!(created.codex_thread_id.as_deref(), Some("fork-thread-1"));
    assert_eq!(
        created.base_sha.as_deref(),
        Some(git_output(&fixture.source, &["rev-parse", "HEAD"]).as_str())
    );
    assert!(
        !created
            .worktree_path
            .as_ref()
            .unwrap()
            .join("source-only.txt")
            .exists()
    );
    let child = workspace(&fixture, &created);
    assert_eq!(child.parent_thread_id, source.codex_thread_id);
    assert_eq!(
        child.context["resolved"]["context"]["lastTurnId"],
        "thread-1-completed"
    );
    fixture
        .worker
        .failed_thread_reads
        .lock()
        .unwrap()
        .push(source.codex_thread_id.unwrap());
    assert_eq!(
        fixture.coordinator.setup_workspace(request).await.unwrap(),
        created
    );
    assert_eq!(
        fixture
            .worker
            .calls()
            .iter()
            .filter(|call| matches!(call, WorkerCall::Fork { .. }))
            .count(),
        1
    );
}

#[tokio::test]
async fn a_changed_context_or_compaction_policy_conflicts_instead_of_recapturing() {
    let fixture = Fixture::new(FakeWorker::default());
    let source = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let request = fork(&fixture, &source);
    fixture
        .coordinator
        .setup_workspace(request.clone())
        .await
        .unwrap();
    for context in [
        WorkspaceSetupContext::Fresh,
        WorkspaceSetupContext::Fork {
            workspace: source.name,
            compact: true,
        },
    ] {
        let mut changed = request.clone();
        changed.request.context = context;
        assert_eq!(
            fixture
                .coordinator
                .setup_workspace(changed)
                .await
                .unwrap_err()
                .code(),
            "IDEMPOTENCY_CONFLICT"
        );
    }
}

#[tokio::test]
async fn foreign_context_ids_and_raw_threads_are_rejected_without_native_reads_or_git_effects() {
    let fixture = Fixture::new(FakeWorker::default());
    let other = Fixture::new(FakeWorker::default());
    let mut source_params = other.create_params();
    source_params.operation_id = "foreign-source".into();
    let foreign = fixture
        .coordinator
        .create_workspace(source_params)
        .await
        .unwrap()
        .workspace;
    let before = fixture.worker.calls();
    for (index, reference) in [foreign.id, "thread:raw-native".into(), "unknown".into()]
        .into_iter()
        .enumerate()
    {
        let mut request = params(&fixture);
        request.request.operation_id = format!("bad-source-{index}");
        request.request.context = WorkspaceSetupContext::Fork {
            workspace: reference,
            compact: false,
        };
        let receipt = fixture.coordinator.setup_workspace(request).await.unwrap();
        assert_eq!(receipt.state, WorkspaceCreationState::Rejected);
        assert_eq!(receipt.error_code.as_deref(), Some("WORKSPACE_NOT_FOUND"));
        assert!(receipt.workspace_id.is_none());
    }
    assert_eq!(fixture.worker.calls(), before);
    assert_eq!(
        git_output(&fixture.source, &["branch", "--format=%(refname:short)"]),
        "main"
    );
}

#[tokio::test]
async fn bound_context_sources_and_binding_plus_fork_remain_rejected() {
    let fixture = Fixture::new(FakeWorker::default());
    let mut source_request = params(&fixture);
    source_request.request.worker_mcp = Some(worker_file(&fixture, "bound"));
    let receipt = fixture
        .coordinator
        .setup_workspace(source_request)
        .await
        .unwrap();
    let source = fixture
        .coordinator
        .materialize_workspace_thread(workspace(&fixture, &receipt))
        .await
        .unwrap();
    let mut request = fork(&fixture, &source);
    request.request.operation_id = "bound-source-fork".into();
    let rejected = fixture
        .coordinator
        .setup_workspace(request.clone())
        .await
        .unwrap();
    assert_eq!(rejected.state, WorkspaceCreationState::Rejected);
    assert_eq!(rejected.error_code.as_deref(), Some("INVALID_PARAMS"));
    request.request.operation_id = "binding-plus-fork".into();
    request.request.worker_mcp = Some(worker_file(&fixture, "different"));
    let rejected = fixture.coordinator.setup_workspace(request).await.unwrap();
    assert_eq!(rejected.state, WorkspaceCreationState::Rejected);
    assert_eq!(rejected.error_code.as_deref(), Some("INVALID_PARAMS"));
    assert!(
        !fixture
            .worker
            .calls()
            .iter()
            .any(|call| matches!(call, WorkerCall::Fork { .. }))
    );
}

#[test]
fn default_and_explicit_fresh_keep_the_exact_previous_creation_hash_bytes() {
    let fixture = Fixture::new(FakeWorker::default());
    let request = params(&fixture).request;
    let bytes = serde_json::to_vec(&request).unwrap();
    let expected = format!(
        "{{\"operationId\":\"public-setup-key\",\"name\":\"first-workspace\",\"worktree\":{},\"changes\":\"reject\",\"profile\":\"default\",\"model\":\"fixture-model\",\"workerMcp\":null}}",
        serde_json::to_string(&request.worktree).unwrap()
    );
    assert_eq!(bytes, expected.as_bytes());
    let mut value = serde_json::to_value(request).unwrap();
    value["context"] = json!({"kind":"fresh"});
    let explicit: WorkspaceSetupRequest = serde_json::from_value(value).unwrap();
    assert_eq!(serde_json::to_vec(&explicit).unwrap(), bytes);
}
