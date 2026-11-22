use super::*;
use crate::protocol::WorkspaceStatuslineParams;

fn query(repository: &Repository, workspace: &Workspace) -> WorkspaceStatuslineParams {
    WorkspaceStatuslineParams {
        repository_id: repository.id.clone(),
        thread_id: workspace.codex_thread_id.clone().unwrap(),
    }
}

#[tokio::test]
async fn passive_footer_reads_only_the_exact_bound_workspace_and_retains_all_stored_state() {
    let fixture = Fixture::new(FakeWorker::default());
    let repository = fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let resources: WorkspaceRuntimeResources = serde_json::from_value(serde_json::json!({
        "backend":"exec_server", "state":"running", "scope":"cgroup_v2", "memoryCurrentBytes":25165824, "cpuPercent":12.3,
    })).unwrap();
    fixture.worker.set_runtime_resources(resources.clone());
    fixture.worker.calls.lock().unwrap().clear();
    let saved = fixture.store.workspace_by_id(&workspace.id).unwrap();
    let events = fixture.store.events_after(None, 0).unwrap();
    let audits = fixture.store.audits_after(None, 0).unwrap();
    let shown = fixture
        .coordinator
        .workspace_statusline(query(&repository, &workspace))
        .await
        .unwrap()
        .workspace
        .unwrap();
    assert_eq!(shown.workspace_id, workspace.id);
    assert_eq!(shown.name, workspace.name);
    assert_eq!(shown.resources, Some(resources));
    assert_eq!(
        fixture.worker.calls(),
        [WorkerCall::Resources {
            workspace_id: workspace.id.clone()
        }]
    );
    assert_eq!(fixture.store.workspace_by_id(&workspace.id).unwrap(), saved);
    assert_eq!(fixture.store.events_after(None, 0).unwrap(), events);
    assert_eq!(fixture.store.audits_after(None, 0).unwrap(), audits);
}

#[tokio::test]
async fn foreign_unknown_prepared_and_closed_bindings_never_observe_or_start_a_runtime() {
    let fixture = Fixture::new(FakeWorker::default());
    let repository = fixture.register().await;
    let prepared = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    assert!(
        fixture
            .coordinator
            .workspace_statusline(WorkspaceStatuslineParams {
                repository_id: repository.id.clone(),
                thread_id: prepared.id.clone(),
            })
            .await
            .unwrap()
            .workspace
            .is_none()
    );
    let bound = fixture
        .coordinator
        .materialize_workspace_thread(prepared)
        .await
        .unwrap();
    let foreign = Repository {
        id: "foreign".into(),
        root_path: "/other-root".into(),
        git_common_dir: "/other-root/.git".into(),
        ..repository.clone()
    };
    fixture.store.register_repository(&foreign).unwrap();
    fixture.worker.calls.lock().unwrap().clear();
    for params in [
        query(&foreign, &bound),
        WorkspaceStatuslineParams {
            repository_id: "unknown".into(),
            thread_id: bound.codex_thread_id.clone().unwrap(),
        },
        WorkspaceStatuslineParams {
            repository_id: repository.id.clone(),
            thread_id: bound.name.clone(),
        },
    ] {
        assert!(
            fixture
                .coordinator
                .workspace_statusline(params)
                .await
                .unwrap()
                .workspace
                .is_none()
        );
    }
    fixture
        .store
        .transition_workspace_availability(
            &bound.id,
            WorkspaceAvailability::Open,
            WorkspaceAvailability::Closed,
            bound.base_sha.as_deref(),
        )
        .unwrap();
    assert!(
        fixture
            .coordinator
            .workspace_statusline(query(&repository, &bound))
            .await
            .unwrap()
            .workspace
            .is_none()
    );
    assert!(fixture.worker.calls().is_empty());
}

#[tokio::test]
async fn invalid_identity_is_rejected_before_any_worker_read() {
    let fixture = Fixture::new(FakeWorker::default());
    for (repository_id, thread_id) in [
        ("".into(), "thread".into()),
        ("repo".into(), "x".repeat(257)),
        ("repo".into(), "bad\nthread".into()),
    ] {
        assert!(
            fixture
                .coordinator
                .workspace_statusline(WorkspaceStatuslineParams {
                    repository_id,
                    thread_id
                })
                .await
                .is_err()
        );
    }
    assert!(fixture.worker.calls().is_empty());
}

#[tokio::test]
async fn slow_resource_observation_is_optional_and_does_not_delay_the_footer_indefinitely() {
    let fixture = Fixture::new(FakeWorker {
        resource_delay: Some(std::time::Duration::from_secs(30)),
        ..FakeWorker::default()
    });
    let repository = fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    fixture.worker.calls.lock().unwrap().clear();
    let start = std::time::Instant::now();
    let shown = fixture
        .coordinator
        .workspace_statusline(query(&repository, &workspace))
        .await
        .unwrap()
        .workspace
        .unwrap();
    // Optional 200 ms observation must not await the worker's 30-second delay;
    // leave headroom for CI scheduler contention rather than asserting latency.
    assert!(start.elapsed() < std::time::Duration::from_secs(3));
    assert_eq!(shown.workspace_id, workspace.id);
    assert!(shown.resources.is_none());
    assert_eq!(
        fixture.worker.calls(),
        [WorkerCall::Resources {
            workspace_id: workspace.id
        }]
    );
}
