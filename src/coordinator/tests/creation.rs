use super::*;
use crate::domain::WorktreeMode;
use crate::git::GitError;

#[tokio::test]
async fn selects_the_git_base_independently_from_workspace_context() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let source = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let source_worktree = source.worktree_path.as_deref().unwrap();
    fs::write(source_worktree.join("context-only.txt"), "source commit\n").unwrap();
    run_git(source_worktree, &["add", "context-only.txt"]);
    run_git(source_worktree, &["commit", "-m", "context-only commit"]);
    let source = fixture
        .coordinator
        .materialize_workspace_thread(source)
        .await
        .unwrap();
    let main_head = git_output(&fixture.source, &["rev-parse", "main"]);

    let created = fixture
        .coordinator
        .create_workspace(WorkspaceCreateParams {
            repository_path: fixture.source.clone(),
            name: "independent".to_owned(),
            context: WorkspaceContextRequest::Fork {
                source: WorkspaceContextSource::Reference {
                    reference: source.name.clone(),
                },
                compact: false,
            },
            worktree: WorkspaceWorktreeRequest::NewBranch {
                branch: None,
                base: WorkspaceBaseRequest::Revision {
                    revision: "main".to_owned(),
                },
            },
            changes: WorkspaceChangesRequest::Reject,
            profile: "default".to_owned(),
            model: None,
            operation_id: "create-independent".to_owned(),
        })
        .await
        .unwrap()
        .workspace;

    let worktree = created.worktree_path.as_deref().unwrap();
    assert_eq!(created.base_sha.as_deref(), Some(main_head.as_str()));
    assert!(created.parent_thread_id.is_none());
    assert_eq!(git_output(worktree, &["rev-parse", "HEAD"]), main_head);
    assert!(!worktree.join("context-only.txt").exists());
    assert_eq!(
        created.context["resolved"]["base"]["requested"],
        json!({"kind": "revision", "revision": "main"})
    );
    assert_eq!(
        created.context["resolved"]["context"]["source"]["workspaceId"],
        source.id
    );
    let materialized = fixture
        .coordinator
        .materialize_workspace_thread(created)
        .await
        .unwrap();
    assert_eq!(materialized.parent_thread_id, source.codex_thread_id);
}

#[tokio::test]
async fn ignores_dirty_source_state_without_changing_the_selected_base() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    run_git(&fixture.source, &["branch", "alternate-base"]);
    fs::write(fixture.source.join("main-only.txt"), "committed on main\n").unwrap();
    run_git(&fixture.source, &["add", "main-only.txt"]);
    run_git(&fixture.source, &["commit", "-m", "advance main"]);
    fs::write(fixture.source.join("README.md"), "local tracked change\n").unwrap();
    fs::write(
        fixture.source.join("local-only.txt"),
        "local untracked file\n",
    )
    .unwrap();
    let source_status = git_output(&fixture.source, &["status", "--porcelain=v1"]);
    let alternate_head = git_output(&fixture.source, &["rev-parse", "alternate-base"]);

    let mut params = fixture.create_params();
    params.name = "ignored-source".to_owned();
    params.operation_id = "create-ignored-source".to_owned();
    params.worktree = WorkspaceWorktreeRequest::NewBranch {
        branch: None,
        base: WorkspaceBaseRequest::Revision {
            revision: "alternate-base".to_owned(),
        },
    };
    params.changes = WorkspaceChangesRequest::Ignore;

    let created = fixture
        .coordinator
        .create_workspace(params)
        .await
        .unwrap()
        .workspace;
    let worktree = created.worktree_path.as_deref().unwrap();

    assert_eq!(git_output(worktree, &["rev-parse", "HEAD"]), alternate_head);
    assert_eq!(
        fs::read_to_string(worktree.join("README.md")).unwrap(),
        "fixture\n"
    );
    assert!(!worktree.join("main-only.txt").exists());
    assert!(!worktree.join("local-only.txt").exists());
    assert!(git_output(worktree, &["status", "--porcelain=v1"]).is_empty());
    assert_eq!(
        git_output(&fixture.source, &["status", "--porcelain=v1"]),
        source_status
    );
    assert_eq!(created.context["request"]["changes"], "ignore");
    assert_eq!(
        created.context["resolved"]["localState"]["carriedTrackedChanges"],
        false
    );
}

