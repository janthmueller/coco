use super::*;

#[tokio::test]
async fn local_file_and_commit_loss_require_separate_explicit_policies() {
    let fixture = Fixture::new(FakeWorker::default());
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let path = workspace.worktree_path.as_ref().unwrap();
    fs::write(path.join("committed"), "unique commit").unwrap();
    git_output(path, &["add", "committed"]);
    git_output(path, &["commit", "-m", "unique"]);
    fs::write(path.join("README.md"), "tracked changes").unwrap();
    fs::write(path.join("untracked"), "local data").unwrap();
    fs::write(fixture.source.join(".git/info/exclude"), "ignored-data\n").unwrap();
    fs::write(path.join("ignored-data"), "ignored local data").unwrap();
    let mut params = request(&fixture, &workspace);
    let plan = preview(&fixture, &params).await.plan.unwrap();
    assert!(
        plan.tracked_changes
            && plan.untracked_file_count > 0
            && plan.ignored_file_count > 0
            && plan.unretained_commit_count > 0
    );
    let rejected = fixture
        .coordinator
        .delete_workspace_operation(params.clone())
        .await
        .unwrap();
    assert_eq!(rejected.state, WorkspaceDeletionState::Rejected);
    assert_eq!(
        rejected.error_code.as_deref(),
        Some("WORKSPACE_RETIREMENT_BLOCKED")
    );
    assert!(path.join("untracked").is_file());
    params.request.operation_id = "file-discard-only".into();
    params.request.discard_changes = true;
    assert_eq!(
        fixture
            .coordinator
            .delete_workspace_operation(params.clone())
            .await
            .unwrap()
            .state,
        WorkspaceDeletionState::Rejected
    );
    assert!(path.is_dir());
    params.request.operation_id = "both-explicitly-authorized".into();
    params.request.discard_unretained_commits = true;
    assert_eq!(
        fixture
            .coordinator
            .delete_workspace_operation(params)
            .await
            .unwrap()
            .state,
        WorkspaceDeletionState::Deleted
    );
    assert!(!path.exists());
}

#[tokio::test]
async fn keep_policies_and_closed_targets_reuse_existing_retirement() {
    let fixture = Fixture::new(FakeWorker::default());
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    fixture
        .coordinator
        .close_workspace(super::super::retirement::close_params(&fixture, &workspace))
        .await
        .unwrap();
    let mut params = request(&fixture, &workspace);
    params.request.keep_thread = true;
    params.request.keep_branch = true;
    let receipt = fixture
        .coordinator
        .delete_workspace_operation(params)
        .await
        .unwrap();
    assert_eq!(receipt.state, WorkspaceDeletionState::Deleted);
    let plan = receipt.plan.unwrap();
    assert!(!plan.remove_worktree && !plan.delete_branch);
    assert_eq!(plan.thread_disposition, WorkspaceThreadDisposition::Retain);
    assert!(
        fixture
            .worker
            .native_threads
            .lock()
            .unwrap()
            .contains_key(workspace.codex_thread_id.as_ref().unwrap())
    );
    assert!(
        !git_output(
            &fixture.source,
            &["rev-parse", workspace.branch_name.as_ref().unwrap()]
        )
        .is_empty()
    );
}

#[tokio::test]
async fn attached_and_busy_workspaces_cannot_be_deleted_through_public_setup() {
    let fixture = Fixture::new(FakeWorker::default());
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let attached = fixture.attach(&workspace).await;
    let params = request(&fixture, &attached);
    let receipt = fixture
        .coordinator
        .delete_workspace_operation(params)
        .await
        .unwrap();
    assert_eq!(receipt.state, WorkspaceDeletionState::Rejected);
    assert!(
        receipt
            .plan
            .unwrap()
            .blockers
            .iter()
            .any(|b| b.contains("attached"))
    );
    assert!(workspace.worktree_path.as_ref().unwrap().exists());
    assert!(
        !fixture
            .worker
            .calls()
            .iter()
            .any(|call| matches!(call, WorkerCall::DeleteThread { .. }))
    );
}

#[tokio::test]
async fn native_activity_and_configured_guards_are_not_bypassed() {
    let fixture = Fixture::new(FakeWorker::default());
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    fixture
        .worker
        .native_threads
        .lock()
        .unwrap()
        .get_mut(workspace.codex_thread_id.as_ref().unwrap())
        .unwrap()
        .status = CodexThreadStatus::Active {
        active_flags: vec![],
    };
    let receipt = fixture
        .coordinator
        .delete_workspace_operation(request(&fixture, &workspace))
        .await
        .unwrap();
    assert_eq!(receipt.state, WorkspaceDeletionState::Rejected);
    assert!(workspace.worktree_path.as_ref().unwrap().exists());

    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("hooks.json");
    fs::write(&path, serde_json::to_vec(&json!({"version":1,"guards":[{
        "id":"protect-delete", "action":"workspace.delete", "onError":"deny",
        "command":["/bin/sh","-c","IFS= read -r request || true; printf '%s' '{\"decision\":\"deny\",\"reason\":\"retained by operator\"}'"]
    }]})).unwrap()).unwrap();
    let fixture =
        Fixture::new_with_hooks(FakeWorker::default(), HookRegistry::load(&path).unwrap());
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let receipt = fixture
        .coordinator
        .delete_workspace_operation(request(&fixture, &workspace))
        .await
        .unwrap();
    assert_eq!(receipt.state, WorkspaceDeletionState::Rejected);
    assert_eq!(receipt.error_code.as_deref(), Some("GUARD_DENIED"));
    assert!(workspace.worktree_path.as_ref().unwrap().exists());
}

#[tokio::test]
async fn repeated_concurrent_requests_do_not_dispatch_deletion_twice() {
    let fixture = Fixture::new(FakeWorker::default());
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let params = request(&fixture, &workspace);
    let (a, b) = tokio::join!(
        fixture
            .coordinator
            .delete_workspace_operation(params.clone()),
        fixture
            .coordinator
            .delete_workspace_operation(params.clone()),
    );
    for receipt in [a.unwrap(), b.unwrap()] {
        assert!(matches!(
            receipt.state,
            WorkspaceDeletionState::Preparing
                | WorkspaceDeletionState::Deleting
                | WorkspaceDeletionState::Deleted
        ));
    }
    assert_eq!(
        lookup(&fixture, &params).state,
        WorkspaceDeletionState::Deleted
    );
    assert_eq!(
        fixture
            .worker
            .calls()
            .iter()
            .filter(|call| matches!(call, WorkerCall::DeleteThread { .. }))
            .count(),
        1
    );
}
