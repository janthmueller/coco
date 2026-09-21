use super::*;

fn record_notification(fixture: &Fixture, method: &str, params: Value) {
    fixture
        .coordinator
        .record_codex_event(CodexEvent::Notification {
            method: method.to_owned(),
            params,
        })
        .unwrap();
}

async fn projected_status(fixture: &Fixture, workspace: &Workspace) -> WorkspaceStatusResult {
    fixture
        .coordinator
        .get_workspace(WorkspaceGetParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id.clone(),
            include_resources: false,
        })
        .await
        .unwrap()
}

fn record_thread_status(fixture: &Fixture, status: Value) {
    fixture
        .coordinator
        .record_codex_event(CodexEvent::Notification {
            method: "thread/status/changed".to_owned(),
            params: json!({"threadId": "thread-1", "status": status}),
        })
        .unwrap();
}

fn assert_status_notification_is_not_persisted(
    fixture: &Fixture,
    workspace: &Workspace,
    status: Value,
) {
    let before = fixture
        .store
        .workspace_by_id(&workspace.id)
        .unwrap()
        .unwrap();
    record_thread_status(fixture, status);
    let after = fixture
        .store
        .workspace_by_id(&workspace.id)
        .unwrap()
        .unwrap();
    assert_eq!(after, before);
}

async fn projected_workspace(fixture: &Fixture, workspace: &Workspace) -> Workspace {
    projected_status(fixture, workspace).await.workspace
}

fn start_reasoning_activity(fixture: &Fixture, turn_id: &str, item_id: &str, delta: &str) {
    record_notification(
        fixture,
        "turn/started",
        json!({"threadId": "thread-1", "turn": {"id": turn_id}}),
    );
    record_notification(
        fixture,
        "item/started",
        json!({
            "threadId": "thread-1",
            "turnId": turn_id,
            "item": {"id": item_id, "type": "reasoning"}
        }),
    );
    record_notification(
        fixture,
        "item/reasoning/summaryTextDelta",
        json!({
            "threadId": "thread-1",
            "turnId": turn_id,
            "itemId": item_id,
            "summaryIndex": 0,
            "delta": delta
        }),
    );
}

#[tokio::test]
async fn projects_reasoning_activity_without_persisting_or_exposing_it_through_list() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    fixture.worker.set_native_status(
        "thread-1",
        CodexThreadStatus::Active {
            active_flags: Vec::new(),
        },
    );
    let stored = fixture
        .store
        .workspace_by_id(&workspace.id)
        .unwrap()
        .unwrap();

    start_reasoning_activity(
        &fixture,
        "external-turn",
        "reasoning-1",
        "**Checking\t tests**",
    );
    let status = projected_status(&fixture, &workspace).await;
    let activity = status.activity.expect("status omitted native activity");
    assert_eq!(activity.label, "Checking tests");
    assert_eq!(activity.source, WorkspaceActivitySource::ReasoningSummary);
    assert_eq!(activity.thread_id, "thread-1");
    assert_eq!(activity.turn_id, "external-turn");
    assert_eq!(activity.item_id, "reasoning-1");
    assert_eq!(activity.runtime_generation, "runtime-test");

    let detailed = fixture
        .coordinator
        .list_workspaces(WorkspaceListParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            phases: None,
            include_resources: false,
            include_activity: true,
        })
        .await
        .unwrap();
    assert_eq!(
        detailed[0]
            .activity
            .as_ref()
            .map(|value| value.label.as_str()),
        Some("Checking tests")
    );

    let compact = fixture
        .coordinator
        .list_workspaces(WorkspaceListParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            phases: None,
            include_resources: false,
            include_activity: false,
        })
        .await
        .unwrap();
    assert_eq!(compact.len(), 1);
    assert!(compact[0].activity.is_none());
    assert_eq!(
        fixture
            .store
            .workspace_by_id(&workspace.id)
            .unwrap()
            .unwrap(),
        stored
    );
}

