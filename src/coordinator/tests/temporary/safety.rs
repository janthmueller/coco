use super::*;

#[tokio::test]
async fn close_and_delete_guards_can_prevent_temporary_cleanup() {
    for action in ["workspace.close", "workspace.delete"] {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("hooks.json");
        fs::write(&config, serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "guards": [{
                "id": "protect-temporary", "action": action,
                "command": ["/bin/sh", "-c", "IFS= read -r request || true; printf '%s' '{\"decision\":\"deny\",\"reason\":\"keep workspace\"}'"],
                "onError": "deny"
            }]
        })).unwrap()).unwrap();
        let hooks = HookRegistry::load(&config).unwrap();
        let fixture = Fixture::new_with_hooks(FakeWorker::default(), hooks);
        let workspace = temporary(&fixture, true, false).await;
        let finish = attach(&fixture, &workspace).await;
        let outcome = fixture
            .coordinator
            .finish_workspace_attach(finish)
            .await
            .unwrap()
            .cleanup;
        assert!(
            matches!(outcome, Some(WorkspaceCleanupResult::Retained { reason }) if reason.contains("guard"))
        );
        assert!(workspace.worktree_path.unwrap().is_dir());
        assert!(
            fixture
                .worker
                .native_threads
                .lock()
                .unwrap()
                .contains_key(workspace.codex_thread_id.as_ref().unwrap())
        );
        assert!(
            fixture
                .store
                .workspace_by_id(&workspace.id)
                .unwrap()
                .unwrap()
                .temporary
                .is_none()
        );
    }
}

#[tokio::test]
async fn adopted_branches_are_always_retained() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    run_git(&fixture.source, &["branch", "existing-work"]);
    let mut params = fixture.create_params();
    params.temporary = Some(TemporaryWorkspacePolicy { keep_thread: false });
    params.worktree = WorkspaceWorktreeRequest::ExistingBranch {
        branch: "existing-work".into(),
    };
    let workspace = fixture.create_and_materialize(params).await;
    let finish = attach(&fixture, &workspace).await;
    assert_eq!(
        fixture
            .coordinator
            .finish_workspace_attach(finish)
            .await
            .unwrap()
            .cleanup,
        Some(WorkspaceCleanupResult::Deleted)
    );
    assert!(!workspace.worktree_path.unwrap().exists());
    assert!(!git_output(&fixture.source, &["rev-parse", "refs/heads/existing-work"]).is_empty());
}

#[tokio::test]
async fn tracked_untracked_and_ignored_files_are_retained_and_disable_cleanup() {
    for kind in ["tracked", "untracked", "ignored"] {
        let fixture = Fixture::new(FakeWorker::default());
        let workspace = temporary(&fixture, true, false).await;
        let finish = attach(&fixture, &workspace).await;
        let path = workspace.worktree_path.as_ref().unwrap();
        let file = match kind {
            "tracked" => path.join("README.md"),
            "ignored" => {
                fs::write(fixture.source.join(".git/info/exclude"), "*.ignored\n").unwrap();
                path.join("valuable.ignored")
            }
            _ => path.join("valuable-untracked.txt"),
        };
        fs::write(&file, "keep this content\n").unwrap();
        let outcome = fixture
            .coordinator
            .finish_workspace_attach(finish)
            .await
            .unwrap()
            .cleanup;
        assert!(matches!(
            outcome,
            Some(WorkspaceCleanupResult::Retained { .. })
        ));
        assert_eq!(fs::read_to_string(&file).unwrap(), "keep this content\n");
        assert!(
            fixture
                .store
                .workspace_by_id(&workspace.id)
                .unwrap()
                .unwrap()
                .temporary
                .is_none()
        );
        if kind == "tracked" {
            fs::write(&file, "fixture\n").unwrap();
        } else {
            fs::remove_file(&file).unwrap();
        }
        let again = attach(&fixture, &workspace).await;
        assert_eq!(
            fixture
                .coordinator
                .finish_workspace_attach(again)
                .await
                .unwrap()
                .cleanup,
            None
        );
        assert!(path.is_dir());
    }
}

#[tokio::test]
async fn active_turns_survive_tui_exit_without_interrupt_or_delayed_deletion() {
    let fixture = Fixture::new(FakeWorker::default());
    let workspace = temporary(&fixture, true, false).await;
    let finish = attach(&fixture, &workspace).await;
    let thread = workspace.codex_thread_id.as_ref().unwrap();
    fixture
        .worker
        .native_threads
        .lock()
        .unwrap()
        .get_mut(thread)
        .unwrap()
        .status = CodexThreadStatus::Active {
        active_flags: vec![],
    };
    let result = fixture
        .coordinator
        .finish_workspace_attach(finish)
        .await
        .unwrap();
    assert!(matches!(
        result.cleanup,
        Some(WorkspaceCleanupResult::Retained { .. })
    ));
    assert!(workspace.worktree_path.as_ref().unwrap().exists());
    assert!(
        fixture
            .worker
            .native_threads
            .lock()
            .unwrap()
            .contains_key(thread)
    );
    assert!(
        fixture
            .store
            .workspace_by_id(&workspace.id)
            .unwrap()
            .unwrap()
            .temporary
            .is_none()
    );
}

#[tokio::test]
async fn commits_without_another_reference_are_not_discarded() {
    let fixture = Fixture::new(FakeWorker::default());
    let workspace = temporary(&fixture, true, false).await;
    let finish = attach(&fixture, &workspace).await;
    let path = workspace.worktree_path.as_ref().unwrap();
    fs::write(path.join("README.md"), "important commit\n").unwrap();
    run_git(path, &["add", "README.md"]);
    run_git(path, &["commit", "-m", "unretained work"]);
    let head = git_output(path, &["rev-parse", "HEAD"]);
    let outcome = fixture
        .coordinator
        .finish_workspace_attach(finish)
        .await
        .unwrap()
        .cleanup;
    assert!(
        matches!(outcome, Some(WorkspaceCleanupResult::Retained { reason }) if reason.contains("commits"))
    );
    assert_eq!(git_output(path, &["rev-parse", "HEAD"]), head);
}

#[tokio::test]
async fn native_context_dependencies_block_automatic_thread_deletion() {
    let fixture = Fixture::new(FakeWorker::default());
    let workspace = temporary(&fixture, true, false).await;
    let child = fixture
        .coordinator
        .create_workspace(fixture.fork_params(&workspace, "child", false))
        .await
        .unwrap()
        .workspace;
    let finish = attach(&fixture, &workspace).await;
    let outcome = fixture
        .coordinator
        .finish_workspace_attach(finish)
        .await
        .unwrap()
        .cleanup;
    assert!(matches!(
        outcome,
        Some(WorkspaceCleanupResult::Retained { .. })
    ));
    assert!(workspace.worktree_path.unwrap().exists());
    assert!(child.worktree_path.unwrap().exists());
}
