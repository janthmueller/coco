use super::*;
use crate::git::GitError;

fn scope(fixture: &Fixture) -> RepositoryScope {
    RepositoryScope::repository(fixture.source.clone())
}

pub(super) fn close_params(fixture: &Fixture, workspace: &Workspace) -> WorkspaceCloseParams {
    WorkspaceCloseParams {
        scope: scope(fixture),
        workspace: workspace.id.clone(),
        archive_thread: false,
        discard_changes: false,
        dry_run: false,
        expected_plan: None,
    }
}

#[tokio::test]
async fn close_hides_a_workspace_and_reopen_restores_its_exact_identity() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let path = workspace.worktree_path.clone().unwrap();
    let branch = workspace.branch_name.clone().unwrap();

    let closed = fixture
        .coordinator
        .close_workspace(close_params(&fixture, &workspace))
        .await
        .unwrap();
    assert!(closed.applied);
    assert_eq!(closed.workspace.id, workspace.id);
    assert_eq!(closed.workspace.availability, WorkspaceAvailability::Closed);
    assert_eq!(closed.workspace.phase, WorkspacePhase::Closed);
    assert_eq!(
        closed.plan.thread_disposition,
        WorkspaceThreadDisposition::Retain
    );
    assert!(!path.exists());
    assert_eq!(
        git_output(&fixture.source, &["rev-parse", &branch]),
        closed.workspace.closed_head_sha.unwrap()
    );
    assert!(
        fixture
            .worker
            .calls
            .lock()
            .unwrap()
            .contains(&WorkerCall::StopExecution {
                workspace_id: workspace.id.clone(),
            })
    );

    let active = fixture
        .coordinator
        .list_workspaces(WorkspaceListParams {
            scope: scope(&fixture),
            phases: None,
        })
        .await
        .unwrap();
    assert!(active.is_empty());
    let retained = fixture
        .coordinator
        .list_workspaces(WorkspaceListParams {
            scope: scope(&fixture),
            phases: Some(vec!["closed".to_owned()]),
        })
        .await
        .unwrap();
    assert_eq!(retained.len(), 1);

    let reopened = fixture
        .coordinator
        .reopen_workspace(WorkspaceReopenParams {
            scope: scope(&fixture),
            workspace: workspace.id.clone(),
        })
        .await
        .unwrap()
        .workspace;
    assert_eq!(reopened.id, workspace.id);
    assert_eq!(reopened.availability, WorkspaceAvailability::Open);
    assert_eq!(reopened.phase, WorkspacePhase::Prepared);
    assert_eq!(reopened.worktree_path.as_deref(), Some(path.as_path()));
    assert!(path.join("README.md").is_file());
}

#[tokio::test]
async fn close_counts_tracked_untracked_and_ignored_state_before_discarding_it() {
    let fixture = Fixture::new(FakeWorker::default());
    let repository = fixture.register().await;
    fs::create_dir_all(repository.git_common_dir.join("info")).unwrap();
    fs::write(
        repository.git_common_dir.join("info/exclude"),
        "ignored.log\n",
    )
    .unwrap();
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let path = workspace.worktree_path.as_ref().unwrap();
    fs::write(path.join("README.md"), "changed\n").unwrap();
    fs::write(path.join("untracked.txt"), "new\n").unwrap();
    fs::write(path.join("ignored.log"), "ignored\n").unwrap();

    let mut preview_params = close_params(&fixture, &workspace);
    preview_params.dry_run = true;
    let preview = fixture
        .coordinator
        .close_workspace(preview_params)
        .await
        .unwrap();
    assert!(!preview.applied);
    assert!(preview.plan.tracked_changes);
    assert_eq!(preview.plan.untracked_file_count, 1);
    assert_eq!(preview.plan.ignored_file_count, 1);
    assert!(preview.plan.has_local_changes());
    assert!(!preview.plan.blockers.is_empty());
    assert!(matches!(
        fixture
            .coordinator
            .close_workspace(close_params(&fixture, &workspace))
            .await,
        Err(CoordinatorError::WorkspaceRetirementBlocked(_))
    ));

    let mut discard = close_params(&fixture, &workspace);
    discard.discard_changes = true;
    let closed = fixture.coordinator.close_workspace(discard).await.unwrap();
    assert!(closed.applied);
    assert!(!path.exists());
}

