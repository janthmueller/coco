use super::*;

#[tokio::test]
async fn context_workspace_picker_requests_only_eligible_sources_and_stores_the_id() {
    let _rpc_test_guard = RPC_TEST_LOCK.lock().await;
    let temporary = tempdir().unwrap();
    let paths = test_paths_at(temporary.path().to_path_buf());
    let seen_params = Arc::new(Mutex::new(Vec::new()));
    let handler = Arc::new(WorkspaceListHandler {
        seen_params: Arc::clone(&seen_params),
        items: vec![workspace_item()],
    });
    let server = RpcServer::bind(&paths.socket_path, handler).await.unwrap();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server_task = tokio::spawn(server.run(shutdown_rx));

    let args = create_args(&["coco", "create", "review/api", "-i"]);
    let mut interaction = ScriptedInteraction::new([0, 0, 0, 1, 0, 0, 0, 0, 0, 0], [] as [&str; 0]);
    let args = walkthrough(&paths, PathBuf::from("/repo"), args, &mut interaction)
        .await
        .unwrap();

    assert_eq!(args.context.as_deref(), Some("workspace:workspace-source"));
    assert!(
        interaction
            .titles
            .iter()
            .any(|title| title == "Context from workspace")
    );
    let params = seen_params.lock().unwrap().last().cloned().unwrap();
    assert_eq!(
        params.phases,
        Some(vec!["idle".to_owned(), "not_loaded".to_owned()])
    );
    assert_eq!(
        params.scope,
        crate::protocol::RepositoryScope::repository(PathBuf::from("/repo"))
    );
    assert_eq!(
        final_review_value(&interaction, "Context"),
        "workspace source/review"
    );
    assert!(
        final_review_fields(&interaction)
            .iter()
            .all(|field| !field.value.contains("workspace-source"))
    );
    interaction.assert_consumed();

    shutdown_tx.send(true).unwrap();
    server_task.await.unwrap().unwrap();
}

#[tokio::test]
async fn current_workspace_context_is_offered_for_nested_worktree_paths_and_stores_its_id() {
    let _rpc_test_guard = RPC_TEST_LOCK.lock().await;
    let temporary = tempdir().unwrap();
    let paths = test_paths_at(temporary.path().to_path_buf());
    let current_worktree = paths.worktrees_dir.join("repository/current");
    let nested = current_worktree.join("src/domain");
    std::fs::create_dir_all(&nested).unwrap();
    let other_worktree = paths.worktrees_dir.join("repository/other");
    std::fs::create_dir_all(&other_worktree).unwrap();
    let seen_params = Arc::new(Mutex::new(Vec::new()));
    let handler = Arc::new(WorkspaceListHandler {
        seen_params: Arc::clone(&seen_params),
        items: vec![
            workspace_item_at("workspace-current", "feat/current", &current_worktree),
            workspace_item_at("workspace-other", "feat/other", &other_worktree),
        ],
    });
    let server = RpcServer::bind(&paths.socket_path, handler).await.unwrap();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server_task = tokio::spawn(server.run(shutdown_rx));

    let args = create_args(&["coco", "create", "review/api", "-i"]);
    let mut interaction = ScriptedInteraction::new([0, 0, 0, 1, 0, 0, 0, 0, 0], [] as [&str; 0]);
    let args = walkthrough(&paths, nested.clone(), args, &mut interaction)
        .await
        .unwrap();

    assert_eq!(args.context.as_deref(), Some("workspace:workspace-current"));
    let context_choices = interaction
        .seen_choices
        .iter()
        .zip(&interaction.titles)
        .find_map(|(choices, title)| (title == "Context").then_some(choices))
        .unwrap();
    assert_eq!(context_choices[0].label, "Fresh");
    assert_eq!(context_choices[1].label, "Current workspace");
    assert!(
        context_choices[1]
            .detail
            .as_deref()
            .unwrap()
            .contains("feat/current")
    );
    assert_eq!(
        final_review_value(&interaction, "Context"),
        "workspace feat/current"
    );
    interaction.assert_consumed();

    let (params, message, jump) =
        normalize_create_args(nested, args, "operation-current".to_owned()).unwrap();
    assert_eq!(
        params.context,
        WorkspaceContextRequest::Fork {
            source: WorkspaceContextSource::Reference {
                reference: "workspace:workspace-current".to_owned(),
            },
            compact: false,
        }
    );
    assert!(message.is_none());
    assert!(!jump);
    {
        let requests = seen_params.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].phases,
            Some(vec!["idle".to_owned(), "not_loaded".to_owned()])
        );
    }

    shutdown_tx.send(true).unwrap();
    server_task.await.unwrap().unwrap();
}

