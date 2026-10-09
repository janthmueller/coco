use super::*;

mod recovery;

#[tokio::test]
async fn captures_and_binds_context_during_create_without_a_turn() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let source = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let child = fixture
        .coordinator
        .create_workspace(fixture.fork_params(&source, "captured", false))
        .await
        .unwrap()
        .workspace;
    assert_eq!(child.codex_thread_id.as_deref(), Some("fork-thread-1"));
    assert_eq!(child.parent_thread_id, source.codex_thread_id);
    assert_eq!(child.lifecycle, WorkspaceLifecycle::Ready);
    assert_eq!(child.phase, WorkspacePhase::Idle);
    assert_eq!(
        child.context["resolved"]["context"]["lastTurnId"],
        "thread-1-completed"
    );
    assert!(fixture.worker.calls().iter().any(|call| matches!(call,
        WorkerCall::Fork { last_turn_id, .. } if last_turn_id.as_deref() == Some("thread-1-completed"))));
    assert!(
        !fixture
            .worker
            .calls()
            .iter()
            .any(|call| matches!(call, WorkerCall::Turn { .. }))
    );
}

#[tokio::test]
async fn creation_replay_keeps_a_confirmed_child_even_after_close() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let source = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let params = fixture.fork_params(&source, "captured-replay", false);
    let child = fixture
        .coordinator
        .create_workspace(params.clone())
        .await
        .unwrap()
        .workspace;
    fixture
        .coordinator
        .close_workspace(retirement::close_params(&fixture, &child))
        .await
        .unwrap();
    fixture
        .worker
        .failed_thread_reads
        .lock()
        .unwrap()
        .push(source.codex_thread_id.unwrap());
    let replay = fixture
        .coordinator
        .create_workspace(params)
        .await
        .unwrap()
        .workspace;
    assert_eq!(replay.id, child.id);
    assert_eq!(replay.codex_thread_id, child.codex_thread_id);
    assert_eq!(replay.availability, WorkspaceAvailability::Closed);
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
async fn source_can_work_before_create_and_again_before_child_activation() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let source = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    fixture
        .coordinator
        .start_turn(TurnStartParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: source.id.clone(),
            message: "Continue source work".to_owned(),
            operation_id: "source-working".to_owned(),
        })
        .await
        .unwrap();
    let child = fixture
        .coordinator
        .create_workspace(fixture.fork_params(&source, "during-work", false))
        .await
        .unwrap()
        .workspace;
    fixture
        .worker
        .failed_thread_reads
        .lock()
        .unwrap()
        .push(source.codex_thread_id.clone().unwrap());
    let attached = fixture.attach(&child).await;
    assert_eq!(attached.codex_thread_id, child.codex_thread_id);
    fixture
        .coordinator
        .start_turn(TurnStartParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: child.id,
            message: "Review independently".to_owned(),
            operation_id: "child-working".to_owned(),
        })
        .await
        .unwrap();
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
async fn missing_completed_boundary_rejects_before_creating_any_artifacts() {
    let fixture = Fixture::new(FakeWorker::default());
    let repository = fixture.register().await;
    let source = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    fixture
        .worker
        .context_boundaries
        .lock()
        .unwrap()
        .insert("thread-1".to_owned(), None);
    let error = fixture
        .coordinator
        .create_workspace(fixture.fork_params(&source, "too-early", false))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("no completed turn yet"));
    assert!(
        fixture
            .store
            .workspace_by_name(&repository.id, "too-early")
            .unwrap()
            .is_none()
    );
    assert!(
        !fixture
            .worker
            .calls()
            .iter()
            .any(|call| matches!(call, WorkerCall::Fork { .. }))
    );
    assert!(
        !git_output(&fixture.source, &["branch", "--list", "coco/too-early"]).contains("too-early")
    );
}

