use super::*;

#[tokio::test]
async fn normalizes_codex_events_and_allows_an_idempotent_follow_up_turn() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let created = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap();
    let workspace = created.workspace;

    let first_send = TurnStartParams {
        scope: RepositoryScope::repository(fixture.source.clone()),
        workspace: workspace.name.clone(),
        message: "Implement the requested behavior".to_owned(),
        operation_id: "send-operation-initial".to_owned(),
    };
    let first_started = fixture.coordinator.start_turn(first_send).await.unwrap();
    assert_eq!(first_started.codex_turn_id.as_deref(), Some("turn-1"));
    assert_eq!(first_started.workspace.phase, WorkspacePhase::Active);
    assert_workspace_events_exclude(
        &fixture,
        &workspace,
        EventKind::MessageReceived,
        "Implement the requested behavior",
    );

    record_and_approve_command(&fixture, &workspace).await;

    complete_first_turn(&fixture);
    let persisted = fixture
        .store
        .workspace_by_id(&workspace.id)
        .unwrap()
        .unwrap();
    assert_eq!(persisted.phase, WorkspacePhase::Unavailable);
    assert!(persisted.active_turn_id.is_none());
    assert_eq!(
        fixture
            .coordinator
            .get_workspace(WorkspaceGetParams {
                scope: RepositoryScope::repository(fixture.source.clone()),
                workspace: workspace.id.clone(),
            })
            .await
            .unwrap()
            .workspace
            .phase,
        WorkspacePhase::Idle
    );

    let send = TurnStartParams {
        scope: RepositoryScope::repository(fixture.source.clone()),
        workspace: workspace.name.clone(),
        message: "Run the final checks".to_owned(),
        operation_id: "send-operation-1".to_owned(),
    };
    let started = fixture.coordinator.start_turn(send.clone()).await.unwrap();
    assert_eq!(started.codex_turn_id.as_deref(), Some("turn-2"));
    let calls_after_start = fixture.worker.calls().len();
    let turn_calls_after_start = fixture
        .worker
        .calls()
        .iter()
        .filter(|call| matches!(call, WorkerCall::Turn { .. }))
        .count();

    let replay = fixture.coordinator.start_turn(send.clone()).await.unwrap();
    assert_eq!(replay.turn_id, started.turn_id);
    assert_eq!(fixture.worker.calls().len(), calls_after_start + 1);
    assert_eq!(
        fixture
            .worker
            .calls()
            .iter()
            .filter(|call| matches!(call, WorkerCall::Turn { .. }))
            .count(),
        turn_calls_after_start
    );

    let mut conflict = send;
    conflict.message = "A different retry".to_owned();
    assert!(matches!(
        fixture.coordinator.start_turn(conflict).await,
        Err(CoordinatorError::IdempotencyConflict)
    ));
}

fn complete_first_turn(fixture: &Fixture) {
    fixture
        .coordinator
        .record_codex_event(CodexEvent::Notification {
            method: "item/completed".to_owned(),
            params: json!({
                "threadId": "thread-1",
                "turnId": "turn-1",
                "completedAtMs": 42,
                "item": {"id": "message-1", "type": "agentMessage", "text": "Done"},
            }),
        })
        .unwrap();
    fixture
        .worker
        .set_native_status("thread-1", CodexThreadStatus::Idle);
    fixture
        .coordinator
        .record_codex_event(CodexEvent::Notification {
            method: "turn/completed".to_owned(),
            params: json!({
                "threadId": "thread-1",
                "turn": {"id": "turn-1", "status": "completed"},
            }),
        })
        .unwrap();
}

fn assert_workspace_events_exclude(
    fixture: &Fixture,
    workspace: &Workspace,
    kind: EventKind,
    text: &str,
) {
    let events = fixture.store.events_after(Some(&workspace.id), 0).unwrap();
    assert!(events.iter().all(|event| event.kind != kind));
    assert!(!serde_json::to_string(&events).unwrap().contains(text));
}

fn assert_decision_is_runtime_only(fixture: &Fixture, decision_id: &str) {
    assert!(fixture.store.decision_by_id(decision_id).unwrap().is_none());
}