#[tokio::test]
async fn direct_current_context_selector_resolves_the_owning_workspace_once() {
    let _rpc_test_guard = RPC_TEST_LOCK.lock().await;
    let temporary = tempdir().unwrap();
    let paths = test_paths_at(temporary.path().to_path_buf());
    let current_worktree = paths.worktrees_dir.join("repository/current");
    let nested = current_worktree.join("src/domain");
    std::fs::create_dir_all(&nested).unwrap();
    let seen_params = Arc::new(Mutex::new(Vec::new()));
    let handler = Arc::new(WorkspaceListHandler {
        seen_params: Arc::clone(&seen_params),
        items: vec![workspace_item_at(
            "workspace-current",
            "feat/current",
            &current_worktree,
        )],
    });
    let server = RpcServer::bind(&paths.socket_path, handler).await.unwrap();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server_task = tokio::spawn(server.run(shutdown_rx));

    let args = create_args(&["coco", "create", "review/api", "-c", "."]);
    let args = resolve_current_context(&paths, &nested, args)
        .await
        .unwrap();

    assert_eq!(args.context.as_deref(), Some("workspace:workspace-current"));
    {
        let requests = seen_params.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].phases, None);
    }

    shutdown_tx.send(true).unwrap();
    server_task.await.unwrap().unwrap();
}

#[tokio::test]
async fn guided_current_context_preset_is_stable_before_the_first_prompt() {
    let _rpc_test_guard = RPC_TEST_LOCK.lock().await;
    let temporary = tempdir().unwrap();
    let paths = test_paths_at(temporary.path().to_path_buf());
    let current_worktree = paths.worktrees_dir.join("repository/current");
    let nested = current_worktree.join("src/domain");
    std::fs::create_dir_all(&nested).unwrap();
    let seen_params = Arc::new(Mutex::new(Vec::new()));
    let handler = Arc::new(WorkspaceListHandler {
        seen_params: Arc::clone(&seen_params),
        items: vec![workspace_item_at(
            "workspace-current",
            "feat/current",
            &current_worktree,
        )],
    });
    let server = RpcServer::bind(&paths.socket_path, handler).await.unwrap();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server_task = tokio::spawn(server.run(shutdown_rx));

    let args = create_args(&["coco", "create", "review/api", "-i", "-c", "."]);
    let mut interaction = ScriptedInteraction::new([0, 0, 0, 0, 0, 0, 0], [] as [&str; 0]);
    let args = walkthrough(&paths, nested, args, &mut interaction)
        .await
        .unwrap();

    assert_eq!(args.context.as_deref(), Some("workspace:workspace-current"));
    assert_eq!(
        final_review_value(&interaction, "Context"),
        "workspace feat/current"
    );
    assert_eq!(seen_params.lock().unwrap().len(), 1);
    interaction.assert_consumed();

    shutdown_tx.send(true).unwrap();
    server_task.await.unwrap().unwrap();
}