#[tokio::test]
async fn native_archive_is_reversible_and_native_delete_is_opt_in() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let thread_id = workspace.codex_thread_id.clone().unwrap();
    let branch = workspace.branch_name.clone().unwrap();
    let mut close = close_params(&fixture, &workspace);
    close.archive_thread = true;
    let closed = fixture
        .coordinator
        .close_workspace(close)
        .await
        .unwrap()
        .workspace;
    assert!(closed.thread_archived);
    assert!(
        fixture
            .worker
            .archived_threads
            .lock()
            .unwrap()
            .contains(&thread_id)
    );

    let reopened = fixture
        .coordinator
        .reopen_workspace(WorkspaceReopenParams {
            scope: scope(&fixture),
            workspace: workspace.id.clone(),
        })
        .await
        .unwrap()
        .workspace;
    assert!(!reopened.thread_archived);
    let mut close_again = close_params(&fixture, &reopened);
    close_again.archive_thread = true;
    fixture
        .coordinator
        .close_workspace(close_again)
        .await
        .unwrap();

    let deleted = fixture
        .coordinator
        .delete_workspace(WorkspaceDeleteParams {
            scope: scope(&fixture),
            workspace: workspace.id.clone(),
            delete_thread: true,
            delete_branch: true,
            dry_run: false,
            expected_plan: None,
        })
        .await
        .unwrap();
    assert!(deleted.applied);
    assert_eq!(
        deleted.plan.thread_disposition,
        WorkspaceThreadDisposition::Delete
    );
    assert!(
        fixture
            .store
            .workspace_by_id(&workspace.id)
            .unwrap()
            .is_none()
    );
    assert!(
        !fixture
            .worker
            .native_threads
            .lock()
            .unwrap()
            .contains_key(&thread_id)
    );
    assert!(matches!(
        fixture.coordinator.git.resolve_local_branch(
            &fixture.coordinator.git.discover(&fixture.source).unwrap(),
            &branch
        ),
        Err(GitError::BranchNotFound(_))
    ));
}

#[tokio::test]
async fn externally_archived_thread_requires_explicit_archive_ownership() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let thread_id = workspace.codex_thread_id.clone().unwrap();
    fixture.worker.archive_thread(&thread_id).await.unwrap();

    let mut preview = close_params(&fixture, &workspace);
    preview.dry_run = true;
    let preview = fixture.coordinator.close_workspace(preview).await.unwrap();
    assert!(
        preview
            .plan
            .blockers
            .iter()
            .any(|blocker| blocker.contains("already archived"))
    );

    let mut close = close_params(&fixture, &workspace);
    close.archive_thread = true;
    fixture.coordinator.close_workspace(close).await.unwrap();
    let reopened = fixture
        .coordinator
        .reopen_workspace(WorkspaceReopenParams {
            scope: scope(&fixture),
            workspace: workspace.id,
        })
        .await
        .unwrap()
        .workspace;
    assert_eq!(reopened.availability, WorkspaceAvailability::Open);
    assert!(
        !fixture
            .worker
            .archived_threads
            .lock()
            .unwrap()
            .contains(&thread_id)
    );
}

#[tokio::test]
async fn native_delete_reports_known_context_dependants_before_codex_rejects_it() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let source = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let prepared_child = fixture
        .coordinator
        .create_workspace(fixture.fork_params(&source, "context-child", false))
        .await
        .unwrap()
        .workspace;
    fixture.attach(&prepared_child).await;

    let mut close = close_params(&fixture, &source);
    close.archive_thread = true;
    fixture.coordinator.close_workspace(close).await.unwrap();
    let preview = fixture
        .coordinator
        .delete_workspace(WorkspaceDeleteParams {
            scope: scope(&fixture),
            workspace: source.id,
            delete_thread: true,
            delete_branch: false,
            dry_run: true,
            expected_plan: None,
        })
        .await
        .unwrap();

    assert!(preview.plan.blockers.iter().any(|blocker| {
        blocker.contains("context-child") && blocker.contains("dependent threads first")
    }));
}

