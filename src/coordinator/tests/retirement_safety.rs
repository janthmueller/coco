use super::retirement::close_params;
use super::*;

#[cfg(unix)]
#[tokio::test]
async fn close_must_not_follow_a_replaced_managed_path() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let mut params = fixture.create_params();
    params.worktree = WorkspaceWorktreeRequest::Detached {
        base: WorkspaceBaseRequest::Revision {
            revision: "HEAD".to_owned(),
        },
    };
    let workspace = fixture
        .coordinator
        .create_workspace(params)
        .await
        .unwrap()
        .workspace;
    let managed_path = workspace.worktree_path.as_ref().unwrap();
    let moved_path = fixture.source.parent().unwrap().join("moved-original");
    let unrelated_path = fixture.source.parent().unwrap().join("unrelated-detached");
    run_git(
        &fixture.source,
        &[
            "worktree",
            "move",
            managed_path.to_str().unwrap(),
            moved_path.to_str().unwrap(),
        ],
    );
    run_git(
        &fixture.source,
        &[
            "worktree",
            "add",
            "--detach",
            unrelated_path.to_str().unwrap(),
            "HEAD",
        ],
    );
    std::os::unix::fs::symlink(&unrelated_path, managed_path).unwrap();

    let result = fixture
        .coordinator
        .close_workspace(close_params(&fixture, &workspace))
        .await;
    assert!(
        result.is_err() && unrelated_path.is_dir(),
        "close accepted a replaced path: success={}, unrelated worktree exists={}",
        result.is_ok(),
        unrelated_path.is_dir()
    );
}

#[tokio::test]
async fn send_remains_available_with_an_idle_resumed_tui() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let first = fixture
        .coordinator
        .attach_workspace(WorkspaceAttachParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id.clone(),
        })
        .await
        .unwrap();
    let second = fixture
        .coordinator
        .attach_workspace(WorkspaceAttachParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id.clone(),
        })
        .await
        .unwrap();
    let WorkspaceAttachLaunch::Resume {
        lease_id: first_id, ..
    } = first.launch
    else {
        panic!("expected resume");
    };
    let WorkspaceAttachLaunch::Resume {
        lease_id: second_id,
        ..
    } = second.launch
    else {
        panic!("expected resume");
    };
    assert_ne!(first_id, second_id);
    fixture
        .coordinator
        .release_workspace_attach(WorkspaceAttachReleaseParams {
            workspace_id: workspace.id.clone(),
            lease_id: first_id,
        })
        .unwrap();
    fixture
        .coordinator
        .renew_workspace_attach(crate::protocol::WorkspaceAttachRenewParams {
            workspace_id: workspace.id.clone(),
            lease_id: second_id,
        })
        .unwrap();
    let preview = fixture
        .coordinator
        .close_workspace(WorkspaceCloseParams {
            dry_run: true,
            ..close_params(&fixture, &workspace)
        })
        .await
        .unwrap();
    assert!(
        preview
            .plan
            .blockers
            .iter()
            .any(|blocker| blocker.contains("terminal UI"))
    );

    let result = fixture
        .coordinator
        .start_turn(TurnStartParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id,
            message: "An instruction from another terminal".to_owned(),
            operation_id: "review-send-with-attached-tui".to_owned(),
        })
        .await;
    assert!(
        result.is_ok(),
        "send rejected an idle bound thread with a TUI: {result:?}"
    );
}

#[tokio::test]
async fn close_preserves_worktree_used_by_an_active_child_agent() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let parent_id = workspace.codex_thread_id.as_deref().unwrap();
    fixture.worker.set_descendants(parent_id, &["active-child"]);
    fixture.worker.remember_materialized_thread(NativeThread {
        id: "active-child".to_owned(),
        cwd: workspace.worktree_path.clone().unwrap(),
        name: None,
        status: CodexThreadStatus::Active {
            active_flags: Vec::new(),
        },
        forked_from_id: Some(parent_id.to_owned()),
    });

    let result = fixture
        .coordinator
        .close_workspace(close_params(&fixture, &workspace))
        .await;
    let path_exists = workspace.worktree_path.as_ref().unwrap().is_dir();
    assert!(
        result.is_err() && path_exists,
        "close ignored a running child: success={}, worktree exists={}",
        result.is_ok(),
        path_exists
    );
}

