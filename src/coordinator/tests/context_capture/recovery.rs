use super::*;

#[tokio::test]
async fn crash_after_native_fork_before_binding_never_leaves_a_prepared_child() {
    let entered = Arc::new(Notify::new());
    let fixture = Fixture::new(FakeWorker::paused_fork(
        entered.clone(),
        Arc::new(Notify::new()),
    ));
    let repository = fixture.register().await;
    let source = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let params = fixture.fork_params(&source, "interrupted-capture", false);
    let mut creating = Box::pin(fixture.coordinator.create_workspace(params.clone()));
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        tokio::select! {
            () = entered.notified() => {},
            result = &mut creating => panic!("capture did not pause: {result:?}"),
        }
    })
    .await
    .expect("native fork never accepted");
    let child = fixture
        .store
        .workspace_by_name(&repository.id, &params.name)
        .unwrap()
        .unwrap();
    assert_eq!(child.lifecycle, WorkspaceLifecycle::Starting);
    assert!(child.codex_thread_id.is_none());
    assert!(child.worktree_path.as_deref().unwrap().exists());
    assert!(
        fixture
            .worker
            .native_threads
            .lock()
            .unwrap()
            .contains_key("fork-thread-1")
    );
    drop(creating);
    assert_eq!(
        fixture
            .store
            .reconcile_unfinished()
            .unwrap()
            .failed_workspace_preparations,
        1
    );
    let child = fixture.store.workspace_by_id(&child.id).unwrap().unwrap();
    assert_eq!(child.lifecycle, WorkspaceLifecycle::Failed);
    assert_eq!(child.last_error_code.as_deref(), Some("DAEMON_RESTART"));
    let restarted = fixture.recovery_coordinator(fixture.worker.clone(), "capture-restart");
    assert!(restarted.create_workspace(params).await.is_err());
    assert!(
        restarted
            .attach_workspace(attach_params(&fixture, &child))
            .await
            .is_err()
    );
    assert_eq!(
        count_calls(&fixture, |call| matches!(call, WorkerCall::Fork { .. })),
        1
    );
}

#[tokio::test]
async fn lost_reply_after_accepted_fork_does_not_repeat_the_native_side_effect() {
    let fixture = Fixture::new(FakeWorker {
        fail_fork_after_creation: true,
        ..FakeWorker::default()
    });
    let repository = fixture.register().await;
    let source = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let params = fixture.fork_params(&source, "lost-fork-reply", false);
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
    assert!(child.codex_thread_id.is_none());
    assert!(child.worktree_path.as_deref().unwrap().exists());
    assert!(
        fixture
            .worker
            .native_threads
            .lock()
            .unwrap()
            .contains_key("fork-thread-1")
    );
    assert_eq!(
        fixture
            .store
            .reconcile_unfinished()
            .unwrap()
            .failed_workspace_preparations,
        0
    );
    let restarted = fixture.recovery_coordinator(fixture.worker.clone(), "reply-restart");
    assert!(restarted.create_workspace(params).await.is_err());
    assert!(
        restarted
            .attach_workspace(attach_params(&fixture, &child))
            .await
            .is_err()
    );
    assert_eq!(
        count_calls(&fixture, |call| matches!(call, WorkerCall::Fork { .. })),
        1
    );
}

#[tokio::test]
async fn restart_during_compaction_preserves_binding_but_does_not_repeat_the_turn() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let source = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let params = fixture.fork_params(&source, "interrupted-compaction", true);
    let child = fixture
        .coordinator
        .create_workspace(params.clone())
        .await
        .unwrap()
        .workspace;
    let mut activating = Box::pin(
        fixture
            .coordinator
            .attach_workspace(attach_params(&fixture, &child)),
    );
    tokio::select! {
        () = context::wait_for_compaction_request(&fixture) => {},
        result = &mut activating => panic!("compaction did not wait: {result:?}"),
    }
    let pending = fixture.store.workspace_by_id(&child.id).unwrap().unwrap();
    assert_eq!(pending.lifecycle, WorkspaceLifecycle::Starting);
    assert_eq!(pending.codex_thread_id, child.codex_thread_id);
    assert_eq!(
        pending.context["resolved"]["context"]["compactionPending"],
        true
    );
    drop(activating);
    assert_eq!(
        fixture
            .store
            .reconcile_unfinished()
            .unwrap()
            .failed_workspace_preparations,
        1
    );
    let failed = fixture.store.workspace_by_id(&child.id).unwrap().unwrap();
    assert_eq!(failed.lifecycle, WorkspaceLifecycle::Failed);
    assert_eq!(failed.codex_thread_id, child.codex_thread_id);
    assert_eq!(
        failed.context["resolved"]["context"]["compactionPending"],
        true
    );
    let restarted = fixture.recovery_coordinator(fixture.worker.clone(), "compact-restart");
    assert!(
        restarted
            .attach_workspace(attach_params(&fixture, &child))
            .await
            .is_err()
    );
    let replay = restarted.create_workspace(params).await.unwrap().workspace;
    assert_eq!(replay.lifecycle, WorkspaceLifecycle::Failed);
    assert_eq!(replay.codex_thread_id, child.codex_thread_id);
    assert_eq!(
        count_calls(&fixture, |call| matches!(call, WorkerCall::Compact { .. })),
        1
    );
    assert_eq!(
        count_calls(&fixture, |call| matches!(call, WorkerCall::Fork { .. })),
        1
    );
}

fn attach_params(fixture: &Fixture, workspace: &Workspace) -> WorkspaceAttachParams {
    WorkspaceAttachParams {
        client: None,
        scope: RepositoryScope::repository(fixture.source.clone()),
        workspace: workspace.id.clone(),
    }
}

fn count_calls(fixture: &Fixture, predicate: impl Fn(&WorkerCall) -> bool) -> usize {
    fixture
        .worker
        .calls()
        .iter()
        .filter(|call| predicate(call))
        .count()
}
