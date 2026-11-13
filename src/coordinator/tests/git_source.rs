use super::*;
use crate::git::GitError;
use crate::protocol::RepositoryGitInspectParams;

#[tokio::test]
async fn git_inspection_is_read_only_even_before_repository_registration() {
    let fixture = Fixture::new(FakeWorker::default());
    let head = git_output(&fixture.source, &["rev-parse", "HEAD"]);
    let state = fixture
        .coordinator
        .inspect_repository_git(RepositoryGitInspectParams {
            path: fixture.source.clone(),
            base: Some(WorkspaceBaseRequest::Revision {
                revision: "main".into(),
            }),
            include_branches: true,
        })
        .unwrap();
    assert_eq!(state.branch.as_deref(), Some("main"));
    assert_eq!(state.base_sha.as_deref(), Some(head.as_str()));
    assert!(fixture.store.list_repositories().unwrap().is_empty());
    assert!(!fixture.worktrees.exists());
    assert!(fixture.worker.calls().is_empty());
}

#[tokio::test]
async fn a_reviewed_base_change_fails_before_provisioning_and_replay_keeps_original_result() {
    let fixture = Fixture::new(FakeWorker::default());
    let head = git_output(&fixture.source, &["rev-parse", "HEAD"]);
    let mut params = fixture.create_params();
    params.expected_base_sha = Some(head.clone());
    run_git(
        &fixture.source,
        &["commit", "--allow-empty", "-m", "advance"],
    );
    let error = fixture
        .coordinator
        .create_workspace(params.clone())
        .await
        .unwrap_err();
    assert!(matches!(error, CoordinatorError::CreationBaseChanged));
    assert_eq!(error.code(), "CREATE_BASE_CHANGED");
    assert!(!fixture.worktrees.exists());
    assert!(fixture.worker.calls().is_empty());
    let current = git_output(&fixture.source, &["rev-parse", "HEAD"]);
    params.expected_base_sha = Some(current.clone());
    let created = fixture
        .coordinator
        .create_workspace(params.clone())
        .await
        .unwrap()
        .workspace;
    assert_eq!(created.base_sha.as_deref(), Some(current.as_str()));
    run_git(&fixture.source, &["commit", "--allow-empty", "-m", "later"]);
    let replay = fixture
        .coordinator
        .create_workspace(params.clone())
        .await
        .unwrap()
        .workspace;
    assert_eq!(replay.id, created.id);
    params.expected_base_sha = Some(head);
    assert!(matches!(
        fixture.coordinator.create_workspace(params).await,
        Err(CoordinatorError::IdempotencyConflict)
    ));
}

#[tokio::test]
async fn occupied_branch_is_a_valid_base_but_cannot_be_adopted() {
    let fixture = Fixture::new(FakeWorker::default());
    let head = git_output(&fixture.source, &["rev-parse", "main"]);
    let mut params = fixture.create_params();
    params.expected_base_sha = Some(head.clone());
    params.worktree = WorkspaceWorktreeRequest::NewBranch {
        branch: None,
        base: WorkspaceBaseRequest::Revision {
            revision: "refs/heads/main".into(),
        },
    };
    let workspace = fixture
        .coordinator
        .create_workspace(params)
        .await
        .unwrap()
        .workspace;
    assert_eq!(workspace.base_sha.as_deref(), Some(head.as_str()));
    let mut checkout = fixture.create_params();
    checkout.name = "other".into();
    checkout.operation_id = "checkout-main".into();
    checkout.worktree = WorkspaceWorktreeRequest::ExistingBranch {
        branch: "main".into(),
    };
    assert!(matches!(
        fixture.coordinator.create_workspace(checkout).await,
        Err(CoordinatorError::Git(
            GitError::BranchAlreadyCheckedOut { .. }
        ))
    ));
    assert_eq!(git_output(&fixture.source, &["rev-parse", "main"]), head);
}

#[tokio::test]
async fn late_untracked_files_cannot_broaden_a_tracked_only_copy() {
    let fixture = Fixture::new(FakeWorker::default());
    let mut params = fixture.create_params();
    params.expected_base_sha = Some(git_output(&fixture.source, &["rev-parse", "HEAD"]));
    params.changes = WorkspaceChangesRequest::CarryTracked;
    fs::write(fixture.source.join("README.md"), "tracked edit").unwrap();
    fs::write(fixture.source.join("late.txt"), "keep here").unwrap();
    assert!(matches!(
        fixture.coordinator.create_workspace(params).await,
        Err(CoordinatorError::Git(GitError::UntrackedChanges(_)))
    ));
    assert!(!fixture.worktrees.exists());
    assert!(fixture.worker.calls().is_empty());
    assert_eq!(
        fs::read_to_string(fixture.source.join("late.txt")).unwrap(),
        "keep here"
    );
}