#[tokio::test]
async fn structured_activity_has_priority_and_clears_at_native_boundaries() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    fixture.worker.set_native_status(
        "thread-1",
        CodexThreadStatus::Active {
            active_flags: Vec::new(),
        },
    );
    start_reasoning_activity(&fixture, "turn-1", "reasoning-1", "**Checking tests**");

    record_notification(
        &fixture,
        "item/started",
        json!({
            "threadId": "thread-1",
            "turnId": "turn-1",
            "item": {"id": "compact-1", "type": "contextCompaction"}
        }),
    );
    let compaction = projected_status(&fixture, &workspace)
        .await
        .activity
        .expect("status omitted compaction activity");
    assert_eq!(compaction.label, "Compacting context");
    assert_eq!(
        compaction.source,
        WorkspaceActivitySource::ContextCompaction
    );

    record_notification(
        &fixture,
        "item/completed",
        json!({
            "threadId": "thread-1",
            "turnId": "turn-1",
            "item": {"id": "compact-1", "type": "contextCompaction"}
        }),
    );
    assert!(
        projected_status(&fixture, &workspace)
            .await
            .activity
            .is_none()
    );

    start_reasoning_activity(&fixture, "turn-2", "reasoning-2", "**Writing report**");
    record_notification(
        &fixture,
        "turn/completed",
        json!({
            "threadId": "thread-1",
            "turn": {"id": "older-turn", "status": "completed"}
        }),
    );
    assert!(
        projected_status(&fixture, &workspace)
            .await
            .activity
            .is_some()
    );
    fixture
        .worker
        .set_native_status("thread-1", CodexThreadStatus::Idle);
    assert!(
        projected_status(&fixture, &workspace)
            .await
            .activity
            .is_none()
    );
    fixture.worker.set_native_status(
        "thread-1",
        CodexThreadStatus::Active {
            active_flags: Vec::new(),
        },
    );
    assert!(
        projected_status(&fixture, &workspace)
            .await
            .activity
            .is_none()
    );

    start_reasoning_activity(&fixture, "turn-3", "reasoning-3", "**Final check**");
    assert!(
        projected_status(&fixture, &workspace)
            .await
            .activity
            .is_some()
    );
    fixture.coordinator.record_codex_disconnected().unwrap();
    assert!(
        projected_status(&fixture, &workspace)
            .await
            .activity
            .is_none()
    );
}

async fn assert_native_waiting_statuses(fixture: &Fixture, workspace: &Workspace) {
    assert_status_notification_is_not_persisted(
        fixture,
        workspace,
        json!({
            "type": "active",
            "activeFlags": ["waitingOnUserInput", "futureFlag", "waitingOnApproval"]
        }),
    );
    fixture.worker.set_native_status(
        "thread-1",
        CodexThreadStatus::Active {
            active_flags: vec![
                "waitingOnUserInput".to_owned(),
                "futureFlag".to_owned(),
                "waitingOnApproval".to_owned(),
            ],
        },
    );
    let waiting = projected_workspace(fixture, workspace).await;
    assert_eq!(waiting.phase, WorkspacePhase::WaitingForApproval);
    assert_eq!(
        waiting.wait_reasons,
        [
            WorkspaceWaitReason::Approval,
            WorkspaceWaitReason::UserInput
        ]
    );
    assert_eq!(
        waiting
            .thread_runtime
            .as_ref()
            .map(|snapshot| &snapshot.status),
        Some(&CodexThreadStatus::Active {
            active_flags: vec![
                "futureFlag".to_owned(),
                "waitingOnApproval".to_owned(),
                "waitingOnUserInput".to_owned(),
            ]
        })
    );

    fixture.worker.set_native_status(
        "thread-1",
        CodexThreadStatus::Active {
            active_flags: vec!["waitingOnUserInput".to_owned()],
        },
    );
    assert_eq!(
        projected_workspace(fixture, workspace).await.phase,
        WorkspacePhase::WaitingForInput
    );
    fixture.worker.set_native_status(
        "thread-1",
        CodexThreadStatus::Active {
            active_flags: Vec::new(),
        },
    );
    assert_eq!(
        projected_workspace(fixture, workspace).await.phase,
        WorkspacePhase::Active
    );
}

