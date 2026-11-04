use super::*;

#[tokio::test]
async fn malformed_completion_and_another_turn_never_create_a_terminal_receipt() {
    let (fixture, workspace) = setup(FakeWorker::default()).await;
    let sent = send(&fixture, &workspace, "strict-completion").await;
    for turn in [
        json!({"id": sent.codex_turn_id}),
        json!({"id": sent.codex_turn_id, "status": "future-state"}),
        json!({"id": "unrelated-turn", "status": "completed"}),
    ] {
        fixture
            .coordinator
            .record_codex_event(CodexEvent::Notification {
                method: "turn/completed".into(),
                params: json!({"threadId": sent.workspace.codex_thread_id, "turn": turn}),
            })
            .unwrap();
    }
    let observed = fixture
        .coordinator
        .get_turn(get_params(&fixture, &workspace, "strict-completion"))
        .await
        .unwrap();
    assert_eq!(observed.state, TurnObservationState::Running);
    assert!(observed.outcome.is_none());
    assert!(observed.outcome_observed_at_ms.is_none());
    complete(&fixture, &sent, TurnTerminalStatus::Interrupted);
    assert_eq!(
        fixture
            .coordinator
            .get_turn(get_params(&fixture, &workspace, "strict-completion"))
            .await
            .unwrap()
            .outcome,
        Some(TurnTerminalStatus::Interrupted)
    );
}

#[tokio::test]
async fn lookup_recovers_ignored_start_reply_and_identical_replay_conflicts_correctly() {
    let (fixture, workspace) = setup(FakeWorker::default()).await;
    let sent = send(&fixture, &workspace, "lost-client-reply").await;
    let found = fixture
        .coordinator
        .get_turn(get_params(&fixture, &workspace, "lost-client-reply"))
        .await
        .unwrap();
    assert_eq!(found.state, TurnObservationState::Running);
    assert_eq!(found.dispatch, TurnDispatchState::Accepted);
    assert_eq!(found.codex_turn_id, sent.codex_turn_id);
    assert_eq!(found.codex_thread_id, sent.workspace.codex_thread_id);
    assert_eq!(
        send(&fixture, &workspace, "lost-client-reply")
            .await
            .codex_turn_id,
        sent.codex_turn_id
    );
    let conflict = fixture
        .coordinator
        .start_turn(TurnStartParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id.clone(),
            message: "Changed instruction".to_owned(),
            operation_id: "lost-client-reply".to_owned(),
        })
        .await
        .unwrap_err();
    assert_eq!(conflict.code(), "IDEMPOTENCY_CONFLICT");
    assert_eq!(
        fixture
            .worker
            .calls()
            .iter()
            .filter(|call| matches!(call, WorkerCall::Turn { .. }))
            .count(),
        1
    );
}

#[tokio::test]
async fn completion_before_start_ack_remains_unconfirmed_until_exact_id_is_bound() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let (fixture, workspace) = setup(FakeWorker::paused_turn_start(
        entered.clone(),
        release.clone(),
    ))
    .await;
    let params = TurnStartParams {
        scope: RepositoryScope::repository(fixture.source.clone()),
        workspace: workspace.id.clone(),
        message: "Frozen".to_owned(),
        operation_id: "overtaking".to_owned(),
    };
    let start = fixture.coordinator.start_turn(params);
    tokio::pin!(start);
    tokio::select! { _ = entered.notified() => {}, result = &mut start => panic!("start did not pause: {result:?}") }
    let pending = fixture
        .coordinator
        .get_turn(get_params(&fixture, &workspace, "overtaking"))
        .await
        .unwrap();
    assert_eq!(pending.state, TurnObservationState::Starting);
    assert!(pending.codex_turn_id.is_none());
    fixture.coordinator.record_codex_event(CodexEvent::Notification { method: "turn/completed".to_owned(),
        params: json!({"threadId": workspace.codex_thread_id, "turn": {"id": "turn-1", "status": "failed"}}) }).unwrap();
    assert_eq!(
        fixture
            .coordinator
            .get_turn(get_params(&fixture, &workspace, "overtaking"))
            .await
            .unwrap()
            .state,
        TurnObservationState::Starting
    );
    release.notify_one();
    start.await.unwrap();
    let done = fixture
        .coordinator
        .get_turn(get_params(&fixture, &workspace, "overtaking"))
        .await
        .unwrap();
    assert_eq!(done.outcome, Some(TurnTerminalStatus::Failed));
    assert_eq!(done.state, TurnObservationState::Finished);
}

#[tokio::test]
async fn successive_operations_keep_independent_outcomes_and_unknown_ids_are_not_uncertain() {
    let (fixture, workspace) = setup(FakeWorker::default()).await;
    let first = send(&fixture, &workspace, "first").await;
    complete(&fixture, &first, TurnTerminalStatus::Failed);
    let second = send(&fixture, &workspace, "second").await;
    assert_ne!(first.codex_turn_id, second.codex_turn_id);
    assert_eq!(
        fixture
            .coordinator
            .get_turn(get_params(&fixture, &workspace, "first"))
            .await
            .unwrap()
            .outcome,
        Some(TurnTerminalStatus::Failed)
    );
    assert_eq!(
        fixture
            .coordinator
            .get_turn(get_params(&fixture, &workspace, "second"))
            .await
            .unwrap()
            .state,
        TurnObservationState::Running
    );
    assert_eq!(
        fixture
            .coordinator
            .get_turn(get_params(&fixture, &workspace, "never-sent"))
            .await
            .unwrap_err()
            .code(),
        "TURN_OPERATION_NOT_FOUND"
    );
}

#[tokio::test]
async fn scope_and_workspace_are_checked_before_any_native_read_or_interrupt() {
    let (fixture, workspace) = setup(FakeWorker::default()).await;
    send(&fixture, &workspace, "private-operation").await;
    let other_path = fixture._temp.path().join("other");
    initialize_repository(&other_path);
    fixture
        .coordinator
        .register_repository(RepositoryRegisterParams {
            path: other_path.clone(),
        })
        .await
        .unwrap();
    let other = fixture
        .coordinator
        .create_workspace(fresh_create_params(
            other_path.clone(),
            "other",
            "create-other",
        ))
        .await
        .unwrap()
        .workspace;
    for target in [&workspace.id, &other.id] {
        let get = TurnGetParams {
            scope: RepositoryScope::repository(other_path.clone()),
            workspace: target.clone(),
            operation_id: "private-operation".to_owned(),
        };
        let expected = if target == &workspace.id {
            "WORKSPACE_NOT_FOUND"
        } else {
            "TURN_OPERATION_NOT_FOUND"
        };
        assert_eq!(
            fixture
                .coordinator
                .get_turn(get.clone())
                .await
                .unwrap_err()
                .code(),
            expected
        );
        for mode in [TurnInterruptMode::Strict, TurnInterruptMode::BestEffort] {
            assert_eq!(
                fixture
                    .coordinator
                    .interrupt_turn(TurnInterruptParams {
                        scope: get.scope.clone(),
                        workspace: get.workspace.clone(),
                        operation_id: get.operation_id.clone(),
                        mode,
                    })
                    .await
                    .unwrap_err()
                    .code(),
                expected
            );
        }
    }
    assert!(
        fixture
            .worker
            .turn_control
            .lookups
            .lock()
            .unwrap()
            .is_empty()
    );
    assert!(
        fixture
            .worker
            .turn_control
            .interrupts
            .lock()
            .unwrap()
            .is_empty()
    );
}
