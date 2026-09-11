use super::retirement::close_params;
use super::*;
use crate::git::GitError;

pub(super) fn delete_params(fixture: &Fixture, workspace: &Workspace) -> WorkspaceDeleteParams {
    WorkspaceDeleteParams {
        scope: RepositoryScope::repository(fixture.source.clone()),
        workspace: workspace.id.clone(),
        delete_thread: true,
        delete_branch: true,
        discard_changes: false,
        discard_unretained_commits: false,
        dry_run: false,
        expected_plan: None,
    }
}

fn commit(path: &Path) {
    fs::write(path.join("unique.txt"), "workspace commit\n").unwrap();
    git_output(path, &["add", "unique.txt"]);
    git_output(path, &["commit", "-m", "workspace-only commit"]);
}

#[tokio::test]
async fn delete_open_workspace_previews_and_removes_all_owned_resources() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let path = workspace.worktree_path.as_ref().unwrap();
    let thread = workspace.codex_thread_id.as_ref().unwrap();
    let branch = workspace.branch_name.as_ref().unwrap();
    let mut request = delete_params(&fixture, &workspace);
    request.dry_run = true;
    let preview = fixture
        .coordinator
        .delete_workspace(request.clone())
        .await
        .unwrap();
    assert!(preview.plan.blockers.is_empty());
    assert!(preview.plan.remove_worktree && preview.plan.delete_branch);
    assert_eq!(
        preview.plan.thread_disposition,
        WorkspaceThreadDisposition::Delete
    );
    assert!(path.is_dir());
    assert!(
        fixture
            .worker
            .native_threads
            .lock()
            .unwrap()
            .contains_key(thread)
    );
    request.dry_run = false;
    request.expected_plan = Some(preview.plan);
    fixture.coordinator.delete_workspace(request).await.unwrap();
    assert!(!path.exists());
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
            .contains_key(thread)
    );
    let repository = fixture.coordinator.git.discover(&fixture.source).unwrap();
    assert!(matches!(
        fixture
            .coordinator
            .git
            .resolve_local_branch(&repository, branch),
        Err(GitError::BranchNotFound(_))
    ));
    assert!(
        fixture
            .worker
            .calls
            .lock()
            .unwrap()
            .contains(&WorkerCall::StopExecution {
                workspace_id: workspace.id
            })
    );
}

#[tokio::test]
async fn delete_keep_policies_preserve_discoverable_branch_and_conversation() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let path = workspace.worktree_path.as_ref().unwrap();
    commit(path);
    let mut request = delete_params(&fixture, &workspace);
    request.delete_branch = false;
    request.delete_thread = false;
    let result = fixture.coordinator.delete_workspace(request).await.unwrap();
    assert!(!path.exists());
    assert_eq!(result.plan.unretained_commit_count, 0);
    assert_eq!(result.plan.thread_id, workspace.codex_thread_id);
    assert_eq!(result.plan.branch_name, workspace.branch_name);
    assert!(
        fixture
            .worker
            .native_threads
            .lock()
            .unwrap()
            .contains_key(workspace.codex_thread_id.as_ref().unwrap())
    );
    assert_eq!(
        git_output(
            &fixture.source,
            &["rev-parse", workspace.branch_name.as_ref().unwrap()]
        ),
        result.plan.head_sha.unwrap()
    );
}

#[tokio::test]
async fn delete_always_preserves_an_adopted_branch() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    git_output(&fixture.source, &["branch", "existing"]);
    let mut create = fixture.create_params();
    create.worktree = WorkspaceWorktreeRequest::ExistingBranch {
        branch: "existing".into(),
    };
    let workspace = fixture
        .coordinator
        .create_workspace(create)
        .await
        .unwrap()
        .workspace;
    commit(workspace.worktree_path.as_ref().unwrap());
    let result = fixture
        .coordinator
        .delete_workspace(delete_params(&fixture, &workspace))
        .await
        .unwrap();
    assert!(!result.plan.delete_branch);
    assert_eq!(result.plan.branch_name.as_deref(), Some("existing"));
    assert_eq!(
        git_output(&fixture.source, &["rev-parse", "existing"]),
        result.plan.head_sha.unwrap()
    );
}