#[tokio::test]
async fn existing_workspace_picker_excludes_the_detected_current_workspace() {
    let _rpc_test_guard = RPC_TEST_LOCK.lock().await;
    let temporary = tempdir().unwrap();
    let paths = test_paths_at(temporary.path().to_path_buf());
    let current_worktree = paths.worktrees_dir.join("repository/current");
    let nested = current_worktree.join("src");
    std::fs::create_dir_all(&nested).unwrap();
    let other_worktree = paths.worktrees_dir.join("repository/other");
    std::fs::create_dir_all(&other_worktree).unwrap();
    let handler = Arc::new(WorkspaceListHandler {
        seen_params: Arc::new(Mutex::new(Vec::new())),
        items: vec![
            workspace_item_at("workspace-current", "feat/current", &current_worktree),
            workspace_item_at("workspace-other", "feat/other", &other_worktree),
        ],
    });
    let server = RpcServer::bind(&paths.socket_path, handler).await.unwrap();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server_task = tokio::spawn(server.run(shutdown_rx));

    let args = create_args(&["coco", "create", "review/api", "-i"]);
    let mut interaction = ScriptedInteraction::new([0, 0, 0, 2, 0, 0, 0, 0, 0, 0], [] as [&str; 0]);
    let args = walkthrough(&paths, nested, args, &mut interaction)
        .await
        .unwrap();

    assert_eq!(args.context.as_deref(), Some("workspace:workspace-other"));
    let workspace_choices = interaction
        .seen_choices
        .iter()
        .zip(&interaction.titles)
        .find_map(|(choices, title)| (title == "Context from workspace").then_some(choices))
        .unwrap();
    assert_eq!(workspace_choices.len(), 1);
    assert_eq!(workspace_choices[0].label, "feat/other");
    interaction.assert_consumed();

    shutdown_tx.send(true).unwrap();
    server_task.await.unwrap().unwrap();
}

#[tokio::test]
async fn active_current_workspace_is_not_offered_as_reusable_context() {
    let _rpc_test_guard = RPC_TEST_LOCK.lock().await;
    let temporary = tempdir().unwrap();
    let paths = test_paths_at(temporary.path().to_path_buf());
    let current_worktree = paths.worktrees_dir.join("repository/current");
    std::fs::create_dir_all(&current_worktree).unwrap();
    let mut current = workspace_item_at("workspace-current", "feat/current", &current_worktree);
    current.workspace.phase = WorkspacePhase::Active;
    current.workspace.active_turn_id = Some("turn-current".to_owned());
    let handler = Arc::new(WorkspaceListHandler {
        seen_params: Arc::new(Mutex::new(Vec::new())),
        items: vec![current],
    });
    let server = RpcServer::bind(&paths.socket_path, handler).await.unwrap();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server_task = tokio::spawn(server.run(shutdown_rx));

    let args = create_args(&["coco", "create", "review/api", "-i"]);
    let mut interaction = ScriptedInteraction::new([0, 0, 0, 0, 0, 0, 0, 0], [] as [&str; 0]);
    let args = walkthrough(&paths, current_worktree, args, &mut interaction)
        .await
        .unwrap();

    assert!(args.context.is_none());
    let context_choices = interaction
        .seen_choices
        .iter()
        .zip(&interaction.titles)
        .find_map(|(choices, title)| (title == "Context").then_some(choices))
        .unwrap();
    assert!(
        context_choices
            .iter()
            .all(|choice| choice.label != "Current workspace")
    );
    interaction.assert_consumed();

    shutdown_tx.send(true).unwrap();
    server_task.await.unwrap().unwrap();
}

#[tokio::test]
async fn current_context_selector_fails_outside_a_coco_worktree_without_rpc() {
    let args = create_args(&["coco", "create", "review/api", "-c", "."]);
    let error = resolve_current_context(&test_paths(), Path::new("/repo"), args)
        .await
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "--context . requires the selected path to be inside an open CoCo workspace worktree"
    );
}

#[tokio::test]
async fn guided_current_context_preset_fails_before_the_first_prompt_outside_a_worktree() {
    let args = create_args(&["coco", "create", "review/api", "-i", "-c", "."]);
    let mut interaction = ScriptedInteraction::new([], [] as [&str; 0]);

    let error = walkthrough(
        &test_paths(),
        PathBuf::from("/repo"),
        args,
        &mut interaction,
    )
    .await
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "--context . requires the selected path to be inside an open CoCo workspace worktree"
    );
    assert!(interaction.titles.is_empty());
    interaction.assert_consumed();
}