#[tokio::test]
async fn recovery_must_check_descendants_before_rearchiving() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let mut params = close_params(&fixture, &workspace);
    params.archive_thread = true;
    let closed = fixture
        .coordinator
        .close_workspace(params)
        .await
        .unwrap()
        .workspace;
    fixture
        .store
        .transition_workspace_availability(
            &closed.id,
            WorkspaceAvailability::Closed,
            WorkspaceAvailability::Reopening,
            None,
        )
        .unwrap();
    let thread_id = workspace.codex_thread_id.as_deref().unwrap();
    fixture.worker.unarchive_thread(thread_id).await.unwrap();
    fixture
        .worker
        .set_descendants(thread_id, &["new-external-child"]);
    fixture.worker.calls.lock().unwrap().clear();

    fixture.coordinator.recover_workspace_retirements().await;
    assert!(
        !fixture
            .worker
            .calls()
            .iter()
            .any(|call| matches!(call, WorkerCall::Archive { .. })),
        "recovery rearchived a thread with unchecked descendants: {:?}",
        fixture.worker.calls()
    );
}

fn remember_child(
    fixture: &Fixture,
    workspace: &Workspace,
    cwd: PathBuf,
    status: CodexThreadStatus,
) {
    let parent_id = workspace.codex_thread_id.as_deref().unwrap();
    fixture.worker.set_descendants(parent_id, &["child-thread"]);
    fixture.worker.remember_materialized_thread(NativeThread {
        id: "child-thread".to_owned(),
        cwd,
        name: None,
        status,
        forked_from_id: Some(parent_id.to_owned()),
    });
}

#[tokio::test]
async fn ordinary_close_distinguishes_idle_children_and_other_worktrees() {
    for (same_worktree, background_terminals, status, blocked) in [
        (true, 0, CodexThreadStatus::Idle, false),
        (true, 1, CodexThreadStatus::Idle, true),
        (
            false,
            1,
            CodexThreadStatus::Active {
                active_flags: Vec::new(),
            },
            false,
        ),
        (true, 0, CodexThreadStatus::SystemError, true),
    ] {
        let fixture = Fixture::new(FakeWorker::default());
        fixture.register().await;
        let workspace = fixture
            .create_and_materialize(fixture.create_params())
            .await;
        let cwd = if same_worktree {
            workspace.worktree_path.clone().unwrap()
        } else {
            fixture.source.clone()
        };
        remember_child(&fixture, &workspace, cwd, status);
        fixture
            .worker
            .set_background_terminals("child-thread", background_terminals);
        let outcome = fixture
            .coordinator
            .close_workspace(close_params(&fixture, &workspace))
            .await;
        assert_eq!(
            outcome.is_err(),
            blocked,
            "unexpected close outcome: {outcome:?}"
        );
        assert_eq!(workspace.worktree_path.as_ref().unwrap().is_dir(), blocked);
    }
}

#[tokio::test]
async fn child_starting_after_the_plan_still_blocks_worktree_removal() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    remember_child(
        &fixture,
        &workspace,
        workspace.worktree_path.clone().unwrap(),
        CodexThreadStatus::Idle,
    );
    *fixture.worker.activate_on_unsubscribe.lock().unwrap() = Some("child-thread".to_owned());
    let outcome = fixture
        .coordinator
        .close_workspace(close_params(&fixture, &workspace))
        .await;
    assert!(matches!(
        outcome,
        Err(CoordinatorError::WorkspaceRetirementBlocked(_))
    ));
    assert!(workspace.worktree_path.as_ref().unwrap().is_dir());
    assert_eq!(
        fixture
            .store
            .workspace_by_id(&workspace.id)
            .unwrap()
            .unwrap()
            .availability,
        WorkspaceAvailability::Open
    );
}

#[tokio::test]
async fn recovery_leaves_newly_active_threads_unarchived() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let mut close = close_params(&fixture, &workspace);
    close.archive_thread = true;
    fixture.coordinator.close_workspace(close).await.unwrap();
    fixture
        .store
        .transition_workspace_availability(
            &workspace.id,
            WorkspaceAvailability::Closed,
            WorkspaceAvailability::Reopening,
            None,
        )
        .unwrap();
    let thread_id = workspace.codex_thread_id.as_deref().unwrap();
    fixture.worker.unarchive_thread(thread_id).await.unwrap();
    fixture.worker.set_native_status(
        thread_id,
        CodexThreadStatus::Active {
            active_flags: Vec::new(),
        },
    );
    fixture.worker.calls.lock().unwrap().clear();
    assert_eq!(fixture.coordinator.recover_workspace_retirements().await, 0);
    assert!(
        !fixture
            .worker
            .calls()
            .iter()
            .any(|call| matches!(call, WorkerCall::Archive { .. }))
    );
    assert_eq!(
        fixture
            .store
            .workspace_by_id(&workspace.id)
            .unwrap()
            .unwrap()
            .availability,
        WorkspaceAvailability::Reopening
    );
}
