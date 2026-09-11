use super::retirement::close_params;
use super::*;

fn native_context_params(repository: PathBuf, source: &Workspace) -> WorkspaceCreateParams {
    let mut params = fresh_create_params(repository, "native-context", "create-native-context");
    params.context = WorkspaceContextRequest::Fork {
        source: WorkspaceContextSource::Thread {
            thread_id: source.codex_thread_id.clone().unwrap(),
        },
        compact: false,
    };
    params
}

fn delete_params(
    fixture: &Fixture,
    workspace: &Workspace,
    delete_thread: bool,
) -> WorkspaceDeleteParams {
    WorkspaceDeleteParams {
        scope: RepositoryScope::repository(fixture.source.clone()),
        workspace: workspace.id.clone(),
        delete_thread,
        delete_branch: false,
        discard_changes: false,
        discard_unretained_commits: false,
        dry_run: false,
        expected_plan: None,
    }
}

#[tokio::test]
async fn retained_native_context_does_not_depend_on_a_deleted_workspace_record() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let source = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let child = fixture
        .coordinator
        .create_workspace(native_context_params(fixture.source.clone(), &source))
        .await
        .unwrap()
        .workspace;
    fixture
        .coordinator
        .close_workspace(close_params(&fixture, &source))
        .await
        .unwrap();
    assert!(matches!(
        fixture
            .coordinator
            .delete_workspace(delete_params(&fixture, &source, true))
            .await,
        Err(CoordinatorError::WorkspaceRetirementBlocked(_))
    ));
    fixture
        .coordinator
        .delete_workspace(delete_params(&fixture, &source, false))
        .await
        .unwrap();
    fixture
        .coordinator
        .materialize_workspace_thread(child)
        .await
        .unwrap();
}

#[tokio::test]
async fn cross_repository_context_creation_cannot_race_source_thread_deletion() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let fixture = Fixture::new(FakeWorker::paused_thread_read(
        entered.clone(),
        release.clone(),
    ));
    fixture.register().await;
    let source = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    fixture
        .coordinator
        .close_workspace(close_params(&fixture, &source))
        .await
        .unwrap();
    let other_repo = fixture.source.parent().unwrap().join("other-repository");
    initialize_repository(&other_repo);
    fixture
        .coordinator
        .register_repository(RepositoryRegisterParams {
            path: other_repo.clone(),
        })
        .unwrap();
    let creation = fixture
        .coordinator
        .create_workspace(native_context_params(other_repo, &source));
    tokio::pin!(creation);
    tokio::select! {
        _ = entered.notified() => {},
        result = &mut creation => panic!("creation bypassed the paused context read: {result:?}"),
    }
    let deletion = fixture
        .coordinator
        .delete_workspace(delete_params(&fixture, &source, true));
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), deletion)
            .await
            .is_err()
    );
    assert!(
        !fixture
            .worker
            .calls()
            .iter()
            .any(|call| matches!(call, WorkerCall::DeleteThread { .. }))
    );
    release.notify_one();
    creation.await.unwrap();
    assert!(matches!(
        fixture
            .coordinator
            .delete_workspace(delete_params(&fixture, &source, true))
            .await,
        Err(CoordinatorError::WorkspaceRetirementBlocked(_))
    ));
    assert!(fixture.store.workspace_by_id(&source.id).unwrap().is_some());
}

#[tokio::test]
async fn record_only_delete_preserves_prepared_context_forks() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let source = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let child = fixture
        .coordinator
        .create_workspace(fixture.fork_params(&source, "prepared-child", false))
        .await
        .unwrap()
        .workspace;
    fixture
        .coordinator
        .close_workspace(close_params(&fixture, &source))
        .await
        .unwrap();
    let deletion = fixture
        .coordinator
        .delete_workspace(WorkspaceDeleteParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: source.id,
            delete_thread: false,
            delete_branch: false,
            discard_changes: false,
            discard_unretained_commits: false,
            dry_run: false,
            expected_plan: None,
        })
        .await;
    assert!(matches!(
        deletion,
        Err(CoordinatorError::WorkspaceRetirementBlocked(_))
    ));
    // The protected source still permits the prepared child to start.
    fixture
        .coordinator
        .materialize_workspace_thread(child)
        .await
        .unwrap();
}

#[tokio::test]
async fn thread_delete_detects_prepared_context_dependants() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let source = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    fixture
        .coordinator
        .create_workspace(fixture.fork_params(&source, "prepared-child", false))
        .await
        .unwrap();
    fixture
        .coordinator
        .close_workspace(close_params(&fixture, &source))
        .await
        .unwrap();
    let deletion = fixture
        .coordinator
        .delete_workspace(WorkspaceDeleteParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: source.id,
            delete_thread: true,
            delete_branch: false,
            discard_changes: false,
            discard_unretained_commits: false,
            dry_run: false,
            expected_plan: None,
        })
        .await;
    assert!(
        deletion.is_err(),
        "thread deletion ignored an unmaterialized context dependant"
    );
}

#[tokio::test]
async fn deletion_recovery_preserves_a_new_prepared_context_dependency() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let source = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let mut params = fixture.fork_params(&source, "prepared-child", false);
    params.worktree = WorkspaceWorktreeRequest::NewBranch {
        branch: None,
        base: WorkspaceBaseRequest::Revision {
            revision: "HEAD".to_owned(),
        },
    };
    fixture
        .coordinator
        .close_workspace(close_params(&fixture, &source))
        .await
        .unwrap();
    fixture
        .store
        .begin_workspace_deletion(
            &source.id,
            WorkspaceDeletionIntent {
                delete_thread: true,
                delete_branch: false,
                ..Default::default()
            },
            None,
        )
        .unwrap();
    // Simulate a dependency present at restart after interrupted deletion.
    fixture.coordinator.create_workspace(params).await.unwrap();
    assert_eq!(fixture.coordinator.recover_workspace_retirements().await, 0);
    assert_eq!(
        fixture
            .store
            .workspace_by_id(&source.id)
            .unwrap()
            .unwrap()
            .availability,
        WorkspaceAvailability::Closed
    );
    assert!(
        fixture
            .worker
            .native_threads
            .lock()
            .unwrap()
            .contains_key(source.codex_thread_id.as_ref().unwrap())
    );
}
