use super::*;

fn closed_list_params(fixture: &Fixture) -> WorkspaceListParams {
    WorkspaceListParams {
        scope: RepositoryScope::repository(fixture.source.clone()),
        phases: Some(vec!["closed".to_owned()]),
        include_resources: false,
        include_activity: false,
        include_clients: false,
    }
}

#[tokio::test]
async fn closed_list_carries_native_recency_without_reopening_or_unarchiving() {
    for archive in [false, true] {
        let fixture = Fixture::new(FakeWorker::default());
        fixture.register().await;
        let workspace = fixture
            .create_and_materialize(fixture.create_params())
            .await;
        let thread_id = workspace.codex_thread_id.as_ref().unwrap();
        let native_time = 1_800_000_000_000;
        let mut native = fixture.worker.native_threads.lock().unwrap()[thread_id].clone();
        native.recency_at_ms = Some(native_time);
        native.status = CodexThreadStatus::NotLoaded;
        fixture.worker.remember_native_thread(native);
        let mut params = super::retirement::close_params(&fixture, &workspace);
        params.archive_thread = archive;
        let closed = fixture
            .coordinator
            .close_workspace(params)
            .await
            .unwrap()
            .workspace;
        let before = fixture.worker.calls();
        for phases in [None, Some(vec!["idle".to_owned()])] {
            let mut params = closed_list_params(&fixture);
            params.phases = phases;
            assert!(
                fixture
                    .coordinator
                    .list_workspaces(params)
                    .await
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(fixture.worker.calls(), before);
        }
        let listed = fixture
            .coordinator
            .list_workspaces(closed_list_params(&fixture))
            .await
            .unwrap();
        assert_eq!(listed.len(), 1);
        let shown = &listed[0].workspace;
        assert_eq!(shown.id, closed.id);
        assert_eq!(shown.phase, WorkspacePhase::Closed);
        assert_eq!(shown.availability, WorkspaceAvailability::Closed);
        assert_eq!(shown.thread_archived, archive);
        let runtime = shown
            .thread_runtime
            .as_ref()
            .expect("closed native metadata missing");
        assert_eq!(runtime.recency_at_ms, Some(native_time));
        assert_eq!(runtime.status, CodexThreadStatus::NotLoaded);
        assert_eq!(shown.updated_at_ms, closed.updated_at_ms);
        assert!(!shown.worktree_path.as_ref().unwrap().exists());
        assert_eq!(
            fixture
                .worker
                .archived_threads
                .lock()
                .unwrap()
                .contains(thread_id),
            archive
        );
        assert_eq!(
            fixture.worker.calls()[before.len()..],
            [WorkerCall::Read {
                thread_id: thread_id.clone()
            }]
        );
        let stored = fixture.store.workspace_by_id(&shown.id).unwrap().unwrap();
        assert_eq!(stored.availability, WorkspaceAvailability::Closed);
        assert_eq!(stored.updated_at_ms, closed.updated_at_ms);
        assert!(
            stored
                .thread_runtime
                .as_ref()
                .is_none_or(|runtime| runtime.recency_at_ms.is_none())
        );
    }
}

#[tokio::test]
async fn closed_list_keeps_missing_or_invalid_native_metadata_unknown_and_stays_closed() {
    for failure in ["unbound", "read", "id", "cwd", "missing", "no-date"] {
        let fixture = Fixture::new(FakeWorker::default());
        fixture.register().await;
        let workspace = if failure == "unbound" {
            fixture
                .coordinator
                .create_workspace(fixture.create_params())
                .await
                .unwrap()
                .workspace
        } else {
            fixture
                .create_and_materialize(fixture.create_params())
                .await
        };
        let params = super::retirement::close_params(&fixture, &workspace);
        let closed = fixture
            .coordinator
            .close_workspace(params)
            .await
            .unwrap()
            .workspace;
        if let Some(thread_id) = &closed.codex_thread_id {
            match failure {
                "read" => fixture.worker.fail_thread_read(thread_id),
                "missing" => {
                    fixture
                        .worker
                        .native_threads
                        .lock()
                        .unwrap()
                        .remove(thread_id);
                }
                "id" => {
                    fixture
                        .worker
                        .native_threads
                        .lock()
                        .unwrap()
                        .get_mut(thread_id)
                        .unwrap()
                        .id = "wrong-thread".into()
                }
                "cwd" => {
                    fixture
                        .worker
                        .native_threads
                        .lock()
                        .unwrap()
                        .get_mut(thread_id)
                        .unwrap()
                        .cwd = "/wrong-worktree".into()
                }
                "no-date" => {}
                _ => unreachable!(),
            }
        }
        let before = fixture.worker.calls();
        let listed = fixture
            .coordinator
            .list_workspaces(closed_list_params(&fixture))
            .await
            .unwrap();
        assert_eq!(listed.len(), 1, "{failure}");
        let shown = &listed[0].workspace;
        assert_eq!(shown.id, closed.id);
        assert_eq!(shown.phase, WorkspacePhase::Closed, "{failure}");
        assert_eq!(shown.availability, WorkspaceAvailability::Closed);
        assert!(!shown.worktree_path.as_ref().unwrap().exists());
        assert!(
            shown
                .thread_runtime
                .as_ref()
                .is_none_or(|runtime| runtime.recency_at_ms.is_none())
        );
        if failure != "no-date" {
            assert!(shown.thread_runtime.is_none(), "{failure}");
        }
        let calls = &fixture.worker.calls()[before.len()..];
        match closed.codex_thread_id {
            Some(thread_id) => assert_eq!(calls, [WorkerCall::Read { thread_id }]),
            None => assert!(calls.is_empty()),
        }
    }
}

#[tokio::test]
async fn passive_list_carries_native_recency_without_loading_or_replacing_it_with_updates() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let thread_id = workspace.codex_thread_id.as_ref().unwrap();
    let native_time = 1_800_000_000_000;
    let mut native = fixture
        .worker
        .native_threads
        .lock()
        .unwrap()
        .get(thread_id)
        .unwrap()
        .clone();
    native.recency_at_ms = Some(native_time);
    native.status = CodexThreadStatus::NotLoaded;
    fixture.worker.remember_native_thread(native);
    let before = fixture.worker.calls();
    let params = WorkspaceListParams {
        scope: RepositoryScope::repository(fixture.source.clone()),
        phases: None,
        include_resources: false,
        include_activity: false,
        include_clients: false,
    };
    let listed = fixture
        .coordinator
        .list_workspaces(params.clone())
        .await
        .unwrap();
    let snapshot = listed[0].workspace.thread_runtime.as_ref().unwrap();
    assert_eq!(snapshot.recency_at_ms, Some(native_time));
    assert_eq!(snapshot.status, CodexThreadStatus::NotLoaded);
    assert_eq!(listed[0].workspace.updated_at_ms, workspace.updated_at_ms);
    assert_eq!(
        fixture.worker.calls()[before.len()..],
        [WorkerCall::Read {
            thread_id: thread_id.clone()
        }]
    );
    let stored = fixture
        .store
        .workspace_by_id(&workspace.id)
        .unwrap()
        .unwrap();
    assert!(
        stored
            .thread_runtime
            .as_ref()
            .is_none_or(|runtime| runtime.recency_at_ms.is_none())
    );
    assert_eq!(
        serde_json::to_value(snapshot).unwrap()["recencyAtMs"],
        native_time
    );
}