#[tokio::test]
async fn a_deleted_selected_branch_never_falls_back_to_head_or_provisions_a_workspace() {
    let fixture = Fixture::new(FakeWorker::default());
    run_git(&fixture.source, &["branch", "review-source"]);
    let inspection = RepositoryGitInspectParams {
        path: fixture.source.clone(),
        base: Some(WorkspaceBaseRequest::Revision {
            revision: "refs/heads/review-source".into(),
        }),
        include_branches: true,
    };
    let reviewed = fixture
        .coordinator
        .inspect_repository_git(inspection.clone())
        .unwrap();
    assert!(
        reviewed
            .branches
            .iter()
            .any(|branch| branch.name == "review-source")
    );
    let mut params = fixture.create_params();
    params.expected_base_sha = reviewed.base_sha.clone();
    params.worktree = WorkspaceWorktreeRequest::NewBranch {
        branch: None,
        base: inspection.base.clone().unwrap(),
    };
    run_git(
        &fixture.source,
        &["commit", "--allow-empty", "-m", "advance HEAD"],
    );
    assert_ne!(
        reviewed.base_sha.unwrap(),
        git_output(&fixture.source, &["rev-parse", "HEAD"])
    );
    run_git(&fixture.source, &["branch", "-D", "review-source"]);
    assert!(matches!(
        fixture.coordinator.inspect_repository_git(inspection),
        Err(CoordinatorError::Git(GitError::CommandFailed {
            category: "resolve-commit",
            ..
        }))
    ));
    let name = params.name.clone();
    assert!(matches!(
        fixture.coordinator.create_workspace(params).await,
        Err(CoordinatorError::Git(GitError::CommandFailed {
            category: "resolve-commit",
            ..
        }))
    ));
    assert!(
        fixture
            .store
            .workspace_by_name(&reviewed.repository_id, &name)
            .unwrap()
            .is_none()
    );
    assert!(!fixture.worktrees.exists());
    assert!(fixture.worker.calls().is_empty());
}

#[tokio::test]
async fn a_moved_selected_branch_requires_the_new_reviewed_commit_independent_of_head() {
    let fixture = Fixture::new(FakeWorker::default());
    run_git(&fixture.source, &["branch", "review-source"]);
    let inspection = RepositoryGitInspectParams {
        path: fixture.source.clone(),
        base: Some(WorkspaceBaseRequest::Revision {
            revision: "refs/heads/review-source".into(),
        }),
        include_branches: false,
    };
    let reviewed = fixture
        .coordinator
        .inspect_repository_git(inspection.clone())
        .unwrap();
    let mut params = fixture.create_params();
    params.expected_base_sha = reviewed.base_sha.clone();
    params.worktree = WorkspaceWorktreeRequest::NewBranch {
        branch: None,
        base: inspection.base.clone().unwrap(),
    };
    let advanced = git_output(
        &fixture.source,
        &[
            "commit-tree",
            "HEAD^{tree}",
            "-p",
            "HEAD",
            "-m",
            "advance selected branch",
        ],
    );
    run_git(
        &fixture.source,
        &["update-ref", "refs/heads/review-source", &advanced],
    );
    let refreshed = fixture
        .coordinator
        .inspect_repository_git(inspection)
        .unwrap();
    assert_eq!(refreshed.head_sha, reviewed.head_sha);
    assert_ne!(refreshed.head_sha, refreshed.base_sha);
    assert_eq!(refreshed.base_sha.as_deref(), Some(advanced.as_str()));
    assert!(matches!(
        fixture.coordinator.create_workspace(params.clone()).await,
        Err(CoordinatorError::CreationBaseChanged)
    ));
    assert!(
        fixture
            .store
            .workspace_by_name(&reviewed.repository_id, &params.name)
            .unwrap()
            .is_none()
    );
    assert!(!fixture.worktrees.exists());
    assert!(fixture.worker.calls().is_empty());
    params.expected_base_sha = refreshed.base_sha;
    let created = fixture
        .coordinator
        .create_workspace(params)
        .await
        .unwrap()
        .workspace;
    assert_eq!(created.base_sha.as_deref(), Some(advanced.as_str()));
    assert_eq!(
        git_output(&fixture.source, &["rev-parse", "HEAD"]),
        reviewed.head_sha.unwrap()
    );
}