#[tokio::test]
async fn forks_context_from_an_exact_native_thread_id_without_a_coco_workspace() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let native_cwd = fixture._temp.path().join("native-thread-cwd");
    fs::create_dir(&native_cwd).unwrap();
    fixture.worker.remember_native_thread(NativeThread {
        id: "native-thread-id".to_owned(),
        cwd: native_cwd.clone(),
        name: Some("external".to_owned()),
        status: CodexThreadStatus::NotLoaded,
        forked_from_id: None,
    });
    let mut params = fixture.create_params();
    params.name = "from-native-thread".to_owned();
    params.operation_id = "create-from-native-thread".to_owned();
    params.context = WorkspaceContextRequest::Fork {
        source: WorkspaceContextSource::Reference {
            reference: "native-thread-id".to_owned(),
        },
        compact: false,
    };

    let created = fixture
        .coordinator
        .create_workspace(params)
        .await
        .unwrap()
        .workspace;

    assert!(created.parent_thread_id.is_none());
    assert_eq!(
        created.context["resolved"]["context"]["source"],
        json!({
            "kind": "thread",
            "threadId": "native-thread-id",
            "cwd": native_cwd,
        })
    );
    assert!(matches!(
        fixture.worker.calls().as_slice(),
        [WorkerCall::Read { thread_id }] if thread_id == "native-thread-id"
    ));
    let materialized = fixture
        .coordinator
        .materialize_workspace_thread(created)
        .await
        .unwrap();
    assert_eq!(
        materialized.parent_thread_id.as_deref(),
        Some("native-thread-id")
    );
    assert!(matches!(
        fixture.worker.calls().as_slice(),
        [
            WorkerCall::Read { thread_id: first },
            WorkerCall::Read { thread_id: second },
            WorkerCall::Fork { source_thread_id, .. },
        ] if first == "native-thread-id"
            && second == "native-thread-id"
            && source_thread_id == "native-thread-id"
    ));
}

#[tokio::test]
async fn explicit_context_prefixes_disambiguate_a_workspace_shaped_thread_id() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let mut source_params = fixture.create_params();
    source_params.name = "shared-reference".to_owned();
    let source = fixture.create_and_materialize(source_params).await;

    let native_cwd = fixture._temp.path().join("explicit-native-thread");
    fs::create_dir(&native_cwd).unwrap();
    fixture.worker.remember_native_thread(NativeThread {
        id: source.name.clone(),
        cwd: native_cwd.clone(),
        name: Some("external".to_owned()),
        status: CodexThreadStatus::NotLoaded,
        forked_from_id: None,
    });
    let mut params = fixture.create_params();
    params.name = "forced-native-context".to_owned();
    params.operation_id = "create-forced-native-context".to_owned();
    params.context = WorkspaceContextRequest::Fork {
        source: WorkspaceContextSource::Reference {
            reference: format!("thread:{}", source.name),
        },
        compact: false,
    };

    let created = fixture
        .coordinator
        .create_workspace(params)
        .await
        .unwrap()
        .workspace;

    assert_eq!(
        created.context["resolved"]["context"]["source"],
        json!({
            "kind": "thread",
            "threadId": "shared-reference",
            "cwd": native_cwd,
        })
    );

    let mut params = fixture.create_params();
    params.name = "forced-workspace-context".to_owned();
    params.operation_id = "create-forced-workspace-context".to_owned();
    params.context = WorkspaceContextRequest::Fork {
        source: WorkspaceContextSource::Reference {
            reference: format!("workspace:{}", source.name),
        },
        compact: false,
    };

    let created = fixture
        .coordinator
        .create_workspace(params)
        .await
        .unwrap()
        .workspace;
    assert_eq!(
        created.context["resolved"]["context"]["source"],
        json!({
            "kind": "workspace",
            "requestedReference": source.name,
            "workspaceId": source.id,
            "workspaceName": "shared-reference",
            "threadId": source.codex_thread_id,
            "cwd": source.worktree_path,
        })
    );
}

#[tokio::test]
async fn reports_when_an_automatic_context_reference_matches_neither_kind() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let mut params = fixture.create_params();
    params.context = WorkspaceContextRequest::Fork {
        source: WorkspaceContextSource::Reference {
            reference: "missing-context".to_owned(),
        },
        compact: false,
    };

    let error = fixture
        .coordinator
        .create_workspace(params)
        .await
        .unwrap_err();
    assert_eq!(error.code(), "CONTEXT_REFERENCE_UNRESOLVED");
    assert!(matches!(
        error,
        CoordinatorError::ContextReferenceUnresolved { reference, source }
            if reference == "missing-context"
                && matches!(source, WorkerError::InvalidThreadRead(_))
    ));
}

