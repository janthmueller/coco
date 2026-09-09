use super::*;

#[tokio::test]
async fn forks_committed_code_and_codex_history_from_an_idle_workspace() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let source = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let source_worktree = source.worktree_path.as_deref().unwrap();
    fs::write(source_worktree.join("source-commit.txt"), "from source\n").unwrap();
    run_git(source_worktree, &["add", "source-commit.txt"]);
    run_git(
        source_worktree,
        &["commit", "-m", "source workspace commit"],
    );
    let source_head = git_output(source_worktree, &["rev-parse", "HEAD"]);
    assert_ne!(
        source_head,
        git_output(&fixture.source, &["rev-parse", "HEAD"])
    );

    let fork_params = fixture.fork_params(&source, "forked-workspace", false);
    let created = fixture
        .coordinator
        .create_workspace(fork_params.clone())
        .await
        .unwrap();
    let prepared = created.workspace;
    assert_eq!(prepared.phase, WorkspacePhase::Prepared);
    assert!(prepared.codex_thread_id.is_none());
    assert!(prepared.parent_thread_id.is_none());
    let workspace = fixture.attach(&prepared).await;
    let target_worktree = workspace.worktree_path.as_deref().unwrap();

    assert_eq!(workspace.lifecycle, WorkspaceLifecycle::Ready);
    assert_eq!(workspace.phase, WorkspacePhase::Idle);
    assert_eq!(workspace.context_mode, ContextMode::Fork);
    assert_eq!(workspace.base_sha.as_deref(), Some(source_head.as_str()));
    assert_eq!(workspace.parent_thread_id, source.codex_thread_id);
    assert_eq!(workspace.codex_thread_id.as_deref(), Some("fork-thread-1"));
    assert_eq!(workspace.context["resolved"]["context"]["mode"], "fork");
    assert_eq!(
        workspace.context["resolved"]["context"]["source"]["requestedReference"],
        source.name
    );
    assert_eq!(
        workspace.context["resolved"]["context"]["source"]["workspaceId"],
        source.id
    );
    assert_eq!(
        workspace.context["resolved"]["base"]["baseSha"],
        source_head
    );
    assert_eq!(workspace.context["resolved"]["context"]["compact"], false);
    assert_eq!(
        git_output(target_worktree, &["rev-parse", "HEAD"]),
        source_head
    );
    assert_eq!(
        fs::read_to_string(target_worktree.join("source-commit.txt")).unwrap(),
        "from source\n"
    );
    assert!(matches!(
        fixture.worker.calls().as_slice(),
        [
            WorkerCall::Thread { .. },
            WorkerCall::Read { thread_id },
            WorkerCall::Read { thread_id: validation_thread_id },
            WorkerCall::Fork { source_thread_id, cwd, config, .. },
            WorkerCall::Read { thread_id: child_thread_id },
        ] if thread_id == "thread-1"
            && validation_thread_id == "thread-1"
            && source_thread_id == "thread-1"
            && child_thread_id == "fork-thread-1"
            && cwd == target_worktree
            && config == &json!({})
    ));
    let replay = fixture
        .coordinator
        .create_workspace(fork_params)
        .await
        .unwrap();
    assert_eq!(replay.workspace.id, workspace.id);
    assert_eq!(replay.workspace.phase, WorkspacePhase::Idle);
    assert!(matches!(
        fixture.worker.calls().last(),
        Some(WorkerCall::Read { thread_id }) if thread_id == "fork-thread-1"
    ));
}

