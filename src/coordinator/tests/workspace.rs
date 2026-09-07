use super::*;

#[tokio::test]
async fn lists_the_app_server_model_catalog_without_repository_state() {
    let fixture = Fixture::new(FakeWorker::default());

    let models = fixture.coordinator.list_models().await.unwrap();

    assert_eq!(models.len(), 1);
    assert_eq!(models[0].model, "gpt-test");
    assert!(models[0].is_default);
    assert_eq!(fixture.worker.calls(), [WorkerCall::Models]);
}

#[tokio::test]
async fn prepares_an_idle_workspace_without_starting_a_turn_and_replays_operation_ids() {
    let fixture = Fixture::new(FakeWorker::default());
    let repository = fixture.register().await;

    let created = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap();
    let workspace = created.workspace.clone();
    assert_eq!(workspace.lifecycle, WorkspaceLifecycle::Ready);
    assert_eq!(workspace.phase, WorkspacePhase::Idle);
    assert_eq!(
        workspace
            .thread_runtime
            .as_ref()
            .map(|snapshot| &snapshot.status),
        Some(&CodexThreadStatus::Idle)
    );
    assert!(workspace.thread_runtime.as_ref().unwrap().is_fresh);
    assert_eq!(workspace.codex_thread_id.as_deref(), Some("thread-1"));
    assert!(created.turn_id.is_none());
    assert_eq!(workspace.profile.effective_settings["model"], "gpt-test");
    let worktree = workspace.worktree_path.as_deref().unwrap();
    assert!(worktree.starts_with(fixture.worktrees.join(&repository.id)));
    assert!(worktree.join("README.md").is_file());

    let calls = fixture.worker.calls();
    assert_eq!(calls.len(), 1);
    assert!(matches!(
        &calls[0],
        WorkerCall::Thread {
            name,
            cwd,
            config,
            model,
        } if name == "first-workspace"
            && cwd == worktree
            && config == &json!({})
            && model.is_none()
    ));
    let replay = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap();
    assert_eq!(replay.workspace.id, workspace.id);
    assert_eq!(replay.workspace.phase, WorkspacePhase::Idle);
    assert!(matches!(
        fixture.worker.calls().as_slice(),
        [WorkerCall::Thread { .. }, WorkerCall::Read { thread_id }]
            if thread_id == "thread-1"
    ));

    let mut conflict = fixture.create_params();
    conflict.worktree = WorkspaceWorktreeRequest::NewBranch {
        branch: None,
        base: WorkspaceBaseRequest::Revision {
            revision: "different-base".to_owned(),
        },
    };
    assert!(matches!(
        fixture.coordinator.create_workspace(conflict).await,
        Err(CoordinatorError::IdempotencyConflict)
    ));

    let events = fixture.store.events_after(Some(&workspace.id), 0).unwrap();
    assert_eq!(
        events.iter().map(|event| event.kind).collect::<Vec<_>>(),
        [
            EventKind::WorkspaceCreated,
            EventKind::WorktreeCreated,
            EventKind::AgentStarted,
        ]
    );
}

#[tokio::test]
async fn passes_an_explicit_model_separately_from_the_selected_profile() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    fs::create_dir_all(&fixture.codex_home).unwrap();
    fs::write(
        fixture.codex_home.join("dev.config.toml"),
        "model = \"gpt-profile\"\n",
    )
    .unwrap();
    let mut params = fixture.create_params();
    params.profile = "dev".to_owned();
    params.model = Some("gpt-explicit".to_owned());

    let created = fixture
        .coordinator
        .create_workspace(params.clone())
        .await
        .unwrap();

    assert_eq!(
        created.workspace.profile.model_override.as_deref(),
        Some("gpt-explicit")
    );
    assert_eq!(
        created.workspace.profile.effective_settings["model"],
        "gpt-explicit"
    );
    assert!(matches!(
        fixture.worker.calls().as_slice(),
        [WorkerCall::Thread { config, model, .. }]
            if config == &json!({"model": "gpt-profile"})
                && model.as_deref() == Some("gpt-explicit")
    ));

    params.model = Some("gpt-other".to_owned());
    assert!(matches!(
        fixture.coordinator.create_workspace(params).await,
        Err(CoordinatorError::IdempotencyConflict)
    ));
    assert_eq!(fixture.worker.calls().len(), 1);
}