async fn assert_native_nonactive_statuses(fixture: &Fixture, workspace: &Workspace) {
    assert_eq!(
        projected_workspace(fixture, workspace).await.phase,
        WorkspacePhase::Idle
    );
    fixture
        .worker
        .set_native_status("thread-1", CodexThreadStatus::SystemError);
    assert_eq!(
        projected_workspace(fixture, workspace).await.phase,
        WorkspacePhase::SystemError
    );
    fixture
        .worker
        .set_native_status("thread-1", CodexThreadStatus::NotLoaded);
    assert_eq!(
        projected_workspace(fixture, workspace).await.phase,
        WorkspacePhase::NotLoaded
    );
    assert_eq!(fixture.coordinator.record_codex_disconnected().unwrap(), 0);
    fixture.worker.fail_thread_read("thread-1");
    assert_eq!(
        projected_workspace(fixture, workspace).await.phase,
        WorkspacePhase::Unavailable
    );
}

#[tokio::test]
async fn projects_external_tui_turns_from_native_status_without_persisting_them() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let started = CodexEvent::Notification {
        method: "turn/started".to_owned(),
        params: json!({
            "threadId": "thread-1",
            "turn": {"id": "external-turn-1", "status": "inProgress"},
        }),
    };

    let stored_before = fixture
        .store
        .workspace_by_id(&workspace.id)
        .unwrap()
        .unwrap();
    fixture.coordinator.record_codex_event(started).unwrap();
    let stored_after = fixture
        .store
        .workspace_by_id(&workspace.id)
        .unwrap()
        .unwrap();
    assert_eq!(stored_after, stored_before);
    assert!(stored_after.active_turn_id.is_none());

    assert_native_waiting_statuses(&fixture, &workspace).await;

    assert_status_notification_is_not_persisted(&fixture, &workspace, json!({"type": "idle"}));
    fixture
        .worker
        .set_native_status("thread-1", CodexThreadStatus::Idle);
    assert_eq!(
        projected_workspace(&fixture, &workspace).await.phase,
        WorkspacePhase::Idle
    );

    fixture
        .coordinator
        .record_codex_event(CodexEvent::Notification {
            method: "turn/completed".to_owned(),
            params: json!({
                "threadId": "thread-1",
                "turn": {"id": "external-turn-1", "status": "completed"},
            }),
        })
        .unwrap();
    assert_eq!(
        fixture
            .store
            .workspace_by_id(&workspace.id)
            .unwrap()
            .unwrap(),
        stored_before
    );
    assert_native_nonactive_statuses(&fixture, &workspace).await;
}

#[tokio::test]
async fn does_not_persist_native_status_plans_diffs_errors_or_unsupported_requests() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let before = fixture.store.events_after(Some(&workspace.id), 0).unwrap();

    for (method, params) in [
        (
            "thread/status/changed",
            json!({"threadId": "thread-1", "status": {"type": "active", "activeFlags": []}}),
        ),
        (
            "turn/plan/updated",
            json!({"threadId": "thread-1", "plan": [{"step": "private plan"}]}),
        ),
        (
            "turn/diff/updated",
            json!({"threadId": "thread-1", "diff": "private diff"}),
        ),
        (
            "error",
            json!({"threadId": "thread-1", "message": "private native error"}),
        ),
    ] {
        fixture
            .coordinator
            .record_codex_event(CodexEvent::Notification {
                method: method.to_owned(),
                params,
            })
            .unwrap();
    }
    fixture
        .coordinator
        .record_codex_event(CodexEvent::ServerRequest {
            id: json!(91),
            method: "future/request".to_owned(),
            params: json!({"threadId": "thread-1", "secret": "private request"}),
        })
        .unwrap();

    assert_eq!(
        fixture.store.events_after(Some(&workspace.id), 0).unwrap(),
        before
    );
}
