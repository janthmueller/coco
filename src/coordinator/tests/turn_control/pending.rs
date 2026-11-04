use super::*;

fn event(fixture: &Fixture, thread: &str, turn: &str, text: &str) {
    for (method, params) in [
        (
            "item/completed",
            json!({"threadId": thread, "turnId": turn,
            "item": {"type": "agentMessage", "text": text}}),
        ),
        (
            "turn/completed",
            json!({"threadId": thread,
            "turn": {"id": turn, "status": "completed"}}),
        ),
    ] {
        fixture
            .coordinator
            .record_codex_event(CodexEvent::Notification {
                method: method.into(),
                params,
            })
            .unwrap();
    }
}

fn output(fixture: &Fixture, key: &str) -> TurnResult {
    fixture
        .coordinator
        .turn_result(TurnResultParams {
            operation_id: key.into(),
        })
        .unwrap()
}

async fn evict_old_result(
    fixture: &Fixture,
    workspace: &Workspace,
    entered: &Notify,
    release: &Notify,
) -> WorkspaceResult {
    release.notify_one();
    let first = send(fixture, workspace, "old-before-eviction").await;
    entered.notified().await;
    complete(fixture, &first, TurnTerminalStatus::Completed);
    for index in 0..256 {
        release.notify_one();
        let sent = send(fixture, workspace, &format!("evict-{index}")).await;
        entered.notified().await;
        complete(fixture, &sent, TurnTerminalStatus::Completed);
    }
    assert!(matches!(
        output(fixture, "old-before-eviction"),
        TurnResult::Unavailable { .. }
    ));
    first
}

#[tokio::test]
async fn pre_ack_outcomes_and_outputs_survive_other_turns_in_either_order_and_real_eviction() {
    for evicted in [false, true] {
        for own_first in [false, true] {
            let entered = Arc::new(Notify::new());
            let release = Arc::new(Notify::new());
            let worker = FakeWorker {
                turn_control: WorkerState {
                    history_unavailable: true,
                    ..Default::default()
                },
                ..FakeWorker::paused_turn_start(entered.clone(), release.clone())
            };
            let (fixture, workspace) = setup(worker).await;
            let old = if evicted {
                evict_old_result(&fixture, &workspace, &entered, &release)
                    .await
                    .codex_turn_id
                    .unwrap()
            } else {
                "external-next-turn".into()
            };
            let sequence = fixture
                .worker
                .calls()
                .iter()
                .filter(|call| matches!(call, WorkerCall::Turn { .. }))
                .count()
                + 1;
            let own = format!("turn-{sequence}");
            let thread = workspace.codex_thread_id.as_deref().unwrap();
            let start = send(&fixture, &workspace, "pre-ack-exact");
            tokio::pin!(start);
            tokio::select! { _ = entered.notified() => {}, done = &mut start => panic!("start did not pause: {done:?}") }
            for id in if own_first {
                [&own, &old]
            } else {
                [&old, &own]
            } {
                event(&fixture, thread, id, id);
            }
            release.notify_one();
            let sent = start.await;
            assert_eq!(sent.codex_turn_id.as_deref(), Some(own.as_str()));
            let observed = fixture
                .coordinator
                .get_turn(get_params(&fixture, &workspace, "pre-ack-exact"))
                .await
                .unwrap();
            assert_eq!(observed.state, TurnObservationState::Finished);
            assert_eq!(observed.outcome, Some(TurnTerminalStatus::Completed));
            assert!(
                matches!(output(&fixture, "pre-ack-exact"), TurnResult::Finished { response: Some(text), .. } if text == own)
            );
            assert!(
                fixture
                    .coordinator
                    .runtime_turn_for_workspace(&workspace)
                    .is_none()
            );
            fixture
                .worker
                .set_native_status(thread, CodexThreadStatus::Idle);
            release.notify_one();
            send(&fixture, &workspace, "after-pre-ack").await;
            entered.notified().await;
        }
    }
}

async fn overflowed_start() -> (Fixture, Workspace, WorkspaceResult) {
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let (fixture, workspace) = setup(FakeWorker::paused_turn_start(
        entered.clone(),
        release.clone(),
    ))
    .await;
    let sent = {
        let start = send(&fixture, &workspace, "overflowed-start");
        tokio::pin!(start);
        tokio::select! { _ = entered.notified() => {}, done = &mut start => panic!("start did not pause: {done:?}") }
        let thread = workspace.codex_thread_id.as_deref().unwrap();
        for index in 0..32 {
            event(&fixture, thread, &format!("other-{index}"), "other output");
        }
        event(&fixture, thread, "turn-1", "lost behind safety bound");
        release.notify_one();
        start.await
    };
    (fixture, workspace, sent)
}