#[tokio::test]
async fn captured_child_survives_restart_without_looking_up_source() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let source = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let child = fixture
        .coordinator
        .create_workspace(fixture.fork_params(&source, "restart-child", false))
        .await
        .unwrap()
        .workspace;
    fixture.store.reconcile_unfinished().unwrap();
    fixture
        .worker
        .remember_bound_thread(&child, CodexThreadStatus::NotLoaded);
    fixture
        .worker
        .failed_thread_reads
        .lock()
        .unwrap()
        .push("thread-1".to_owned());
    let restarted = fixture.recovery_coordinator(fixture.worker.clone(), "restarted-capture");
    let attached = restarted
        .attach_workspace(WorkspaceAttachParams {
            client: None,
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: child.id,
        })
        .await
        .unwrap();
    assert_eq!(
        attached.workspace.codex_thread_id.as_deref(),
        Some("fork-thread-1")
    );
    assert!(matches!(
        attached.launch,
        WorkspaceAttachLaunch::Resume { .. }
    ));
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
async fn failed_fork_is_not_replayed_or_lazily_recaptured() {
    let fixture = Fixture::new(FakeWorker {
        fail_fork: true,
        ..FakeWorker::default()
    });
    let repository = fixture.register().await;
    let source = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let params = fixture.fork_params(&source, "unconfirmed", false);
    assert!(
        fixture
            .coordinator
            .create_workspace(params.clone())
            .await
            .is_err()
    );
    let child = fixture
        .store
        .workspace_by_name(&repository.id, &params.name)
        .unwrap()
        .unwrap();
    assert_eq!(child.lifecycle, WorkspaceLifecycle::Failed);
    assert!(child.worktree_path.as_deref().unwrap().exists());
    assert!(fixture.coordinator.create_workspace(params).await.is_err());
    assert!(
        fixture
            .coordinator
            .materialize_workspace_thread(child)
            .await
            .is_err()
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
async fn pending_child_compaction_survives_restart_and_runs_only_once() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let source = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let child = fixture
        .coordinator
        .create_workspace(fixture.fork_params(&source, "pending-compact", true))
        .await
        .unwrap()
        .workspace;
    assert_eq!(
        child.context["resolved"]["context"]["compactionPending"],
        true
    );
    assert!(
        !fixture
            .worker
            .calls()
            .iter()
            .any(|call| matches!(call, WorkerCall::Compact { .. }))
    );
    fixture.store.reconcile_unfinished().unwrap();
    fixture
        .worker
        .remember_bound_thread(&child, CodexThreadStatus::NotLoaded);
    let restarted = fixture.recovery_coordinator(fixture.worker.clone(), "restarted-compaction");
    let attached = restarted.attach_workspace(WorkspaceAttachParams {
        client: None,
        scope: RepositoryScope::repository(fixture.source.clone()),
        workspace: child.id.clone(),
    });
    let completion = async {
        context::wait_for_compaction_request(&fixture).await;
        for (method, params) in [
            (
                "turn/started",
                json!({"threadId": "fork-thread-1", "turn": {"id": "compact", "status": "inProgress"}}),
            ),
            (
                "item/completed",
                json!({"threadId": "fork-thread-1", "turnId": "compact", "item": {"type": "contextCompaction"}}),
            ),
            (
                "turn/completed",
                json!({"threadId": "fork-thread-1", "turn": {"id": "compact", "status": "completed"}}),
            ),
        ] {
            assert!(restarted.observe_pending_compaction(method, &params));
        }
    };
    let (attached, ()) = tokio::join!(attached, completion);
    let attached = attached.unwrap();
    assert_eq!(
        attached.workspace.context["resolved"]["context"]["compactionPending"],
        false
    );
    restarted
        .attach_workspace(WorkspaceAttachParams {
            client: None,
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: child.id,
        })
        .await
        .unwrap();
    assert_eq!(
        fixture
            .worker
            .calls()
            .iter()
            .filter(|call| matches!(call, WorkerCall::Compact { .. }))
            .count(),
        1
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

/// Represents a workspace created by the previous deferred-fork implementation.
/// This deliberately bypasses the new create contract, rather than weakening it.
pub(super) fn legacy_context_workspace(
    fixture: &Fixture,
    source: &Workspace,
    name: &str,
) -> Workspace {
    let repository = fixture.coordinator.git.discover(&fixture.source).unwrap();
    let base_sha = source.base_sha.as_deref().unwrap();
    let plan = fixture
        .coordinator
        .git
        .plan_worktree(
            &repository,
            &fixture.worktrees,
            name,
            crate::git::WorktreeTarget::NewBranch {
                branch_name: format!("coco/{name}"),
            },
            base_sha,
        )
        .unwrap();
    let (workspace, _) = fixture.store.create_workspace_with_event(crate::store::NewWorkspace {
        create_operation_id: Some(format!("legacy-create-{name}")), repository_id: repository.id.clone(),
        name: name.to_owned(), context_mode: ContextMode::Fork,
        context: json!({"version": 3, "resolved": {"context": {
            "mode": "fork", "compact": false, "source": {
                "kind": "workspace", "requestedReference": source.name, "workspaceId": source.id,
                "workspaceName": source.name, "threadId": source.codex_thread_id,
                "cwd": source.worktree_path,
            },
        }}}), profile: source.profile.clone(), worktree_mode: plan.mode,
        branch_name: plan.branch_name.clone(), base_sha: Some(base_sha.to_owned()), worktree_path: Some(plan.path.clone()),
    }, crate::store::EventDraft::workspace(EventKind::WorkspaceCreated, EventSource::Coco, json!({}))).unwrap();
    fixture
        .coordinator
        .git
        .create_worktree(&repository, &plan)
        .unwrap();
    fixture
        .store
        .transition_workspace_lifecycle_with_event(
            &workspace.id,
            WorkspaceLifecycle::Provisioning,
            WorkspaceLifecycle::Ready,
            None,
            crate::store::EventDraft::workspace(
                EventKind::WorktreeCreated,
                EventSource::Git,
                json!({}),
            ),
        )
        .unwrap()
        .0
}