#[tokio::test]
async fn ignores_uncommitted_context_files_but_rejects_an_active_context_source() {
    let dirty_fixture = Fixture::new(FakeWorker::default());
    let repository = dirty_fixture.register().await;
    let source = dirty_fixture
        .create_and_materialize(dirty_fixture.create_params())
        .await;
    fs::write(
        source.worktree_path.as_deref().unwrap().join("dirty.txt"),
        "not committed\n",
    )
    .unwrap();

    let child = dirty_fixture
        .coordinator
        .create_workspace(dirty_fixture.fork_params(&source, "dirty-child", false))
        .await
        .unwrap()
        .workspace;
    assert!(!child.worktree_path.unwrap().join("dirty.txt").exists());
    assert!(
        dirty_fixture
            .store
            .workspace_by_name(&repository.id, "dirty-child")
            .unwrap()
            .is_some()
    );

    let active_fixture = Fixture::new(FakeWorker::default());
    active_fixture.register().await;
    let source = active_fixture
        .coordinator
        .create_workspace(active_fixture.create_params())
        .await
        .unwrap()
        .workspace;
    active_fixture
        .coordinator
        .start_turn(TurnStartParams {
            scope: RepositoryScope::repository(active_fixture.source.clone()),
            workspace: source.name.clone(),
            message: "keep working".to_owned(),
            operation_id: "activate-source".to_owned(),
        })
        .await
        .unwrap();
    assert!(matches!(
        active_fixture
            .coordinator
            .create_workspace(active_fixture.fork_params(&source, "active-child", false))
            .await,
        Err(CoordinatorError::InvalidWorkspaceState {
            expected: "an idle or unloaded source workspace",
            actual: WorkspacePhase::Active,
        })
    ));
    assert_eq!(active_fixture.worker.calls().len(), 2);
}

#[tokio::test]
async fn compacts_only_the_child_before_it_accepts_a_message() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let source = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let prepared = fixture
        .coordinator
        .create_workspace(fixture.fork_params(&source, "compact-child", true))
        .await
        .unwrap()
        .workspace;
    let attach = fixture.coordinator.attach_workspace(WorkspaceAttachParams {
        scope: RepositoryScope::repository(fixture.source.clone()),
        workspace: prepared.id,
    });
    let complete_compaction = complete_fake_compaction(&fixture, "fork-thread-1");
    let (attached, ()) = tokio::join!(attach, complete_compaction);
    let attached = attached.unwrap();
    let lease_id = match &attached.launch {
        WorkspaceAttachLaunch::Resume { lease_id, .. } => lease_id.clone(),
        WorkspaceAttachLaunch::Start { .. } => panic!("fork attach must resume its thread"),
    };
    let workspace = attached.workspace;

    assert_eq!(workspace.lifecycle, WorkspaceLifecycle::Ready);
    assert_eq!(workspace.phase, WorkspacePhase::Idle);
    assert_eq!(workspace.context["resolved"]["context"]["compact"], true);
    assert!(matches!(
        fixture.worker.calls().as_slice(),
        [
            WorkerCall::Thread { .. },
            WorkerCall::Read { .. },
            WorkerCall::Read { .. },
            WorkerCall::Fork { .. },
            WorkerCall::Compact { thread_id },
            WorkerCall::Read { thread_id: hydrated_thread_id },
            WorkerCall::Read { thread_id: attached_thread_id },
        ] if thread_id == "fork-thread-1"
            && hydrated_thread_id == "fork-thread-1"
            && attached_thread_id == "fork-thread-1"
    ));
    let events = fixture.store.events_after(Some(&workspace.id), 0).unwrap();
    assert_eq!(
        events.iter().map(|event| event.kind).collect::<Vec<_>>(),
        [
            EventKind::WorkspaceCreated,
            EventKind::WorktreeCreated,
            EventKind::AgentStarted,
            EventKind::ContextCompacted,
        ]
    );

    fixture
        .coordinator
        .release_workspace_attach(WorkspaceAttachReleaseParams {
            workspace_id: workspace.id.clone(),
            lease_id,
        })
        .unwrap();

    fixture
        .coordinator
        .start_turn(TurnStartParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id.clone(),
            message: "continue in the child".to_owned(),
            operation_id: "send-after-compact".to_owned(),
        })
        .await
        .unwrap();
    let calls = fixture.worker.calls();
    let WorkerCall::Turn {
        additional_context: Some(additional_context),
        ..
    } = calls.last().unwrap()
    else {
        panic!("forked turn did not receive the workspace boundary context");
    };
    let binding = additional_context["coco.workspace-binding"]["value"]
        .as_str()
        .and_then(|value| serde_json::from_str::<Value>(value).ok())
        .expect("workspace boundary context was not valid JSON");
    assert_eq!(binding["workspaceId"], workspace.id);
    assert_eq!(binding["worktreePath"], json!(workspace.worktree_path));
    assert_eq!(binding["sourceWorkspaceId"], source.id);
}

