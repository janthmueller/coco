use super::*;
use std::time::Duration;

fn recent(scope: RepositoryScope) -> WorkspacePathListParams {
    WorkspacePathListParams {
        scope,
        include_recency: true,
    }
}

#[tokio::test]
async fn optional_recency_is_passive_validated_and_never_persisted_or_required_for_navigation() {
    let fixture = Fixture::new(FakeWorker::default());
    let mut params = fixture.create_params();
    params.temporary = Some(crate::domain::TemporaryWorkspacePolicy { keep_thread: true });
    let workspace = fixture.create_and_materialize(params).await;
    let thread_id = workspace.codex_thread_id.as_ref().unwrap();
    let mut native = fixture.worker.native_threads.lock().unwrap()[thread_id].clone();
    native.recency_at_ms = Some(1_800_000_000_000);
    native.status = CodexThreadStatus::NotLoaded;
    let before = fixture
        .store
        .workspace_by_id(&workspace.id)
        .unwrap()
        .unwrap();
    for failure in [
        "none", "read", "id", "cwd", "missing", "no-date", "negative", "overflow",
    ] {
        fixture.worker.failed_thread_reads.lock().unwrap().clear();
        fixture.worker.remember_native_thread(native.clone());
        match failure {
            "none" => {}
            "read" => fixture.worker.fail_thread_read(thread_id),
            "missing" => {
                fixture
                    .worker
                    .native_threads
                    .lock()
                    .unwrap()
                    .remove(thread_id);
            }
            _ => {
                let mut threads = fixture.worker.native_threads.lock().unwrap();
                let invalid = threads.get_mut(thread_id).unwrap();
                match failure {
                    "id" => invalid.id = "wrong-thread".into(),
                    "cwd" => invalid.cwd = "/wrong-worktree".into(),
                    "no-date" => invalid.recency_at_ms = None,
                    "negative" => invalid.recency_at_ms = Some(-1),
                    "overflow" => invalid.recency_at_ms = Some(i64::MAX),
                    _ => unreachable!(),
                }
            }
        }
        fixture.worker.calls.lock().unwrap().clear();
        let rows = fixture
            .coordinator
            .list_workspace_paths(recent(RepositoryScope::AllRepositories))
            .await
            .unwrap();
        assert_eq!(rows.len(), 1, "{failure}");
        assert_eq!(rows[0].workspace_id, workspace.id);
        assert_eq!(
            rows[0].recency_at_ms,
            if failure == "none" {
                native.recency_at_ms
            } else {
                None
            },
            "{failure}"
        );
        assert_eq!(rows[0].path, *workspace.worktree_path.as_ref().unwrap());
        assert_eq!(
            fixture
                .coordinator
                .workspace_path(lookup(&fixture, &workspace.id))
                .unwrap(),
            rows[0].path
        );
        assert_eq!(
            fixture.worker.calls(),
            [WorkerCall::Read {
                thread_id: thread_id.clone()
            }]
        );
        assert_eq!(
            fixture
                .store
                .workspace_by_id(&workspace.id)
                .unwrap()
                .unwrap(),
            before
        );
    }
}

#[tokio::test]
async fn optional_recency_only_reads_eligible_bound_workspaces_inside_the_requested_repository() {
    let fixture = Fixture::new(FakeWorker::default());
    let first = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let other = fixture.source.parent().unwrap().join("other");
    run_git(
        fixture.source.parent().unwrap(),
        &[
            "clone",
            "--local",
            fixture.source.to_str().unwrap(),
            other.to_str().unwrap(),
        ],
    );
    let mut params = fixture.create_params();
    params.repository_path = other;
    params.operation_id = "other-repo".to_owned();
    fixture.create_and_materialize(params).await;

    let params = fresh_create_params(fixture.source.clone(), "unbound", "unbound");
    fixture.coordinator.create_workspace(params).await.unwrap();
    let missing = fixture
        .create_and_materialize(fresh_create_params(
            fixture.source.clone(),
            "missing",
            "missing",
        ))
        .await;
    let path = missing.worktree_path.as_ref().unwrap();
    fs::rename(path, path.with_extension("moved")).unwrap();
    fixture.worker.calls.lock().unwrap().clear();

    let rows = fixture
        .coordinator
        .list_workspace_paths(recent(RepositoryScope::repository(fixture.source.clone())))
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .all(|row| row.repository.id == first.repository_id)
    );
    assert_eq!(
        fixture.worker.calls(),
        [WorkerCall::Read {
            thread_id: first.codex_thread_id.unwrap()
        }]
    );
    let error = fixture
        .coordinator
        .list_workspace_paths(recent(RepositoryScope::repository(
            fixture.source.parent().unwrap().join("unregistered"),
        )))
        .await
        .unwrap_err();
    assert_eq!(error.code(), "GIT_ERROR");
    assert_eq!(fixture.worker.calls().len(), 1);
}