#[tokio::test]
async fn preserves_the_worktree_and_marks_the_workspace_failed_after_worker_failure() {
    let fixture = Fixture::new(FakeWorker::failing_thread_start());
    let repository = fixture.register().await;

    assert!(matches!(
        fixture
            .coordinator
            .create_workspace(fixture.create_params())
            .await,
        Err(CoordinatorError::Worker(WorkerError::Runtime(_)))
    ));
    let workspace = fixture
        .store
        .workspace_by_name(&repository.id, "first-workspace")
        .unwrap()
        .unwrap();
    assert_eq!(workspace.phase, WorkspacePhase::Failed);
    assert_eq!(workspace.lifecycle, WorkspaceLifecycle::Failed);
    assert_eq!(workspace.last_error_code.as_deref(), Some("CODEX_ERROR"));
    assert!(workspace.worktree_path.unwrap().is_dir());
    assert_eq!(fixture.worker.calls().len(), 1);
}

#[tokio::test]
async fn passive_native_idle_preserves_the_local_mutation_guard() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    fixture
        .coordinator
        .start_turn(TurnStartParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id.clone(),
            message: "leave a stale local active turn".to_owned(),
            operation_id: "stale-local-turn".to_owned(),
        })
        .await
        .unwrap();
    let operation = fixture
        .store
        .operation_by_client_id("stale-local-turn")
        .unwrap()
        .unwrap();
    assert_eq!(operation.state, OperationState::Accepted);
    fixture
        .worker
        .set_native_status("thread-1", CodexThreadStatus::Idle);

    let shown = fixture
        .coordinator
        .get_workspace(WorkspaceGetParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id.clone(),
        })
        .await
        .unwrap();

    assert_eq!(shown.workspace.phase, WorkspacePhase::Active);
    assert!(shown.workspace.wait_reasons.is_empty());
    assert!(shown.workspace.active_turn_id.is_some());
    assert_eq!(
        shown
            .workspace
            .thread_runtime
            .as_ref()
            .map(|runtime| &runtime.status),
        Some(&CodexThreadStatus::Idle)
    );
    let persisted = fixture
        .store
        .workspace_by_id(&workspace.id)
        .unwrap()
        .unwrap();
    assert_eq!(persisted.phase, WorkspacePhase::Unavailable);
    assert!(persisted.active_turn_id.is_none());
    assert!(matches!(
        fixture.worker.calls().last(),
        Some(WorkerCall::Read { thread_id }) if thread_id == "thread-1"
    ));

    let calls_before_mutations = fixture.worker.calls().len();
    assert!(matches!(
        fixture
            .coordinator
            .start_turn(TurnStartParams {
                scope: RepositoryScope::repository(fixture.source.clone()),
                workspace: workspace.id.clone(),
                message: "must not race the accepted turn".to_owned(),
                operation_id: "blocked-by-local-turn".to_owned(),
            })
            .await,
        Err(CoordinatorError::InvalidWorkspaceState {
            expected: "idle",
            actual: WorkspacePhase::Active,
        })
    ));
    assert!(matches!(
        fixture
            .coordinator
            .create_workspace(fixture.fork_params(&workspace, "blocked-fork", false))
            .await,
        Err(CoordinatorError::InvalidWorkspaceState {
            expected: "an idle or unloaded source workspace",
            actual: WorkspacePhase::Active,
        })
    ));
    assert_eq!(fixture.worker.calls().len(), calls_before_mutations);
}

#[tokio::test]
async fn native_thread_read_failure_projects_unavailable_without_serving_or_persisting_stale_state()
{
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    fixture.worker.fail_thread_read("thread-1");

    let shown = fixture
        .coordinator
        .get_workspace(WorkspaceGetParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id.clone(),
        })
        .await
        .unwrap();

    assert_eq!(shown.workspace.phase, WorkspacePhase::Unavailable);
    assert!(shown.workspace.thread_runtime.is_none());
    assert!(shown.workspace.wait_reasons.is_empty());
    let persisted = fixture
        .store
        .workspace_by_id(&workspace.id)
        .unwrap()
        .unwrap();
    assert_eq!(persisted.phase, WorkspacePhase::Unavailable);
    assert!(persisted.thread_runtime.is_none());
}