fn assert_decision_is_absent_from_new_generation(fixture: &Fixture, decision_id: String) {
    let next_generation = fixture.recovery_coordinator(
        Arc::clone(&fixture.worker),
        "runtime-without-the-live-request",
    );
    assert_eq!(
        next_generation
            .get_decision(DecisionGetParams { decision_id })
            .unwrap_err()
            .code(),
        "NOT_FOUND"
    );
}

async fn record_and_approve_command(fixture: &Fixture, workspace: &Workspace) {
    fixture
        .coordinator
        .record_codex_event(CodexEvent::ServerRequest {
            id: json!(17),
            method: "item/commandExecution/requestApproval".to_owned(),
            params: json!({
                "threadId": "thread-1",
                "turnId": "turn-1",
                "reason": "needs network",
                "additionalPermissions": {
                    "fileSystem": {"write": ["/shared/cache"]},
                    "network": {"enabled": true}
                },
                "environment": {"TOKEN": "must-not-persist"},
            }),
        })
        .unwrap();
    assert_workspace_events_exclude(
        fixture,
        workspace,
        EventKind::DecisionRequested,
        "must-not-persist",
    );
    let status = fixture
        .coordinator
        .get_workspace(WorkspaceGetParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id.clone(),
        })
        .await
        .unwrap();
    let decision = status.open_decisions.first().unwrap();
    assert_eq!(decision.state, DecisionState::Pending);
    assert!(
        !serde_json::to_string(decision)
            .unwrap()
            .contains("must-not-persist")
    );
    assert!(
        serde_json::to_string(decision)
            .unwrap()
            .contains("/shared/cache")
    );
    assert!(matches!(decision.prompt, DecisionPrompt::Approval(_)));
    let decision_id = decision.id.clone();
    assert_decision_is_runtime_only(fixture, &decision_id);
    let (first, second) = tokio::join!(
        fixture.coordinator.respond_decision(DecisionRespondParams {
            decision_id: decision_id.clone(),
            submission: DecisionSubmission::Choice { choice: 1 },
        }),
        fixture.coordinator.respond_decision(DecisionRespondParams {
            decision_id: decision_id.clone(),
            submission: DecisionSubmission::Choice { choice: 1 },
        })
    );
    let (submitted, rejected) = match (first, second) {
        (Ok(submitted), Err(rejected)) | (Err(rejected), Ok(submitted)) => (submitted, rejected),
        unexpected => panic!("expected exactly one accepted decision response: {unexpected:?}"),
    };
    assert_eq!(rejected.code(), "INVALID_DECISION_STATE");
    assert_eq!(submitted.decision.state, DecisionState::Submitted);
    let responses = fixture
        .worker
        .calls()
        .into_iter()
        .filter(|call| matches!(call, WorkerCall::Response { .. }))
        .collect::<Vec<_>>();
    assert!(matches!(
        responses.as_slice(),
        [WorkerCall::Response { id, result }]
            if id == &json!(17) && result == &json!({"decision": "accept"})
    ));
    fixture
        .coordinator
        .record_codex_event(CodexEvent::Notification {
            method: "serverRequest/resolved".to_owned(),
            params: json!({"threadId": "thread-1", "requestId": 17}),
        })
        .unwrap();
    assert!(
        fixture
            .coordinator
            .open_decisions_for_workspace(&workspace.id)
            .is_empty()
    );
    let resolved = fixture
        .coordinator
        .get_decision(DecisionGetParams { decision_id })
        .unwrap();
    assert_eq!(resolved.decision.state, DecisionState::Resolved);
}