#[tokio::test]
async fn rejected_native_delete_returns_the_workspace_to_closed_for_a_safe_retry() {
    let fixture = Fixture::new(FakeWorker::failing_delete_thread());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let thread_id = workspace.codex_thread_id.clone().unwrap();
    let mut close = close_params(&fixture, &workspace);
    close.archive_thread = true;
    fixture.coordinator.close_workspace(close).await.unwrap();

    assert!(matches!(
        fixture
            .coordinator
            .delete_workspace(WorkspaceDeleteParams {
                scope: scope(&fixture),
                workspace: workspace.id.clone(),
                delete_thread: true,
                delete_branch: false,
                dry_run: false,
                expected_plan: None,
            })
            .await,
        Err(CoordinatorError::Worker(_))
    ));
    let retained = fixture
        .store
        .workspace_by_id(&workspace.id)
        .unwrap()
        .unwrap();
    assert_eq!(retained.availability, WorkspaceAvailability::Closed);
    assert_eq!(
        retained.codex_thread_id.as_deref(),
        Some(thread_id.as_str())
    );
    assert_eq!(
        fixture
            .store
            .workspace_deletion_intent(&workspace.id)
            .unwrap(),
        WorkspaceDeletionIntent {
            delete_thread: false,
            delete_branch: false,
        }
    );
}

#[tokio::test]
async fn verified_absence_completes_an_ambiguous_native_delete() {
    let fixture = Fixture::new(FakeWorker::ambiguously_deleted_thread());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let mut close = close_params(&fixture, &workspace);
    close.archive_thread = true;
    fixture.coordinator.close_workspace(close).await.unwrap();

    let result = fixture
        .coordinator
        .delete_workspace(WorkspaceDeleteParams {
            scope: scope(&fixture),
            workspace: workspace.id.clone(),
            delete_thread: true,
            delete_branch: false,
            dry_run: false,
            expected_plan: None,
        })
        .await
        .unwrap();

    assert!(result.applied);
    assert!(
        fixture
            .store
            .workspace_by_id(&workspace.id)
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn descendants_background_terminals_and_tui_leases_block_close() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let thread_id = workspace.codex_thread_id.as_deref().unwrap();
    fixture.worker.set_descendants(thread_id, &["thread-child"]);
    fixture.worker.set_background_terminals(thread_id, 2);
    let attached = fixture
        .coordinator
        .attach_workspace(WorkspaceAttachParams {
            scope: scope(&fixture),
            workspace: workspace.id.clone(),
        })
        .await
        .unwrap();

    let mut preview = close_params(&fixture, &workspace);
    preview.archive_thread = true;
    preview.dry_run = true;
    let plan = fixture
        .coordinator
        .close_workspace(preview)
        .await
        .unwrap()
        .plan;
    assert_eq!(plan.descendant_thread_count, 1);
    assert!(
        plan.blockers
            .iter()
            .any(|value| value.contains("terminal UI"))
    );
    assert!(
        plan.blockers
            .iter()
            .any(|value| value.contains("background terminal"))
    );
    assert!(
        plan.blockers
            .iter()
            .any(|value| value.contains("descendant"))
    );

    let lease_id = match attached.launch {
        WorkspaceAttachLaunch::Resume { lease_id, .. } => lease_id,
        WorkspaceAttachLaunch::Start { .. } => panic!("materialized workspace must resume"),
    };
    fixture
        .coordinator
        .release_workspace_attach(WorkspaceAttachReleaseParams {
            workspace_id: workspace.id,
            lease_id,
        })
        .unwrap();
}

#[tokio::test]
async fn detached_commits_are_never_silently_deleted() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let mut params = fixture.create_params();
    params.name = "detached".to_owned();
    params.operation_id = "create-detached".to_owned();
    params.worktree = WorkspaceWorktreeRequest::Detached {
        base: WorkspaceBaseRequest::Revision {
            revision: "HEAD".to_owned(),
        },
    };
    let detached = fixture
        .coordinator
        .create_workspace(params)
        .await
        .unwrap()
        .workspace;
    let path = detached.worktree_path.as_ref().unwrap();
    fs::write(path.join("commit.txt"), "commit\n").unwrap();
    run_git(path, &["add", "commit.txt"]);
    run_git(path, &["commit", "-m", "detached commit"]);
    let mut preview = close_params(&fixture, &detached);
    preview.dry_run = true;
    let plan = fixture
        .coordinator
        .close_workspace(preview)
        .await
        .unwrap()
        .plan;
    assert!(plan.detached_commits);
    assert!(plan.blockers.iter().any(|value| value.contains("detached")));
}