#[tokio::test]
async fn supports_detached_and_existing_branch_worktree_bindings() {
    let detached_fixture = Fixture::new(FakeWorker::default());
    detached_fixture.register().await;
    let mut detached_params = detached_fixture.create_params();
    detached_params.name = "detached".to_owned();
    detached_params.operation_id = "create-detached".to_owned();
    detached_params.worktree = WorkspaceWorktreeRequest::Detached {
        base: WorkspaceBaseRequest::Revision {
            revision: "HEAD".to_owned(),
        },
    };
    let detached = detached_fixture
        .coordinator
        .create_workspace(detached_params)
        .await
        .unwrap()
        .workspace;
    assert_eq!(detached.worktree_mode, WorktreeMode::Detached);
    assert_eq!(detached.branch_name, None);
    let detached_path = detached.worktree_path.as_deref().unwrap();
    assert!(
        Command::new("git")
            .current_dir(detached_path)
            .args(["symbolic-ref", "--quiet", "HEAD"])
            .output()
            .unwrap()
            .status
            .code()
            .is_some_and(|code| code != 0)
    );

    let branch_fixture = Fixture::new(FakeWorker::default());
    branch_fixture.register().await;
    run_git(&branch_fixture.source, &["branch", "feat/existing"]);
    let mut branch_params = branch_fixture.create_params();
    branch_params.name = "feat/existing".to_owned();
    branch_params.operation_id = "create-existing".to_owned();
    branch_params.worktree = WorkspaceWorktreeRequest::ExistingBranch {
        branch: "feat/existing".to_owned(),
    };
    let existing = branch_fixture
        .coordinator
        .create_workspace(branch_params)
        .await
        .unwrap()
        .workspace;
    assert_eq!(existing.worktree_mode, WorktreeMode::ExistingBranch);
    assert_eq!(existing.branch_name.as_deref(), Some("feat/existing"));
    assert_eq!(
        git_output(
            existing.worktree_path.as_deref().unwrap(),
            &["symbolic-ref", "--short", "HEAD"],
        ),
        "feat/existing"
    );
}

#[tokio::test]
async fn carries_selected_tracked_untracked_and_ignored_files_without_mutating_the_source() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    fs::write(
        fixture.source.join(".gitignore"),
        ".env\nAGENTS.override.md\n",
    )
    .unwrap();
    fs::write(fixture.source.join(".worktreeinclude"), ".env\n").unwrap();
    run_git(&fixture.source, &["add", ".gitignore", ".worktreeinclude"]);
    run_git(&fixture.source, &["commit", "-m", "configure local files"]);
    fs::write(fixture.source.join("README.md"), "staged\n").unwrap();
    run_git(&fixture.source, &["add", "README.md"]);
    fs::write(fixture.source.join("README.md"), "unstaged\n").unwrap();
    fs::write(fixture.source.join(".env"), "LOCAL_ONLY=yes\n").unwrap();
    fs::write(fixture.source.join("notes.txt"), "ordinary untracked\n").unwrap();
    fs::write(
        fixture.source.join("AGENTS.override.md"),
        "local instructions\n",
    )
    .unwrap();
    let source_status = git_output(&fixture.source, &["status", "--porcelain=v1"]);
    let mut params = fixture.create_params();
    params.name = "dirty-copy".to_owned();
    params.operation_id = "create-dirty-copy".to_owned();
    params.worktree = WorkspaceWorktreeRequest::Detached {
        base: WorkspaceBaseRequest::Revision {
            revision: "HEAD".to_owned(),
        },
    };
    params.changes = WorkspaceChangesRequest::CarryTrackedAndUntracked;

    let created = fixture
        .coordinator
        .create_workspace(params)
        .await
        .unwrap()
        .workspace;
    let worktree = created.worktree_path.as_deref().unwrap();

    assert_eq!(git_output(worktree, &["show", ":README.md"]), "staged");
    assert_eq!(
        fs::read_to_string(worktree.join("README.md")).unwrap(),
        "unstaged\n"
    );
    assert_eq!(
        fs::read_to_string(worktree.join(".env")).unwrap(),
        "LOCAL_ONLY=yes\n"
    );
    assert_eq!(
        fs::read_to_string(worktree.join("notes.txt")).unwrap(),
        "ordinary untracked\n"
    );
    assert!(worktree.join("AGENTS.override.md").is_file());
    assert_eq!(
        git_output(&fixture.source, &["status", "--porcelain=v1"]),
        source_status
    );
    assert_eq!(
        created.context["resolved"]["localState"]["includedFileCount"],
        2
    );
    assert_eq!(
        created.context["resolved"]["localState"]["untrackedFileCount"],
        1
    );
    assert_eq!(
        created.context["resolved"]["localState"]["sourcePath"],
        json!(fixture.source)
    );
    assert_eq!(
        created.context["resolved"]["localState"]["carriedTrackedChanges"],
        true
    );
}

#[tokio::test]
async fn rejects_unsafe_carry_requests_before_persisting_or_starting_a_thread() {
    let fixture = Fixture::new(FakeWorker::default());
    let repository = fixture.register().await;
    fs::write(fixture.source.join("untracked.txt"), "untracked\n").unwrap();
    let mut params = fixture.create_params();
    params.name = "unsafe-carry".to_owned();
    params.operation_id = "create-unsafe-carry".to_owned();
    params.worktree = WorkspaceWorktreeRequest::Detached {
        base: WorkspaceBaseRequest::Revision {
            revision: "HEAD".to_owned(),
        },
    };
    params.changes = WorkspaceChangesRequest::CarryTracked;

    assert!(matches!(
        fixture.coordinator.create_workspace(params).await,
        Err(CoordinatorError::Git(GitError::UntrackedChanges(path)))
            if path.as_path() == Path::new("untracked.txt")
    ));
    assert!(
        fixture
            .store
            .workspace_by_name(&repository.id, "unsafe-carry")
            .unwrap()
            .is_none()
    );
    assert!(fixture.worker.calls().is_empty());
}