#[tokio::test]
async fn forwards_validated_user_input_without_exposing_answer_values() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    fixture
        .coordinator
        .start_turn(TurnStartParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id.clone(),
            message: "Ask me a question".to_owned(),
            operation_id: "question-turn".to_owned(),
        })
        .await
        .unwrap();
    fixture
        .coordinator
        .record_codex_event(CodexEvent::ServerRequest {
            id: json!("native-question-1"),
            method: "item/tool/requestUserInput".to_owned(),
            params: json!({
                "threadId": "thread-1",
                "turnId": "turn-1",
                "itemId": "question-item",
                "isBlocking": true,
                "questions": [{
                    "id": "strategy",
                    "header": "Strategy",
                    "question": "Which strategy should Codex use?",
                    "options": [
                        {"label": "Safe", "description": "Prefer safety"},
                        {"label": "Fast", "description": "Prefer speed"}
                    ],
                    "isOther": true,
                    "isSecret": false
                }]
            }),
        })
        .unwrap();
    let status = fixture
        .coordinator
        .get_workspace(WorkspaceGetParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id.clone(),
        })
        .await
        .unwrap();
    let decision = status.open_decisions.first().unwrap();
    assert_eq!(decision.kind, DecisionKind::UserInput);
    let decision_id = decision.id.clone();
    assert_decision_is_runtime_only(&fixture, &decision_id);
    let calls_before = fixture.worker.calls().len();

    let invalid = fixture
        .coordinator
        .respond_decision(DecisionRespondParams {
            decision_id: decision_id.clone(),
            submission: DecisionSubmission::Answers {
                answers: BTreeMap::from([("strategy".to_owned(), "  ".to_owned())]),
            },
        })
        .await
        .unwrap_err();
    assert_eq!(invalid.code(), "INVALID_PARAMS");
    assert_eq!(fixture.worker.calls().len(), calls_before);

    let private_answer = "A private custom strategy".to_owned();
    fixture
        .coordinator
        .respond_decision(DecisionRespondParams {
            decision_id: decision_id.clone(),
            submission: DecisionSubmission::Answers {
                answers: BTreeMap::from([("strategy".to_owned(), private_answer.clone())]),
            },
        })
        .await
        .unwrap();
    assert!(matches!(
        fixture.worker.calls().last(),
        Some(WorkerCall::Response { id, result })
            if id == &json!("native-question-1")
                && result == &json!({
                    "answers": {"strategy": {"answers": ["A private custom strategy"]}}
                })
    ));
    let submitted = fixture
        .coordinator
        .get_decision(DecisionGetParams { decision_id })
        .unwrap();
    assert_eq!(submitted.decision.state, DecisionState::Submitted);
    assert!(
        !serde_json::to_string(&submitted)
            .unwrap()
            .contains(&private_answer)
    );
    assert_decision_is_absent_from_new_generation(&fixture, submitted.decision.id);
}

#[tokio::test]
async fn presents_bounded_file_changes_and_orphans_them_on_disconnect() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    fixture
        .coordinator
        .start_turn(TurnStartParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id.clone(),
            message: "Change a file".to_owned(),
            operation_id: "file-turn".to_owned(),
        })
        .await
        .unwrap();
    fixture
        .coordinator
        .record_codex_event(CodexEvent::Notification {
            method: "item/started".to_owned(),
            params: json!({
                "threadId": "thread-1",
                "turnId": "turn-1",
                "startedAtMs": 20,
                "item": {
                    "id": "file-item",
                    "type": "fileChange",
                    "status": "inProgress",
                    "changes": [{
                        "path": "src/main.rs",
                        "kind": {"type": "update", "move_path": null},
                        "diff": "@@ -1 +1 @@\n-old\n+new\n"
                    }]
                }
            }),
        })
        .unwrap();
    fixture
        .coordinator
        .record_codex_event(CodexEvent::ServerRequest {
            id: json!(18),
            method: "item/fileChange/requestApproval".to_owned(),
            params: json!({
                "threadId": "thread-1",
                "turnId": "turn-1",
                "itemId": "file-item",
                "startedAtMs": 21,
                "reason": "Apply the patch"
            }),
        })
        .unwrap();
    let status = fixture
        .coordinator
        .get_workspace(WorkspaceGetParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id.clone(),
        })
        .await
        .unwrap();
    let decision = status.open_decisions.first().unwrap();
    let DecisionPrompt::Approval(prompt) = &decision.prompt else {
        panic!("file approval was not projected as an approval")
    };
    assert_eq!(prompt.changes.len(), 1);
    assert_eq!(prompt.changes[0].path, PathBuf::from("src/main.rs"));
    assert_eq!(prompt.changes[0].kind, "update");
    assert!(prompt.changes[0].diff.contains("+new"));
    let decision_id = decision.id.clone();

    assert_eq!(fixture.coordinator.record_codex_disconnected().unwrap(), 0);
    let orphaned = fixture
        .coordinator
        .get_decision(DecisionGetParams { decision_id })
        .unwrap();
    assert_eq!(orphaned.decision.state, DecisionState::Orphaned);
}