async fn complete_fake_compaction(fixture: &Fixture, thread_id: &str) {
    wait_for_compaction_request(fixture).await;
    for event in [
        CodexEvent::Notification {
            method: "turn/started".to_owned(),
            params: json!({
                "threadId": thread_id,
                "turn": {"id": "compact-turn", "status": "inProgress"},
            }),
        },
        CodexEvent::Notification {
            method: "item/completed".to_owned(),
            params: json!({
                "threadId": thread_id,
                "turnId": "compact-turn",
                "item": {"id": "compact-item", "type": "contextCompaction"},
            }),
        },
        CodexEvent::Notification {
            method: "turn/completed".to_owned(),
            params: json!({
                "threadId": thread_id,
                "turn": {"id": "compact-turn", "status": "completed"},
            }),
        },
    ] {
        fixture.coordinator.record_codex_event(event).unwrap();
    }
}

async fn wait_for_compaction_request(fixture: &Fixture) {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if fixture
                .worker
                .calls()
                .iter()
                .any(|call| matches!(call, WorkerCall::Compact { .. }))
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("coordinator did not request child compaction");
}

#[tokio::test]
async fn retains_a_bound_failed_child_when_compaction_cannot_start() {
    let fixture = Fixture::new(FakeWorker::failing_compact());
    let repository = fixture.register().await;
    let source = fixture
        .create_and_materialize(fixture.create_params())
        .await;

    let prepared = fixture
        .coordinator
        .create_workspace(fixture.fork_params(&source, "failed-compact", true))
        .await
        .unwrap()
        .workspace;

    assert!(matches!(
        fixture
            .coordinator
            .attach_workspace(WorkspaceAttachParams {
                scope: RepositoryScope::repository(fixture.source.clone()),
                workspace: prepared.id,
            })
            .await,
        Err(CoordinatorError::CompactionFailed(_))
    ));
    let failed = fixture
        .store
        .workspace_by_name(&repository.id, "failed-compact")
        .unwrap()
        .unwrap();
    assert_eq!(failed.lifecycle, WorkspaceLifecycle::Failed);
    assert_eq!(failed.phase, WorkspacePhase::Failed);
    assert_eq!(
        failed.last_error_code.as_deref(),
        Some("CODEX_COMPACTION_FAILED")
    );
    assert_eq!(failed.codex_thread_id.as_deref(), Some("fork-thread-1"));
    assert_eq!(failed.parent_thread_id, source.codex_thread_id);
    assert!(failed.worktree_path.unwrap().is_dir());
}

#[tokio::test]
async fn retains_a_bound_failed_child_when_native_compaction_fails() {
    let fixture = Fixture::new(FakeWorker::default());
    let repository = fixture.register().await;
    let source = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let prepared = fixture
        .coordinator
        .create_workspace(fixture.fork_params(&source, "failed-native-compact", true))
        .await
        .unwrap()
        .workspace;
    let attach = fixture.coordinator.attach_workspace(WorkspaceAttachParams {
        scope: RepositoryScope::repository(fixture.source.clone()),
        workspace: prepared.id,
    });
    let fail_compaction = async {
        wait_for_compaction_request(&fixture).await;
        for event in [
            CodexEvent::Notification {
                method: "turn/started".to_owned(),
                params: json!({
                    "threadId": "fork-thread-1",
                    "turn": {"id": "failed-compact-turn", "status": "inProgress"},
                }),
            },
            CodexEvent::Notification {
                method: "turn/completed".to_owned(),
                params: json!({
                    "threadId": "fork-thread-1",
                    "turn": {"id": "failed-compact-turn", "status": "failed"},
                }),
            },
        ] {
            fixture.coordinator.record_codex_event(event).unwrap();
        }
    };
    let (result, ()) = tokio::join!(attach, fail_compaction);
    assert!(matches!(result, Err(CoordinatorError::CompactionFailed(_))));

    let failed = fixture
        .store
        .workspace_by_name(&repository.id, "failed-native-compact")
        .unwrap()
        .unwrap();
    assert_eq!(failed.lifecycle, WorkspaceLifecycle::Failed);
    assert_eq!(failed.codex_thread_id.as_deref(), Some("fork-thread-1"));
    assert!(failed.worktree_path.unwrap().is_dir());
}

