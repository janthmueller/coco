use super::*;

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
