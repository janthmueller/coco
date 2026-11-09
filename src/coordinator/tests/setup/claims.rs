use super::*;

#[tokio::test]
async fn private_create_waiting_at_dependency_gate_cannot_consume_a_later_public_claim() {
    for same_request in [false, true] {
        for with_worker in [false, true] {
            let fixture = Fixture::new(FakeWorker::default());
            let mut public = params(&fixture);
            if with_worker {
                public.request.worker_mcp = Some(worker_file(&fixture, "public-grant"));
            }
            let mut private = fixture.create_params();
            private.operation_id = public.request.operation_id.clone();
            if same_request {
                private.model = public.request.model.clone();
            } else {
                private.name = "private-target".into();
            }
            let expected_name = public.request.name.clone();
            let expected_model = public.request.model.clone();
            let expected_binding = public.request.worker_mcp.clone();
            let dependencies = fixture.coordinator.context_dependencies.lock().await;
            let mut private = Box::pin(fixture.coordinator.create_workspace(private));
            assert!(futures_util::poll!(private.as_mut()).is_pending());
            let retry = public.clone();
            let mut public = Box::pin(fixture.coordinator.setup_workspace(public));
            assert!(futures_util::poll!(public.as_mut()).is_pending());
            let intent = fixture
                .coordinator
                .get_workspace_creation(get(&fixture))
                .unwrap();
            assert_eq!(intent.state, WorkspaceCreationState::Preparing);
            assert!(intent.workspace_id.is_none());
            drop(dependencies);

            assert_eq!(private.await.unwrap_err().code(), "IDEMPOTENCY_CONFLICT");
            assert!(fixture.store.list_workspaces(None).unwrap().is_empty());
            assert_eq!(
                git_output(&fixture.source, &["branch", "--format=%(refname:short)"]),
                "main"
            );
            assert_eq!(
                git_output(&fixture.source, &["worktree", "list", "--porcelain"])
                    .lines()
                    .filter(|line| line.starts_with("worktree "))
                    .count(),
                1
            );
            assert_eq!(
                fixture
                    .coordinator
                    .get_workspace_creation(get(&fixture))
                    .unwrap(),
                intent
            );
            let created = public.await.unwrap();
            assert_eq!(created.state, WorkspaceCreationState::Configured);
            assert_eq!(
                created.workspace_name.as_deref(),
                Some(expected_name.as_str())
            );
            assert_eq!(created.profile.as_ref().unwrap().model, expected_model);
            assert_eq!(created.worker_mcp, expected_binding);
            assert_eq!(
                fixture.coordinator.setup_workspace(retry).await.unwrap(),
                created
            );
            assert_eq!(fixture.store.list_workspaces(None).unwrap().len(), 1);
            assert!(!fixture.worker.calls().iter().any(|call| matches!(
                call,
                WorkerCall::Thread { .. } | WorkerCall::Fork { .. } | WorkerCall::Turn { .. }
            )));
        }
    }
}

#[tokio::test]
async fn a_completed_private_create_cannot_be_adopted_by_public_setup() {
    let fixture = Fixture::new(FakeWorker::default());
    let mut private = fixture.create_params();
    private.operation_id = params(&fixture).request.operation_id;
    let original = fixture
        .coordinator
        .create_workspace(private.clone())
        .await
        .unwrap();
    let error = fixture
        .coordinator
        .setup_workspace(params(&fixture))
        .await
        .unwrap_err();
    assert_eq!(error.code(), "IDEMPOTENCY_CONFLICT");
    let receipt = fixture
        .coordinator
        .get_workspace_creation(get(&fixture))
        .unwrap();
    assert_eq!(
        receipt.workspace_id.as_deref(),
        Some(original.workspace.id.as_str())
    );
    assert_eq!(receipt.state, WorkspaceCreationState::Unavailable);
    assert!(receipt.worker_mcp.is_none() && receipt.request_sha256.is_none());
    assert_eq!(
        fixture
            .coordinator
            .create_workspace(private)
            .await
            .unwrap()
            .workspace
            .id,
        original.workspace.id
    );
}

#[tokio::test]
async fn a_foreign_private_create_cannot_consume_a_public_claim_for_another_repository() {
    let fixture = Fixture::new(FakeWorker::default());
    let other = Fixture::new(FakeWorker::default());
    let mut private = fixture.create_params();
    private.operation_id = params(&fixture).request.operation_id;
    let dependencies = fixture.coordinator.context_dependencies.lock().await;
    let mut private = Box::pin(fixture.coordinator.create_workspace(private));
    assert!(futures_util::poll!(private.as_mut()).is_pending());
    let mut public = params(&fixture);
    public.repository_path = other.source.clone();
    let mut public = Box::pin(fixture.coordinator.setup_workspace(public));
    assert!(futures_util::poll!(public.as_mut()).is_pending());
    drop(dependencies);
    assert_eq!(private.await.unwrap_err().code(), "IDEMPOTENCY_CONFLICT");
    let receipt = public.await.unwrap();
    assert_eq!(receipt.state, WorkspaceCreationState::Configured);
    assert_eq!(
        fixture
            .coordinator
            .get_workspace_creation(get(&fixture))
            .unwrap()
            .state,
        WorkspaceCreationState::Unknown
    );
    assert_eq!(fixture.store.list_workspaces(None).unwrap().len(), 1);
}