#[tokio::test]
async fn delete_branch_never_claims_an_existing_branch() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    run_git(&fixture.source, &["branch", "feat/existing"]);
    let mut params = fixture.create_params();
    params.name = "existing-owner".to_owned();
    params.operation_id = "create-existing-owner".to_owned();
    params.worktree = WorkspaceWorktreeRequest::ExistingBranch {
        branch: "feat/existing".to_owned(),
    };
    let workspace = fixture
        .coordinator
        .create_workspace(params)
        .await
        .unwrap()
        .workspace;
    fixture
        .coordinator
        .close_workspace(close_params(&fixture, &workspace))
        .await
        .unwrap();

    let deletion = WorkspaceDeleteParams {
        scope: scope(&fixture),
        workspace: workspace.id.clone(),
        delete_thread: false,
        delete_branch: true,
        dry_run: true,
        expected_plan: None,
    };
    let preview = fixture
        .coordinator
        .delete_workspace(deletion.clone())
        .await
        .unwrap();
    assert!(
        preview
            .plan
            .blockers
            .iter()
            .any(|blocker| blocker.contains("did not create"))
    );
    assert!(matches!(
        fixture
            .coordinator
            .delete_workspace(WorkspaceDeleteParams {
                dry_run: false,
                ..deletion
            })
            .await,
        Err(CoordinatorError::WorkspaceRetirementBlocked(_))
    ));
    assert_eq!(
        git_output(&fixture.source, &["rev-parse", "feat/existing"]),
        git_output(&fixture.source, &["rev-parse", "HEAD"])
    );
}

#[tokio::test]
async fn interrupted_close_converges_from_the_observed_git_state() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let path = workspace.worktree_path.as_ref().unwrap();
    let (_, repository) = fixture
        .coordinator
        .git_repository_for_workspace(&workspace)
        .unwrap();
    let observation = fixture
        .coordinator
        .git
        .observe_worktree_retirement(
            &repository,
            path,
            workspace.worktree_mode,
            workspace.branch_name.as_deref(),
        )
        .unwrap();
    fixture
        .store
        .begin_workspace_close(&workspace.id, &observation.binding.head_sha, false)
        .unwrap();
    fixture
        .coordinator
        .git
        .remove_worktree(&repository, &observation.binding, false)
        .unwrap();

    assert_eq!(fixture.coordinator.recover_workspace_retirements().await, 1);
    let recovered = fixture
        .store
        .workspace_by_id(&workspace.id)
        .unwrap()
        .unwrap();
    assert_eq!(recovered.availability, WorkspaceAvailability::Closed);
    assert!(!path.exists());
}