#[tokio::test]
async fn rejects_blank_create_inputs_before_side_effects() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;

    let mut blank_base = fixture.create_params();
    blank_base.worktree = WorkspaceWorktreeRequest::Detached {
        base: WorkspaceBaseRequest::Revision {
            revision: "  ".to_owned(),
        },
    };
    assert!(matches!(
        fixture.coordinator.create_workspace(blank_base).await,
        Err(CoordinatorError::InvalidParams(_))
    ));

    let mut blank_workspace = fixture.create_params();
    blank_workspace.context = WorkspaceContextRequest::Fork {
        source: WorkspaceContextSource::Workspace {
            workspace: "  ".to_owned(),
        },
        compact: false,
    };
    assert!(matches!(
        fixture.coordinator.create_workspace(blank_workspace).await,
        Err(CoordinatorError::InvalidParams(_))
    ));

    let mut blank_thread = fixture.create_params();
    blank_thread.context = WorkspaceContextRequest::Fork {
        source: WorkspaceContextSource::Thread {
            thread_id: "  ".to_owned(),
        },
        compact: false,
    };
    assert!(matches!(
        fixture.coordinator.create_workspace(blank_thread).await,
        Err(CoordinatorError::InvalidParams(_))
    ));

    let mut blank_model = fixture.create_params();
    blank_model.model = Some("  ".to_owned());
    assert!(matches!(
        fixture.coordinator.create_workspace(blank_model).await,
        Err(CoordinatorError::InvalidParams(_))
    ));

    assert!(fixture.worker.calls().is_empty());
}

#[tokio::test]
async fn passive_reads_project_not_loaded_without_resuming_or_persisting_it() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    fixture.store.reconcile_unfinished().unwrap();

    let worker = Arc::new(FakeWorker::default());
    worker.remember_bound_thread(&workspace, CodexThreadStatus::NotLoaded);
    let coordinator = fixture.recovery_coordinator(worker.clone(), "runtime-restarted");
    let scope = RepositoryScope::repository(fixture.source.clone());

    let shown = coordinator
        .get_workspace(WorkspaceGetParams {
            scope: scope.clone(),
            workspace: workspace.id.clone(),
        })
        .await
        .unwrap();
    assert_eq!(shown.workspace.phase, WorkspacePhase::NotLoaded);
    let listed = coordinator
        .list_workspaces(WorkspaceListParams {
            scope: scope.clone(),
            phases: None,
        })
        .await
        .unwrap();
    assert_eq!(listed[0].workspace.phase, WorkspacePhase::NotLoaded);
    let events = coordinator
        .list_events(EventListParams {
            scope,
            workspace: workspace.id.clone(),
            after_sequence: 0,
        })
        .await
        .unwrap();
    assert_eq!(events.workspace.phase, WorkspacePhase::NotLoaded);
    assert!(worker.calls().iter().all(|call| matches!(
        call,
        WorkerCall::Read { thread_id } if thread_id == "thread-1"
    )));
    let persisted = fixture
        .store
        .workspace_by_id(&workspace.id)
        .unwrap()
        .unwrap();
    assert_eq!(persisted.phase, WorkspacePhase::Unavailable);
}