#[tokio::test]
async fn stalled_recency_has_one_total_budget_bounded_concurrency_and_no_background_retries() {
    let release = Arc::new(Notify::new());
    let fixture = Fixture::new(FakeWorker::paused_thread_read(
        Arc::new(Notify::new()),
        release.clone(),
    ));
    for index in 0..9 {
        fixture
            .create_and_materialize(fresh_create_params(
                fixture.source.clone(),
                &format!("work/{index}"),
                &format!("create-{index}"),
            ))
            .await;
    }
    fixture.worker.calls.lock().unwrap().clear();
    let rows = tokio::time::timeout(
        Duration::from_secs(4),
        fixture
            .coordinator
            .list_workspace_paths(recent(RepositoryScope::AllRepositories)),
    )
    .await
    .expect("one total budget, not nine per-row deadlines")
    .unwrap();
    assert_eq!(rows.len(), 9);
    assert!(rows.iter().all(|row| row.recency_at_ms.is_none()));
    let calls = fixture.worker.calls();
    assert_eq!(calls.len(), 4, "at most four passive reads in flight");
    assert!(
        calls
            .iter()
            .all(|call| matches!(call, WorkerCall::Read { .. }))
    );
    release.notify_waiters();
    tokio::task::yield_now().await;
    assert_eq!(fixture.worker.calls(), calls);
    for row in rows {
        assert_eq!(
            fixture
                .coordinator
                .workspace_path(WorkspacePathGetParams {
                    scope: RepositoryScope::AllRepositories,
                    workspace: row.workspace_id
                })
                .unwrap(),
            row.path
        );
    }
}

#[tokio::test]
async fn a_finished_read_is_retained_when_another_candidate_exhausts_the_budget() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let fixture = Fixture::new(FakeWorker::paused_thread_read(
        entered.clone(),
        release.clone(),
    ));
    let mut workspaces = Vec::new();
    for (name, time) in [
        ("work/2", 1_800_000_000_000),
        ("work/10", 1_800_000_300_000),
    ] {
        let workspace = fixture
            .create_and_materialize(fresh_create_params(
                fixture.source.clone(),
                name,
                &format!("create-{name}"),
            ))
            .await;
        fixture
            .worker
            .native_threads
            .lock()
            .unwrap()
            .get_mut(workspace.codex_thread_id.as_ref().unwrap())
            .unwrap()
            .recency_at_ms = Some(time);
        workspaces.push(workspace);
    }
    fixture.worker.calls.lock().unwrap().clear();
    let list = fixture
        .coordinator
        .list_workspace_paths(recent(RepositoryScope::AllRepositories));
    tokio::pin!(list);
    tokio::select! {
        _ = entered.notified() => {},
        done = &mut list => panic!("reads did not pause: {done:?}"),
    }
    let calls = fixture.worker.calls();
    assert_eq!(calls.len(), 2);
    let WorkerCall::Read { thread_id } = &calls[0] else {
        panic!("not a passive read")
    };
    let completed = workspaces
        .iter()
        .find(|workspace| workspace.codex_thread_id.as_ref() == Some(thread_id))
        .unwrap();
    let expected = fixture.worker.native_threads.lock().unwrap()[thread_id].recency_at_ms;
    release.notify_one();
    let rows = tokio::time::timeout(Duration::from_secs(4), list)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(rows.len(), 2);
    for row in rows {
        assert_eq!(
            row.recency_at_ms,
            if row.workspace_id == completed.id {
                expected
            } else {
                None
            }
        );
    }
    assert_eq!(fixture.worker.calls(), calls);
}