#[tokio::test]
async fn failed_lifecycle_remains_failed_without_a_native_thread_read() {
    let fixture = Fixture::new(FakeWorker::failing_thread_start());
    let repository = fixture.register().await;
    fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap_err();
    let failed = fixture
        .store
        .workspace_by_name(&repository.id, "first-workspace")
        .unwrap()
        .unwrap();

    let shown = fixture
        .coordinator
        .get_workspace(WorkspaceGetParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: failed.id,
        })
        .await
        .unwrap();

    assert_eq!(shown.workspace.phase, WorkspacePhase::Failed);
    assert_eq!(shown.workspace.lifecycle, WorkspaceLifecycle::Failed);
    assert!(
        fixture
            .worker
            .calls()
            .iter()
            .all(|call| !matches!(call, WorkerCall::Read { .. }))
    );
}

#[tokio::test]
async fn workspace_list_filters_after_hydrating_every_native_phase() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let waiting = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let mut second = fixture.create_params();
    second.name = "second-workspace".to_owned();
    second.operation_id = "create-operation-2".to_owned();
    fixture.coordinator.create_workspace(second).await.unwrap();
    fixture.worker.set_native_status(
        "thread-1",
        CodexThreadStatus::Active {
            active_flags: vec!["waitingOnUserInput".to_owned()],
        },
    );

    let listed = fixture
        .coordinator
        .list_workspaces(WorkspaceListParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            phases: Some(vec!["waiting_for_input".to_owned()]),
        })
        .await
        .unwrap();

    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].workspace.id, waiting.id);
    assert_eq!(
        listed[0].workspace.wait_reasons,
        [WorkspaceWaitReason::UserInput]
    );
    let reads = fixture
        .worker
        .calls()
        .into_iter()
        .filter(|call| matches!(call, WorkerCall::Read { .. }))
        .count();
    assert_eq!(reads, 2);
}

#[tokio::test]
async fn event_list_uses_one_metadata_only_native_read() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let calls_before = fixture.worker.calls().len();

    let result = fixture
        .coordinator
        .list_events(EventListParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id,
            after_sequence: 0,
        })
        .await
        .unwrap();

    assert_eq!(result.workspace.phase, WorkspacePhase::Idle);
    assert_eq!(
        &fixture.worker.calls()[calls_before..],
        [WorkerCall::Read {
            thread_id: "thread-1".to_owned(),
        }]
    );
}

#[tokio::test]
async fn serves_repository_views_events_and_bounded_diffs() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let created = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap();
    let workspace = created.workspace;
    let worktree = workspace.worktree_path.as_deref().unwrap();
    fs::write(worktree.join("new.txt"), "new content\n").unwrap();

    let listed = fixture
        .coordinator
        .list_workspaces(WorkspaceListParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            phases: Some(vec!["idle".to_owned()]),
        })
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);

    let shown = fixture
        .coordinator
        .get_workspace(WorkspaceGetParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id.clone(),
        })
        .await
        .unwrap();
    assert!(matches!(
        shown.git,
        WorkspaceGitStatus::Observed(ref observation) if observation.observed && observation.dirty
    ));

    let events = fixture
        .coordinator
        .list_events(EventListParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: "first-workspace".to_owned(),
            after_sequence: 0,
        })
        .await
        .unwrap();
    assert_eq!(events.events.len(), 3);

    let diff = fixture
        .coordinator
        .workspace_diff(WorkspaceDiffParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: "first-workspace".to_owned(),
            max_bytes: Some(16),
        })
        .unwrap();
    assert_eq!(diff.untracked_paths, [PathBuf::from("new.txt")]);
}

struct MultiRepositorySetup {
    first_repository: Repository,
    second_repository: Repository,
    second_source: PathBuf,
    first: Workspace,
    second: Workspace,
    unique: Workspace,
}

async fn prepare_multi_repository_workspaces(fixture: &Fixture) -> MultiRepositorySetup {
    let first_repository = fixture.register().await;
    let second_source = fixture._temp.path().join("source-two");
    initialize_repository(&second_source);
    let second_repository = fixture
        .coordinator
        .register_repository(RepositoryRegisterParams {
            path: second_source.clone(),
        })
        .unwrap();

    let first = fixture
        .coordinator
        .create_workspace(fresh_create_params(
            fixture.source.clone(),
            "feat/shared",
            "create-first-shared",
        ))
        .await
        .unwrap()
        .workspace;
    let second = fixture
        .coordinator
        .create_workspace(fresh_create_params(
            second_source.clone(),
            "feat/shared",
            "create-second-shared",
        ))
        .await
        .unwrap()
        .workspace;
    let unique = fixture
        .coordinator
        .create_workspace(fresh_create_params(
            second_source.clone(),
            "fix/unique",
            "create-second-unique",
        ))
        .await
        .unwrap()
        .workspace;

    MultiRepositorySetup {
        first_repository,
        second_repository,
        second_source,
        first,
        second,
        unique,
    }
}

