use super::retirement::close_params;
use super::*;

#[tokio::test]
async fn confirmed_name_must_not_retarget_to_a_replacement_workspace() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let original = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    fixture
        .coordinator
        .close_workspace(close_params(&fixture, &original))
        .await
        .unwrap();
    let request = WorkspaceDeleteParams {
        scope: RepositoryScope::repository(fixture.source.clone()),
        workspace: original.name.clone(),
        delete_thread: false,
        delete_branch: false,
        discard_changes: false,
        discard_unretained_commits: false,
        dry_run: true,
        expected_plan: None,
    };
    let preview = fixture
        .coordinator
        .delete_workspace(request.clone())
        .await
        .unwrap();

    // Another client replaces the named workspace while the CLI asks for confirmation.
    fixture
        .coordinator
        .delete_workspace(WorkspaceDeleteParams {
            workspace: original.id.clone(),
            dry_run: false,
            expected_plan: None,
            ..request.clone()
        })
        .await
        .unwrap();
    let mut create = fixture.create_params();
    create.operation_id = "replacement-create".to_owned();
    create.worktree = WorkspaceWorktreeRequest::NewBranch {
        branch: Some("coco/replacement".to_owned()),
        base: WorkspaceBaseRequest::Revision {
            revision: "HEAD".to_owned(),
        },
    };
    let replacement = fixture
        .coordinator
        .create_workspace(create)
        .await
        .unwrap()
        .workspace;
    fixture
        .coordinator
        .close_workspace(close_params(&fixture, &replacement))
        .await
        .unwrap();

    // An acknowledged plan cannot be redirected even by a client that resends a name.
    let applied = fixture
        .coordinator
        .delete_workspace(WorkspaceDeleteParams {
            dry_run: false,
            expected_plan: Some(preview.plan),
            ..request
        })
        .await;
    assert!(matches!(
        applied,
        Err(CoordinatorError::WorkspaceRetirementBlocked(_))
    ));
    assert!(
        fixture
            .store
            .workspace_by_id(&replacement.id)
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn confirmed_close_rejects_commits_and_new_local_files_after_preview() {
    for commit in [false, true] {
        let fixture = Fixture::new(FakeWorker::default());
        fixture.register().await;
        let workspace = fixture
            .coordinator
            .create_workspace(fixture.create_params())
            .await
            .unwrap()
            .workspace;
        let mut params = close_params(&fixture, &workspace);
        params.discard_changes = true;
        params.dry_run = true;
        let preview = fixture
            .coordinator
            .close_workspace(params.clone())
            .await
            .unwrap();
        let path = workspace.worktree_path.as_ref().unwrap();
        fs::write(path.join("late.txt"), "retain me\n").unwrap();
        if commit {
            run_git(path, &["add", "late.txt"]);
            run_git(path, &["commit", "-m", "late commit"]);
        }
        params.dry_run = false;
        params.expected_plan = Some(preview.plan);
        assert!(matches!(
            fixture.coordinator.close_workspace(params).await,
            Err(CoordinatorError::WorkspaceRetirementBlocked(_))
        ));
        assert!(path.join("late.txt").is_file());
    }
}
