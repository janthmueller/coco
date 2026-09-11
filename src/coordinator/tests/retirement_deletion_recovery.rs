use super::retirement_deletion::delete_params;
use super::*;

#[tokio::test]
async fn interrupted_open_deletion_preserves_the_worktree_for_a_new_confirmation() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let path = workspace.worktree_path.as_ref().unwrap();
    let head = git_output(path, &["rev-parse", "HEAD"]);
    fixture
        .store
        .begin_workspace_deletion(
            &workspace.id,
            WorkspaceDeletionIntent {
                delete_thread: true,
                delete_branch: true,
                from_open: true,
                discard_unretained_commits: true,
            },
            Some(&head),
        )
        .unwrap();
    fs::write(path.join("after-crash.txt"), "never replay a file discard").unwrap();
    assert_eq!(fixture.coordinator.recover_workspace_retirements().await, 1);
    assert_eq!(
        fixture
            .store
            .workspace_by_id(&workspace.id)
            .unwrap()
            .unwrap()
            .availability,
        WorkspaceAvailability::Open
    );
    assert!(path.join("after-crash.txt").is_file());
    assert!(
        fixture
            .worker
            .native_threads
            .lock()
            .unwrap()
            .contains_key(workspace.codex_thread_id.as_ref().unwrap())
    );
    assert_eq!(
        fixture
            .store
            .workspace_deletion_intent(&workspace.id)
            .unwrap(),
        WorkspaceDeletionIntent::default()
    );
}

#[tokio::test]
async fn interrupted_open_deletion_finishes_after_the_verified_worktree_is_gone() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let path = workspace.worktree_path.as_ref().unwrap();
    let head = git_output(path, &["rev-parse", "HEAD"]);
    fixture
        .store
        .begin_workspace_deletion(
            &workspace.id,
            WorkspaceDeletionIntent {
                delete_thread: true,
                delete_branch: true,
                from_open: true,
                discard_unretained_commits: false,
            },
            Some(&head),
        )
        .unwrap();
    git_output(
        &fixture.source,
        &["worktree", "remove", "--", path.to_str().unwrap()],
    );
    assert_eq!(fixture.coordinator.recover_workspace_retirements().await, 1);
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
            .contains_key(workspace.codex_thread_id.as_ref().unwrap())
    );
}

#[tokio::test]
async fn delete_cleans_failed_provisioning_without_claiming_an_unverified_branch() {
    for missing_worktree in [false, true] {
        let fixture = Fixture::new(FakeWorker::default());
        fixture.register().await;
        let template = fixture
            .coordinator
            .create_workspace(fixture.create_params())
            .await
            .unwrap()
            .workspace;
        let repository = fixture.coordinator.git.discover(&fixture.source).unwrap();
        let plan = fixture
            .coordinator
            .git
            .plan_worktree(
                &repository,
                &fixture.coordinator.worktrees_dir,
                "failed-create",
                crate::git::WorktreeTarget::NewBranch {
                    branch_name: "coco/failed-create".into(),
                },
                template.base_sha.as_deref().unwrap(),
            )
            .unwrap();
        let (workspace, _) = fixture
            .store
            .create_workspace_with_event(
                crate::store::NewWorkspace {
                    create_operation_id: Some("failed-create-operation".into()),
                    repository_id: template.repository_id.clone(),
                    name: "failed-create".into(),
                    context_mode: template.context_mode,
                    context: template.context.clone(),
                    profile: template.profile.clone(),
                    worktree_mode: plan.mode,
                    branch_name: plan.branch_name.clone(),
                    base_sha: Some(plan.base_sha.clone()),
                    worktree_path: Some(plan.path.clone()),
                },
                EventDraft::workspace(EventKind::WorkspaceCreated, EventSource::Coco, json!({})),
            )
            .unwrap();
        if missing_worktree {
            // A competing branch appeared after planning, before Git creation.
            git_output(&fixture.source, &["branch", "coco/failed-create"]);
        } else {
            fixture
                .coordinator
                .git
                .create_worktree(&repository, &plan)
                .unwrap();
        }
        fixture.coordinator.mark_workspace_failed(
            &workspace.id,
            "worktree.create",
            &CoordinatorError::InvalidParams("simulated failed provisioning".into()),
            EventSource::Git,
        );
        assert_eq!(
            fixture
                .store
                .workspace_by_id(&workspace.id)
                .unwrap()
                .unwrap()
                .lifecycle,
            WorkspaceLifecycle::Failed
        );
        let result = fixture
            .coordinator
            .delete_workspace(delete_params(&fixture, &workspace))
            .await
            .unwrap();
        assert_eq!(result.plan.remove_worktree, !missing_worktree);
        assert_eq!(result.plan.delete_branch, !missing_worktree);
        assert!(
            fixture
                .store
                .workspace_by_id(&workspace.id)
                .unwrap()
                .is_none()
        );
        if missing_worktree {
            assert_eq!(
                git_output(
                    &fixture.source,
                    &["rev-parse", workspace.branch_name.as_ref().unwrap()]
                ),
                workspace.base_sha.unwrap()
            );
        }
    }
}