#[tokio::test]
async fn send_loads_a_not_loaded_thread_before_starting_the_turn() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let mut create = fixture.create_params();
    create.model = Some("gpt-resume".to_owned());
    let workspace = fixture.create_and_materialize(create).await;
    fixture.store.reconcile_unfinished().unwrap();

    let worker = Arc::new(FakeWorker::default());
    worker.remember_bound_thread(&workspace, CodexThreadStatus::NotLoaded);
    let coordinator = fixture.recovery_coordinator(worker.clone(), "runtime-restarted");
    coordinator
        .start_turn(TurnStartParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id.clone(),
            message: "continue after restart".to_owned(),
            operation_id: "send-after-restart".to_owned(),
        })
        .await
        .unwrap();

    assert!(matches!(
        worker.calls().as_slice(),
        [
            WorkerCall::Read { thread_id },
            WorkerCall::Resume { thread_id: resumed_id, cwd, config, model },
            WorkerCall::Turn { thread_id: turn_thread_id, .. },
        ] if thread_id == "thread-1"
            && resumed_id == "thread-1"
            && turn_thread_id == "thread-1"
            && cwd == workspace.worktree_path.as_ref().unwrap()
            && config == &json!({})
            && model.as_deref() == Some("gpt-resume")
    ));
    let events = fixture.store.events_after(Some(&workspace.id), 0).unwrap();
    assert!(events.iter().all(|event| {
        event.source_method.as_deref() != Some("thread/resume")
            && event.payload.get("reason") != Some(&json!("daemon_recovery"))
    }));
}

#[tokio::test]
async fn attach_subscribes_when_another_client_already_loaded_the_thread() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    fixture.store.reconcile_unfinished().unwrap();
    let worker = Arc::new(FakeWorker::default());
    worker.remember_bound_thread(&workspace, CodexThreadStatus::Idle);
    let coordinator = fixture.recovery_coordinator(worker.clone(), "runtime-restarted");

    let attached = coordinator
        .attach_workspace(WorkspaceAttachParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id.clone(),
        })
        .await
        .unwrap();

    assert_eq!(attached.workspace.phase, WorkspacePhase::Idle);
    assert!(matches!(
        worker.calls().as_slice(),
        [WorkerCall::Read { .. }, WorkerCall::Resume { .. },]
    ));
    assert!(
        worker
            .calls()
            .iter()
            .all(|call| !matches!(call, WorkerCall::Turn { .. }))
    );
    let persisted = fixture
        .store
        .workspace_by_id(&workspace.id)
        .unwrap()
        .unwrap();
    assert_eq!(persisted.phase, WorkspacePhase::Unavailable);
    assert_eq!(persisted.last_error_code, None);
}

#[tokio::test]
async fn concurrent_attach_resumes_a_not_loaded_thread_once() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    fixture.store.reconcile_unfinished().unwrap();
    let worker = Arc::new(FakeWorker::default());
    worker.remember_bound_thread(&workspace, CodexThreadStatus::NotLoaded);
    let coordinator = Arc::new(fixture.recovery_coordinator(worker.clone(), "runtime-restarted"));
    let params = WorkspaceAttachParams {
        scope: RepositoryScope::repository(fixture.source.clone()),
        workspace: workspace.id.clone(),
    };

    let (first, second) = tokio::join!(
        coordinator.attach_workspace(params.clone()),
        coordinator.attach_workspace(params)
    );
    let first = first.unwrap();
    let second = second.unwrap();
    assert_ne!(first.launch, second.launch);

    assert_eq!(
        worker
            .calls()
            .iter()
            .filter(|call| matches!(call, WorkerCall::Resume { .. }))
            .count(),
        1
    );
}

#[tokio::test]
async fn refuses_on_demand_resume_when_the_named_profile_changed() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    fs::create_dir_all(&fixture.codex_home).unwrap();
    fs::write(
        fixture.codex_home.join("dev.config.toml"),
        "model = \"gpt-before\"\n",
    )
    .unwrap();
    let mut params = fixture.create_params();
    params.profile = "dev".to_owned();
    let workspace = fixture.create_and_materialize(params).await;
    fixture.store.reconcile_unfinished().unwrap();
    fs::write(
        fixture.codex_home.join("dev.config.toml"),
        "model = \"gpt-after\"\n",
    )
    .unwrap();

    let worker = Arc::new(FakeWorker::default());
    worker.remember_bound_thread(&workspace, CodexThreadStatus::NotLoaded);
    let coordinator = fixture.recovery_coordinator(worker.clone(), "runtime-recovered");
    let error = coordinator
        .attach_workspace(WorkspaceAttachParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id.clone(),
        })
        .await
        .unwrap_err();
    assert_eq!(error.code(), "PROFILE_CHANGED");
    assert!(matches!(
        worker.calls().as_slice(),
        [WorkerCall::Read { .. }]
    ));
    let unavailable = fixture
        .store
        .workspace_by_id(&workspace.id)
        .unwrap()
        .unwrap();
    assert_eq!(unavailable.phase, WorkspacePhase::Unavailable);
    assert_eq!(unavailable.last_error_code, None);
    assert_eq!(unavailable.last_error_message, None);
}