#[tokio::test]
async fn interrupted_archive_rolls_back_when_the_worktree_still_exists() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let thread_id = workspace.codex_thread_id.as_deref().unwrap();
    let path = workspace.worktree_path.as_ref().unwrap();
    let (_, repository) = fixture
        .coordinator
        .git_repository_for_workspace(&workspace)
        .unwrap();
    let observation = fixture
        .coordinator
        .git
        .observe_worktree_retirement(
            &repository,
            path,
            workspace.worktree_mode,
            workspace.branch_name.as_deref(),
        )
        .unwrap();
    fixture
        .store
        .begin_workspace_close(&workspace.id, &observation.binding.head_sha, true)
        .unwrap();
    fixture.worker.archive_thread(thread_id).await.unwrap();

    assert_eq!(fixture.coordinator.recover_workspace_retirements().await, 1);
    let recovered = fixture
        .store
        .workspace_by_id(&workspace.id)
        .unwrap()
        .unwrap();
    assert_eq!(recovered.availability, WorkspaceAvailability::Open);
    assert!(!recovered.thread_archived);
    assert!(path.exists());
    assert!(
        !fixture
            .worker
            .archived_threads
            .lock()
            .unwrap()
            .contains(thread_id)
    );
}

#[tokio::test]
async fn interrupted_reopen_before_git_restoration_returns_to_closed() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let path = workspace.worktree_path.clone().unwrap();
    let mut close = close_params(&fixture, &workspace);
    close.archive_thread = true;
    let closed = fixture
        .coordinator
        .close_workspace(close)
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

    assert_eq!(fixture.coordinator.recover_workspace_retirements().await, 1);
    let recovered = fixture.store.workspace_by_id(&closed.id).unwrap().unwrap();
    assert_eq!(recovered.availability, WorkspaceAvailability::Closed);
    assert!(recovered.thread_archived);
    assert!(!path.exists());
}

#[tokio::test]
async fn interrupted_reopen_after_git_restoration_finishes_open() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let mut close = close_params(&fixture, &workspace);
    close.archive_thread = true;
    let closed = fixture
        .coordinator
        .close_workspace(close)
        .await
        .unwrap()
        .workspace;
    let reopening = fixture
        .store
        .transition_workspace_availability(
            &closed.id,
            WorkspaceAvailability::Closed,
            WorkspaceAvailability::Reopening,
            None,
        )
        .unwrap();
    let (_, repository) = fixture
        .coordinator
        .git_repository_for_workspace(&reopening)
        .unwrap();
    fixture
        .coordinator
        .git
        .restore_worktree(
            &repository,
            &fixture.worktrees,
            &reopening.name,
            reopening.worktree_path.as_deref().unwrap(),
            reopening.worktree_mode,
            reopening.branch_name.as_deref(),
            reopening.base_sha.as_deref().unwrap(),
            reopening.closed_head_sha.as_deref().unwrap(),
        )
        .unwrap();

    assert_eq!(fixture.coordinator.recover_workspace_retirements().await, 1);
    let recovered = fixture
        .store
        .workspace_by_id(&reopening.id)
        .unwrap()
        .unwrap();
    assert_eq!(recovered.availability, WorkspaceAvailability::Open);
    assert!(!recovered.thread_archived);
    assert!(recovered.closed_head_sha.is_none());
    assert!(recovered.worktree_path.unwrap().is_dir());
}

#[tokio::test]
async fn interrupted_delete_finishes_already_applied_native_and_git_steps() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let thread_id = workspace.codex_thread_id.as_deref().unwrap();
    let branch = workspace.branch_name.as_deref().unwrap();
    let mut close = close_params(&fixture, &workspace);
    close.archive_thread = true;
    let closed = fixture
        .coordinator
        .close_workspace(close)
        .await
        .unwrap()
        .workspace;
    fixture
        .store
        .begin_workspace_deletion(
            &closed.id,
            WorkspaceDeletionIntent {
                delete_thread: true,
                delete_branch: true,
            },
        )
        .unwrap();
    fixture.worker.delete_thread(thread_id).await.unwrap();
    let repository = fixture.coordinator.git.discover(&fixture.source).unwrap();
    fixture
        .coordinator
        .git
        .delete_created_branch(
            &repository,
            branch,
            closed.closed_head_sha.as_deref().unwrap(),
        )
        .unwrap();

    assert_eq!(fixture.coordinator.recover_workspace_retirements().await, 1);
    assert!(
        fixture
            .store
            .workspace_by_id(&workspace.id)
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        fixture
            .coordinator
            .git
            .resolve_local_branch(&repository, branch),
        Err(GitError::BranchNotFound(_))
    ));
}