#[tokio::test]
async fn delete_protects_unique_commits_for_open_closed_and_detached_workspaces() {
    for (closed, detached) in [(false, false), (true, false), (false, true)] {
        let fixture = Fixture::new(FakeWorker::default());
        fixture.register().await;
        let mut create = fixture.create_params();
        if detached {
            create.worktree = WorkspaceWorktreeRequest::Detached {
                base: WorkspaceBaseRequest::Revision {
                    revision: "HEAD".into(),
                },
            };
        }
        let workspace = fixture
            .coordinator
            .create_workspace(create)
            .await
            .unwrap()
            .workspace;
        let path = workspace.worktree_path.as_ref().unwrap();
        commit(path);
        if closed {
            fixture
                .coordinator
                .close_workspace(close_params(&fixture, &workspace))
                .await
                .unwrap();
        }
        let mut request = delete_params(&fixture, &workspace);
        request.dry_run = true;
        let preview = fixture
            .coordinator
            .delete_workspace(request.clone())
            .await
            .unwrap();
        assert_eq!(preview.plan.unretained_commit_count, 1);
        assert!(
            preview
                .plan
                .blockers
                .iter()
                .any(|b| b.contains("--discard-unretained-commits"))
        );
        request.dry_run = false;
        assert!(
            fixture
                .coordinator
                .delete_workspace(request.clone())
                .await
                .is_err()
        );
        assert_eq!(path.exists(), !closed);
        request.discard_unretained_commits = true;
        fixture.coordinator.delete_workspace(request).await.unwrap();
        assert!(
            fixture
                .store
                .workspace_by_id(&workspace.id)
                .unwrap()
                .is_none()
        );
    }
}

#[tokio::test]
async fn close_detached_commits_works_when_a_tag_retains_them() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let mut create = fixture.create_params();
    create.worktree = WorkspaceWorktreeRequest::Detached {
        base: WorkspaceBaseRequest::Revision {
            revision: "HEAD".into(),
        },
    };
    let workspace = fixture
        .coordinator
        .create_workspace(create)
        .await
        .unwrap()
        .workspace;
    let path = workspace.worktree_path.as_ref().unwrap();
    commit(path);
    assert!(
        fixture
            .coordinator
            .close_workspace(close_params(&fixture, &workspace))
            .await
            .is_err()
    );
    git_output(path, &["tag", "save-detached"]);
    let result = fixture
        .coordinator
        .close_workspace(close_params(&fixture, &workspace))
        .await
        .unwrap();
    assert!(!result.plan.detached_commits);
    assert!(!path.exists());
    fixture
        .coordinator
        .reopen_workspace(WorkspaceReopenParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id,
        })
        .await
        .unwrap();
    assert!(path.join("unique.txt").is_file());
}

#[tokio::test]
async fn delete_requires_independent_file_discard_and_checks_busy_threads_first() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let path = workspace.worktree_path.as_ref().unwrap();
    fs::write(path.join("new.txt"), "keep me").unwrap();
    let mut request = delete_params(&fixture, &workspace);
    request.discard_unretained_commits = true;
    assert!(
        fixture
            .coordinator
            .delete_workspace(request.clone())
            .await
            .is_err()
    );
    request.discard_changes = true;
    let thread = workspace.codex_thread_id.as_ref().unwrap();
    fixture.worker.set_background_terminals(thread, 1);
    assert!(
        fixture
            .coordinator
            .delete_workspace(request.clone())
            .await
            .is_err()
    );
    assert!(path.join("new.txt").is_file());
    assert_eq!(
        fixture
            .store
            .workspace_by_id(&workspace.id)
            .unwrap()
            .unwrap()
            .availability,
        WorkspaceAvailability::Open
    );
    fixture.worker.set_background_terminals(thread, 0);
    fixture.coordinator.delete_workspace(request).await.unwrap();
    assert!(!path.exists());
}