#[tokio::test]
async fn early_evidence_overflow_is_unknown_until_exact_terminal_metadata_repairs_it() {
    let (fixture, workspace, sent) = overflowed_start().await;
    let params = get_params(&fixture, &workspace, "overflowed-start");
    let observed = fixture.coordinator.get_turn(params.clone()).await.unwrap();
    assert_eq!(observed.dispatch, TurnDispatchState::Accepted);
    assert_eq!(observed.state, TurnObservationState::Unavailable);
    assert_eq!(
        observed.unavailable_reason,
        Some(TurnUnavailableReason::CompletionNotObserved)
    );
    assert!(matches!(
        output(&fixture, "overflowed-start"),
        TurnResult::Unavailable { .. }
    ));
    assert_eq!(
        fixture
            .coordinator
            .project_current_runtime_turn(workspace.clone())
            .phase,
        WorkspacePhase::Unavailable
    );
    let rejected = fixture
        .coordinator
        .start_turn(TurnStartParams {
            scope: params.scope.clone(),
            workspace: workspace.id.clone(),
            operation_id: "do-not-bypass-unknown".into(),
            message: "No guessing".into(),
        })
        .await
        .unwrap_err();
    assert!(matches!(
        rejected,
        CoordinatorError::InvalidWorkspaceState {
            actual: WorkspacePhase::Unavailable,
            ..
        }
    ));
    fixture.worker.turn_control.outcomes.lock().unwrap().insert(
        (
            sent.workspace.codex_thread_id.unwrap(),
            sent.codex_turn_id.unwrap(),
        ),
        TurnTerminalStatus::Completed,
    );
    let repaired = fixture.coordinator.get_turn(params).await.unwrap();
    assert_eq!(repaired.outcome, Some(TurnTerminalStatus::Completed));
    assert!(
        fixture
            .coordinator
            .runtime_turn_for_workspace(&workspace)
            .is_none()
    );
}

#[tokio::test]
async fn exact_in_progress_metadata_restores_tracking_after_early_evidence_overflow() {
    let (fixture, workspace, sent) = overflowed_start().await;
    *fixture.worker.turn_control.in_progress.lock().unwrap() = Some((
        sent.workspace.codex_thread_id.clone().unwrap(),
        sent.codex_turn_id.clone().unwrap(),
    ));
    let observed = fixture
        .coordinator
        .get_turn(get_params(&fixture, &workspace, "overflowed-start"))
        .await
        .unwrap();
    assert_eq!(observed.state, TurnObservationState::Running);
    assert!(
        !fixture
            .coordinator
            .runtime_turn_for_workspace(&workspace)
            .unwrap()
            .completion_tracking_lost
    );
    assert!(matches!(
        output(&fixture, "overflowed-start"),
        TurnResult::Pending { .. }
    ));
    complete(&fixture, &sent, TurnTerminalStatus::Interrupted);
    assert_eq!(
        fixture
            .coordinator
            .get_turn(get_params(&fixture, &workspace, "overflowed-start"))
            .await
            .unwrap()
            .outcome,
        Some(TurnTerminalStatus::Interrupted)
    );
}

#[tokio::test]
async fn metadata_reconciliation_before_binding_keeps_only_the_exact_early_output() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let (fixture, workspace) = setup(FakeWorker::paused_turn_start(
        entered.clone(),
        release.clone(),
    ))
    .await;
    let start = send(&fixture, &workspace, "metadata-before-bind");
    tokio::pin!(start);
    tokio::select! { _ = entered.notified() => {}, done = &mut start => panic!("start did not pause: {done:?}") }
    let thread = workspace.codex_thread_id.as_deref().unwrap();
    event(&fixture, thread, "turn-1", "Exact early answer");
    event(&fixture, thread, "other", "Unrelated early answer");
    // Interleave precisely after durable acceptance but before runtime binding.
    let operation = fixture
        .store
        .operation_by_client_id("metadata-before-bind")
        .unwrap()
        .unwrap();
    fixture
        .store
        .accept_operation(&operation.id, "turn-1")
        .unwrap();
    fixture.worker.turn_control.outcomes.lock().unwrap().insert(
        (thread.into(), "turn-1".into()),
        TurnTerminalStatus::Completed,
    );
    let observed = fixture
        .coordinator
        .get_turn(get_params(&fixture, &workspace, "metadata-before-bind"))
        .await
        .unwrap();
    assert_eq!(observed.outcome, Some(TurnTerminalStatus::Completed));
    assert!(
        matches!(output(&fixture, "metadata-before-bind"), TurnResult::Finished { response: Some(text), .. } if text == "Exact early answer")
    );
    assert!(
        fixture
            .coordinator
            .runtime_turn_for_workspace(&workspace)
            .is_none()
    );
    release.notify_one();
    start.await;
    assert!(
        matches!(output(&fixture, "metadata-before-bind"), TurnResult::Finished { response: Some(text), .. } if text == "Exact early answer")
    );
}
