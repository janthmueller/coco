use super::*;

#[tokio::test]
async fn missing_source_blocks_send_even_if_the_native_thread_is_still_loaded() {
    let fixture = Fixture::new(FakeWorker::default());
    let mut request = params(&fixture);
    let reference = worker_file(&fixture, "run-a");
    request.request.worker_mcp = Some(reference.clone());
    let receipt = fixture.coordinator.setup_workspace(request).await.unwrap();
    let attached = fixture
        .coordinator
        .materialize_workspace_thread(workspace(&fixture, &receipt))
        .await
        .unwrap();
    fs::remove_file(&reference.path).unwrap();
    let error = fixture
        .coordinator
        .start_turn(TurnStartParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: attached.id,
            message: "must not dispatch".into(),
            operation_id: "missing-source-send".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(error.code(), "WORKER_MCP_BINDING_INVALID");
    assert!(
        !fixture
            .worker
            .calls()
            .iter()
            .any(|call| matches!(call, WorkerCall::Turn { .. }))
    );
}

#[tokio::test]
async fn native_loader_errors_do_not_forward_private_recipe_details() {
    let fixture = Fixture::new(FakeWorker::failing_thread_start());
    let mut request = params(&fixture);
    request.request.worker_mcp = Some(worker_file(&fixture, "run-a"));
    let receipt = fixture.coordinator.setup_workspace(request).await.unwrap();
    let error = fixture
        .coordinator
        .materialize_workspace_thread(workspace(&fixture, &receipt))
        .await
        .unwrap_err();
    assert_eq!(error.code(), "WORKER_MCP_BINDING_INVALID");
    assert!(!error.to_string().contains("injected"));
    let bound = workspace(&fixture, &receipt);
    let rejection = fixture.coordinator.worker_turn_error(
        &bound,
        WorkerError::TurnRejected(Box::new(std::io::Error::other(
            "DO_NOT_LEAK_REJECTED_RECIPE",
        ))),
    );
    assert!(matches!(rejection, WorkerError::TurnRejected(_)));
    assert!(!rejection.to_string().contains("DO_NOT_LEAK"));
    let ambiguous = fixture.coordinator.worker_turn_error(
        &bound,
        WorkerError::runtime(std::io::Error::other("DO_NOT_LEAK_UNCERTAIN_RECIPE")),
    );
    assert!(!matches!(ambiguous, WorkerError::TurnRejected(_)));
    assert!(!ambiguous.to_string().contains("DO_NOT_LEAK"));
}

#[tokio::test]
async fn immutable_recipe_is_applied_to_start_and_resume_without_polluting_profile() {
    let fixture = Fixture::new(FakeWorker::default());
    let mut request = params(&fixture);
    request.request.worker_mcp = Some(worker_file(&fixture, "run-a"));
    let created = fixture.coordinator.setup_workspace(request).await.unwrap();
    assert_eq!(created.state, WorkspaceCreationState::Configured);
    let prepared = workspace(&fixture, &created);
    assert!(
        prepared
            .profile
            .effective_settings
            .get("mcp_servers")
            .is_none()
    );
    let error = fixture
        .coordinator
        .attach_workspace(WorkspaceAttachParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: prepared.id.clone(),
            client: None,
        })
        .await
        .unwrap_err();
    assert_eq!(error.code(), "WORKER_MCP_FIRST_SEND_REQUIRED");
    let attached = fixture
        .coordinator
        .materialize_workspace_thread(prepared)
        .await
        .unwrap();
    assert!(
        attached.codex_thread_id.is_some(),
        "the loaded native binding must be recorded"
    );
    let calls = fixture.worker.calls();
    let config = calls
        .iter()
        .find_map(|call| match call {
            WorkerCall::Thread { config, .. } => Some(config),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        config["mcp_servers"]["worker"]["args"],
        json!(["--grant", "run-a"])
    );
    assert!(!serde_json::to_string(&created).unwrap().contains("--grant"));
    let recovered = fixture.recovery_coordinator(fixture.worker.clone(), "new-generation");
    recovered
        .ensure_workspace_thread_loaded(attached.clone())
        .await
        .unwrap();
    let calls = fixture.worker.calls();
    let resumed = calls
        .iter()
        .find_map(|call| match call {
            WorkerCall::Resume { config, .. } => Some(config),
            _ => None,
        })
        .unwrap();
    assert_eq!(config, resumed);
    let mut fork = fixture.fork_params(&attached, "leaking-fork", false);
    fork.context = WorkspaceContextRequest::Fork {
        source: WorkspaceContextSource::Thread {
            thread_id: attached.codex_thread_id.unwrap(),
        },
        compact: false,
    };
    assert_eq!(
        fixture
            .coordinator
            .create_workspace(fork)
            .await
            .unwrap_err()
            .code(),
        "INVALID_PARAMS"
    );
}

#[tokio::test]
async fn source_drift_blocks_activation_but_does_not_rewrite_historical_receipts() {
    let fixture = Fixture::new(FakeWorker::default());
    let mut request = params(&fixture);
    let reference = worker_file(&fixture, "run-a");
    request.request.worker_mcp = Some(reference.clone());
    let created = fixture
        .coordinator
        .setup_workspace(request.clone())
        .await
        .unwrap();
    let prepared = workspace(&fixture, &created);
    fs::write(&reference.path, "changed").unwrap();
    assert_eq!(
        fixture
            .coordinator
            .start_turn(TurnStartParams {
                scope: RepositoryScope::repository(fixture.source.clone()),
                workspace: prepared.id,
                message: "must not dispatch".into(),
                operation_id: "changed-source-send".into(),
            })
            .await
            .unwrap_err()
            .code(),
        "WORKER_MCP_BINDING_INVALID"
    );
    assert_eq!(
        fixture.coordinator.setup_workspace(request).await.unwrap(),
        created
    );
    assert!(
        !fixture
            .worker
            .calls()
            .iter()
            .any(|call| matches!(call, WorkerCall::Thread { .. }))
    );
}

#[tokio::test]
async fn invalid_file_profile_and_native_slot_collisions_are_durable_rejections_before_git_effects()
{
    for collision in ["missing", "profile", "native"] {
        let fixture = Fixture::new(FakeWorker::default());
        let mut request = params(&fixture);
        let reference = worker_file(&fixture, "run-a");
        request.request.worker_mcp = Some(reference.clone());
        match collision {
            "missing" => fs::remove_file(&reference.path).unwrap(),
            "profile" => {
                fs::create_dir_all(&fixture.codex_home).unwrap();
                fs::write(
                    fixture.codex_home.join("conflict.config.toml"),
                    "[mcp_servers.worker]\ncommand = 'OLD_PRIVATE_FIXTURE'\n",
                )
                .unwrap();
                request.request.profile = "conflict".into();
            }
            _ => {
                fixture
                    .worker
                    .native_worker_slots
                    .lock()
                    .unwrap()
                    .insert("worker".into());
            }
        }
        let receipt = fixture.coordinator.setup_workspace(request).await.unwrap();
        assert_eq!(
            receipt.state,
            WorkspaceCreationState::Rejected,
            "{collision}"
        );
        assert_eq!(
            receipt.error_code.as_deref(),
            Some("WORKER_MCP_BINDING_INVALID")
        );
        assert!(receipt.workspace_id.is_none());
        assert!(fixture.worktrees.read_dir().is_err());
        assert!(
            !serde_json::to_string(&receipt)
                .unwrap()
                .contains("OLD_PRIVATE_FIXTURE")
        );
    }
}