#[tokio::test]
async fn rejects_a_mismatched_or_still_unloaded_resume_result() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    fixture.store.reconcile_unfinished().unwrap();

    let mismatched = Arc::new(FakeWorker::with_resume_result(
        Some("different-thread"),
        None,
        CodexThreadStatus::Idle,
    ));
    mismatched.remember_bound_thread(&workspace, CodexThreadStatus::NotLoaded);
    let coordinator = fixture.recovery_coordinator(mismatched, "runtime-mismatch");
    assert!(matches!(
        coordinator
            .attach_workspace(WorkspaceAttachParams {
                scope: RepositoryScope::repository(fixture.source.clone()),
                workspace: workspace.id.clone(),
            })
            .await,
        Err(CoordinatorError::Worker(
            WorkerError::ThreadIdMismatch { .. }
        ))
    ));

    let still_unloaded = Arc::new(FakeWorker::with_resume_result(
        None,
        None,
        CodexThreadStatus::NotLoaded,
    ));
    still_unloaded.remember_bound_thread(&workspace, CodexThreadStatus::NotLoaded);
    let coordinator = fixture.recovery_coordinator(still_unloaded, "runtime-unloaded");
    assert!(matches!(
        coordinator
            .attach_workspace(WorkspaceAttachParams {
                scope: RepositoryScope::repository(fixture.source.clone()),
                workspace: workspace.id.clone(),
            })
            .await,
        Err(CoordinatorError::Worker(WorkerError::InvalidThreadRead(message)))
            if message.contains("remained notLoaded")
    ));
}

#[tokio::test]
async fn rejects_a_resume_result_bound_to_another_worktree() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    fixture.store.reconcile_unfinished().unwrap();
    let worker = Arc::new(FakeWorker::with_resume_result(
        None,
        Some(fixture.source.clone()),
        CodexThreadStatus::Idle,
    ));
    worker.remember_bound_thread(&workspace, CodexThreadStatus::NotLoaded);
    let coordinator = fixture.recovery_coordinator(worker, "runtime-cwd-mismatch");

    assert!(matches!(
        coordinator
            .attach_workspace(WorkspaceAttachParams {
                scope: RepositoryScope::repository(fixture.source.clone()),
                workspace: workspace.id,
            })
            .await,
        Err(CoordinatorError::Worker(WorkerError::CwdMismatch { .. }))
    ));
}

#[tokio::test]
async fn fork_after_restart_reads_the_source_without_resuming_it() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let source = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    fixture.store.reconcile_unfinished().unwrap();
    let worker = Arc::new(FakeWorker::default());
    worker.remember_bound_thread(&source, CodexThreadStatus::NotLoaded);
    let coordinator = fixture.recovery_coordinator(worker.clone(), "runtime-restarted");

    let prepared = coordinator
        .create_workspace(fixture.fork_params(&source, "child-after-restart", false))
        .await
        .unwrap()
        .workspace;
    assert!(prepared.parent_thread_id.is_none());
    let child = coordinator
        .materialize_workspace_thread(prepared)
        .await
        .unwrap();

    assert_eq!(child.context_mode, ContextMode::Fork);
    assert_eq!(child.parent_thread_id.as_deref(), Some("thread-1"));
    assert!(matches!(
        worker.calls().as_slice(),
        [
            WorkerCall::Read { thread_id: initial_thread_id },
            WorkerCall::Read { thread_id: validation_thread_id },
            WorkerCall::Fork { source_thread_id, .. },
        ] if initial_thread_id == "thread-1"
            && validation_thread_id == "thread-1"
            && source_thread_id == "thread-1"
    ));
}