#[tokio::test]
async fn resolves_a_registered_repository_from_an_inside_path() {
    let fixture = Fixture::new(FakeWorker::default());
    let repository = fixture.register().await;
    let inside = fixture.source.join("nested");
    fs::create_dir(&inside).unwrap();

    let resolved = fixture
        .coordinator
        .resolve_repository(crate::protocol::RepositoryResolveParams { path: inside })
        .unwrap();

    assert_eq!(resolved.id, repository.id);
    assert_eq!(resolved.root_path, fixture.source);
}

#[tokio::test]
async fn scopes_workspace_names_to_repositories_and_resolves_global_references() {
    let fixture = Fixture::new(FakeWorker::default());
    let MultiRepositorySetup {
        first_repository,
        second_repository,
        second_source,
        first,
        second,
        unique,
    } = prepare_multi_repository_workspaces(&fixture).await;

    let repositories = fixture
        .coordinator
        .list_repositories(crate::protocol::RepositoryListParams {})
        .unwrap();
    assert_eq!(repositories.len(), 2);
    assert!(
        repositories
            .iter()
            .any(|item| item.id == first_repository.id)
    );
    assert!(
        repositories
            .iter()
            .any(|item| item.id == second_repository.id)
    );
    let listed = fixture
        .coordinator
        .list_workspaces(WorkspaceListParams {
            scope: RepositoryScope::AllRepositories,
            phases: None,
        })
        .await
        .unwrap();
    assert_eq!(listed.len(), 3);
    assert!(listed.iter().all(|item| {
        item.repository.id == item.workspace.repository_id
            && [fixture.source.as_path(), second_source.as_path()]
                .contains(&item.repository.root_path.as_path())
    }));

    let local = fixture
        .coordinator
        .get_workspace(WorkspaceGetParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: "feat/shared".to_owned(),
        })
        .await
        .unwrap();
    assert_eq!(local.workspace.id, first.id);

    let global_id = fixture
        .coordinator
        .get_workspace(WorkspaceGetParams {
            scope: RepositoryScope::AllRepositories,
            workspace: second.id.clone(),
        })
        .await
        .unwrap();
    assert_eq!(global_id.workspace.id, second.id);

    let global_unique = fixture
        .coordinator
        .get_workspace(WorkspaceGetParams {
            scope: RepositoryScope::AllRepositories,
            workspace: unique.name.clone(),
        })
        .await
        .unwrap();
    assert_eq!(global_unique.workspace.id, unique.id);

    let ambiguous = fixture
        .coordinator
        .get_workspace(WorkspaceGetParams {
            scope: RepositoryScope::AllRepositories,
            workspace: "feat/shared".to_owned(),
        })
        .await
        .unwrap_err();
    assert_error_match_count(&ambiguous, "WORKSPACE_REFERENCE_AMBIGUOUS", 2);
    let CoordinatorError::WorkspaceReferenceAmbiguous { candidates, .. } = ambiguous else {
        panic!("global duplicate name did not produce an ambiguity error");
    };
    assert_eq!(candidates.len(), 2);
    assert!(candidates.iter().any(|item| item.workspace_id == first.id));
    assert!(candidates.iter().any(|item| item.workspace_id == second.id));

    let local_miss = fixture
        .coordinator
        .get_workspace(WorkspaceGetParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: unique.name.clone(),
        })
        .await
        .unwrap_err();
    assert_error_match_count(&local_miss, "WORKSPACE_NOT_FOUND", 1);
    let CoordinatorError::WorkspaceNotFound { candidates, .. } = local_miss else {
        panic!("repository-local miss did not remain a not-found error");
    };
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].workspace_id, unique.id);
    assert_eq!(candidates[0].repository_path, second_source);
}

fn assert_error_match_count(error: &CoordinatorError, code: &str, expected: usize) {
    assert_eq!(error.code(), code);
    assert_eq!(
        error
            .data()
            .and_then(|data| data["matches"].as_array().map(Vec::len)),
        Some(expected)
    );
}
