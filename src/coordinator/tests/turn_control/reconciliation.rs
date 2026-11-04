use super::*;

async fn recover_completion(
    fixture: &Fixture,
    workspace: &Workspace,
    sent: &WorkspaceResult,
    key: &str,
) {
    let thread = sent.workspace.codex_thread_id.clone().unwrap();
    fixture
        .worker
        .set_native_status(&thread, CodexThreadStatus::Idle);
    fixture.worker.turn_control.outcomes.lock().unwrap().insert(
        (thread, sent.codex_turn_id.clone().unwrap()),
        TurnTerminalStatus::Completed,
    );
    let observed = fixture
        .coordinator
        .get_turn(get_params(fixture, workspace, key))
        .await
        .unwrap();
    assert_eq!(observed.state, TurnObservationState::Finished);
    assert!(
        fixture
            .coordinator
            .runtime_turn_for_workspace(workspace)
            .is_none()
    );
}

fn message(fixture: &Fixture, thread: &str, turn: &str, text: &str) {
    fixture
        .coordinator
        .record_codex_event(CodexEvent::Notification {
            method: "item/completed".into(),
            params: json!({"threadId": thread, "turnId": turn,
            "item": {"type": "agentMessage", "text": text}}),
        })
        .unwrap();
}

fn result(fixture: &Fixture, key: &str) -> TurnResult {
    fixture
        .coordinator
        .turn_result(TurnResultParams {
            operation_id: key.into(),
        })
        .unwrap()
}

#[tokio::test]
async fn metadata_completion_unblocks_a_new_send_and_keeps_late_output_exact() {
    let (fixture, workspace) = setup(FakeWorker::default()).await;
    let first = send(&fixture, &workspace, "repaired").await;
    recover_completion(&fixture, &workspace, &first, "repaired").await;
    assert!(matches!(
        result(&fixture, "repaired"),
        TurnResult::Pending { .. }
    ));
    let next = send(&fixture, &workspace, "next-after-repair").await;
    let thread = first.workspace.codex_thread_id.as_deref().unwrap();
    message(
        &fixture,
        thread,
        first.codex_turn_id.as_deref().unwrap(),
        "Late final answer",
    );
    fixture
        .coordinator
        .record_codex_event(CodexEvent::Notification {
            method: "turn/completed".into(),
            params: json!({"threadId": thread,
            "turn": {"id": first.codex_turn_id, "status": "completed"}}),
        })
        .unwrap();
    assert!(
        matches!(result(&fixture, "repaired"), TurnResult::Finished {
        response: Some(text), status: TurnTerminalStatus::Completed, .. } if text == "Late final answer")
    );
    let old = fixture
        .coordinator
        .get_turn(get_params(&fixture, &workspace, "repaired"))
        .await
        .unwrap();
    assert_eq!(old.codex_turn_id, first.codex_turn_id);
    assert_eq!(
        fixture
            .coordinator
            .runtime_turn_for_workspace(&workspace)
            .unwrap()
            .native_result_id,
        next.codex_turn_id
    );
    assert!(matches!(
        result(&fixture, "next-after-repair"),
        TurnResult::Pending { .. }
    ));
}

#[tokio::test]
async fn an_already_saved_receipt_also_reconciles_the_matching_guard() {
    let (fixture, workspace) = setup(FakeWorker::default()).await;
    let sent = send(&fixture, &workspace, "saved-before-read").await;
    let op = fixture
        .store
        .operation_by_client_id("saved-before-read")
        .unwrap()
        .unwrap();
    fixture
        .store
        .record_operation_completion(
            &op.id,
            sent.workspace.codex_thread_id.as_deref().unwrap(),
            sent.codex_turn_id.as_deref().unwrap(),
            TurnTerminalStatus::Completed,
        )
        .unwrap();
    fixture
        .coordinator
        .get_turn(get_params(&fixture, &workspace, "saved-before-read"))
        .await
        .unwrap();
    assert!(
        fixture
            .coordinator
            .runtime_turn_for_workspace(&workspace)
            .is_none()
    );
    assert!(
        fixture
            .worker
            .turn_control
            .lookups
            .lock()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn repaired_output_does_not_wait_forever_for_a_missing_completion_notification() {
    let (fixture, workspace) = setup(FakeWorker::default()).await;
    let sent = send(&fixture, &workspace, "bounded-output").await;
    recover_completion(&fixture, &workspace, &sent, "bounded-output").await;
    tokio::time::sleep(std::time::Duration::from_millis(2100)).await;
    assert!(matches!(
        result(&fixture, "bounded-output"),
        TurnResult::Finished {
            status: TurnTerminalStatus::Completed,
            response: None,
            ..
        }
    ));
}

#[tokio::test]
async fn late_old_events_do_not_overwrite_a_new_turn_before_its_start_reply() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let (fixture, workspace) = setup(FakeWorker::paused_turn_start(
        entered.clone(),
        release.clone(),
    ))
    .await;
    release.notify_one();
    let first = send(&fixture, &workspace, "late-old").await;
    entered.notified().await;
    recover_completion(&fixture, &workspace, &first, "late-old").await;
    let start = fixture.coordinator.start_turn(TurnStartParams {
        scope: RepositoryScope::repository(fixture.source.clone()),
        workspace: workspace.id.clone(),
        message: "New instruction".into(),
        operation_id: "new-pre-ack".into(),
    });
    tokio::pin!(start);
    tokio::select! { _ = entered.notified() => {}, done = &mut start => panic!("start did not pause: {done:?}") }
    let thread = first.workspace.codex_thread_id.as_deref().unwrap();
    message(&fixture, thread, "turn-2", "New answer before ack");
    message(
        &fixture,
        thread,
        first.codex_turn_id.as_deref().unwrap(),
        "Old delayed answer",
    );
    fixture.coordinator.record_codex_event(CodexEvent::Notification { method: "turn/completed".into(),
        params: json!({"threadId": thread, "turn": {"id": first.codex_turn_id, "status": "completed"}}) }).unwrap();
    release.notify_one();
    let next = start.await.unwrap();
    complete(&fixture, &next, TurnTerminalStatus::Completed);
    assert!(
        matches!(result(&fixture, "new-pre-ack"), TurnResult::Finished { response: Some(text), .. } if text == "New answer before ack")
    );
    assert!(
        matches!(result(&fixture, "late-old"), TurnResult::Finished { response: Some(text), .. } if text == "Old delayed answer")
    );
}
