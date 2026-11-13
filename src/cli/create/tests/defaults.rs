use super::*;

fn tab_at(
    title: &str,
    choices: impl IntoIterator<Item = usize>,
    texts: impl IntoIterator<Item = impl Into<String>>,
) -> ScriptedInteraction {
    let mut interaction = ScriptedInteraction::new(choices, texts);
    interaction.defaults_at = Some(title.to_owned());
    interaction
}

#[test]
fn tab_uses_the_marked_default_even_when_it_is_not_the_first_choice() {
    let mut interaction = tab_at("Choose model", [0], [] as [&str; 0]);
    let choices = [
        Choice::new("First model", None),
        Choice::new("Native default model", None).with_default_marker(),
    ];
    {
        let mut defaults = super::super::defaults::DefaultsInteraction::new(&mut interaction);
        assert_eq!(defaults.select("Choose model", &choices).unwrap(), 1);
        // Review still consumes an explicit answer after defaults are enabled.
        assert_eq!(
            defaults
                .select_with_review("Create workspace", &[], &[Choice::new("Create", None)])
                .unwrap(),
            0
        );
    }
    interaction.assert_consumed();
}

#[tokio::test]
async fn tab_at_the_first_step_keeps_defaults_and_skips_unneeded_discovery() {
    let paths = test_paths();
    // This path would normally require discovery to offer Current workspace.
    let scope = paths.worktrees_dir.join("current/repo");
    let mut interaction = tab_at("Worktree", [0], ["review/api"]);
    let args = walkthrough(
        &paths,
        scope.clone(),
        create_args(&["coco", "create"]),
        &mut interaction,
    )
    .await
    .unwrap();
    assert_eq!(interaction.titles, ["Worktree", "Create workspace"]);
    assert_eq!(final_review_value(&interaction, "Code"), "main · aaaaaaaa");
    assert_eq!(final_review_value(&interaction, "Context"), "fresh");
    let (params, message, jump) = normalize_create_args(scope, args, "defaults".into()).unwrap();
    assert_eq!(params.context, WorkspaceContextRequest::Fresh);
    assert_eq!(params.profile, "default");
    assert!(params.temporary.is_none());
    assert!(message.is_none() && !jump);
    interaction.assert_consumed();
}

#[tokio::test]
async fn tab_after_code_and_context_keeps_prior_choices_and_reviews_the_result() {
    let mut interaction = tab_at("Profile", [3, 3, 2, 1, 0], ["main", "probe-context"]);
    let args = walkthrough(
        &test_paths(),
        "/repo".into(),
        create_args(&["coco", "create", "review/api", "-i"]),
        &mut interaction,
    )
    .await
    .unwrap();
    assert!(args.detached && args.compact_context);
    assert_eq!(args.base.as_deref(), Some("main"));
    assert_eq!(args.context.as_deref(), Some("thread:probe-context"));
    assert_eq!(
        interaction.titles.last().map(String::as_str),
        Some("Create workspace")
    );
    assert!(
        !interaction
            .titles
            .iter()
            .any(|title| title == "Model" || title == "After create")
    );
    assert_eq!(final_review_value(&interaction, "Code"), "main · aaaaaaaa");
    assert_eq!(
        final_review_value(&interaction, "Context"),
        "thread probe-context · compact"
    );
    interaction.assert_consumed();
}

#[tokio::test]
async fn tab_on_context_size_uses_full_context_and_keeps_its_selected_source() {
    let mut interaction = tab_at("Context size", [0, 2, 0], ["probe-context"]);
    let args = walkthrough(
        &test_paths(),
        "/repo".into(),
        create_args(&["coco", "create", "review/api", "-i", "--base", "main"]),
        &mut interaction,
    )
    .await
    .unwrap();
    assert_eq!(args.context.as_deref(), Some("thread:probe-context"));
    assert!(!args.compact_context);
    assert_eq!(
        final_review_value(&interaction, "Context"),
        "thread probe-context"
    );
    interaction.assert_consumed();
}

#[tokio::test]
async fn tab_preserves_explicit_flags_including_nondefault_actions() {
    let args = create_args(&[
        "coco",
        "create",
        "review/api",
        "-i",
        "--base",
        "main",
        "--context",
        "thread:source",
        "--compact-context",
        "--dirty",
        "--profile",
        "dev",
        "--model",
        "explicit-model",
        "--temporary",
        "--keep-thread",
        "--send",
        "Inspect this",
        "--jump",
    ]);
    let mut interaction = tab_at("Worktree", [0], [] as [&str; 0]);
    let args = walkthrough(&test_paths(), "/repo".into(), args, &mut interaction)
        .await
        .unwrap();
    assert_eq!(args.base.as_deref(), Some("main"));
    assert_eq!(args.context.as_deref(), Some("thread:source"));
    assert_eq!(args.profile.as_deref(), Some("dev"));
    assert_eq!(args.model.as_deref(), Some("explicit-model"));
    assert!(args.compact_context && args.dirty && args.temporary && args.keep_thread && args.jump);
    assert_eq!(args.send.as_deref(), Some("Inspect this"));
    interaction.assert_consumed();
}

#[tokio::test]
async fn tab_respects_temporary_sessions_conditional_post_create_default() {
    for temporary in [false, true] {
        let mut arguments = vec!["coco", "create", "review/api", "-i"];
        if temporary {
            arguments.push("--temporary");
        }
        let mut interaction = tab_at("Worktree", [0], [] as [&str; 0]);
        let args = walkthrough(
            &test_paths(),
            "/repo".into(),
            create_args(&arguments),
            &mut interaction,
        )
        .await
        .unwrap();
        assert_eq!(args.temporary, temporary);
        assert_eq!(args.jump, temporary);
        assert!(!args.keep_thread);
        interaction.assert_consumed();
    }
}

#[tokio::test]
async fn tab_still_collects_required_context_and_text_without_a_semantic_default() {
    let mut interaction = tab_at("Worktree", [1, 0], ["probe-context"]);
    let args = walkthrough(
        &test_paths(),
        "/repo".into(),
        create_args(&["coco", "create", "review/api", "-i", "--compact-context"]),
        &mut interaction,
    )
    .await
    .unwrap();
    assert_eq!(
        interaction.titles,
        ["Worktree", "Context", "Create workspace"]
    );
    assert_eq!(args.context.as_deref(), Some("thread:probe-context"));
    assert!(args.compact_context);
    interaction.assert_consumed();
}

#[tokio::test]
async fn tab_in_the_source_picker_requires_selection_instead_of_using_the_first_row() {
    let temporary = tempdir().unwrap();
    let paths = test_paths_at(temporary.path().to_path_buf());
    let handler = Arc::new(WorkspaceListHandler {
        seen_params: Arc::new(Mutex::new(Vec::new())),
        items: vec![
            workspace_item_at("first", "a/source", Path::new("/sources/a")),
            workspace_item_at("second", "b/source", Path::new("/sources/b")),
        ],
    });
    let server = RpcServer::bind(&paths.socket_path, handler).await.unwrap();
    let (stop, receiver) = watch::channel(false);
    let server = tokio::spawn(server.run(receiver));
    let mut interaction = tab_at(
        "Context from workspace",
        [0, 0, 0, 1, 1, 0],
        [] as [&str; 0],
    );
    let result = walkthrough(
        &paths,
        "/repo".into(),
        create_args(&["coco", "create", "review/api", "-i"]),
        &mut interaction,
    )
    .await;
    stop.send(true).unwrap();
    server.await.unwrap().unwrap();
    let args = result.unwrap();
    assert_eq!(args.context.as_deref(), Some("workspace:second"));
    assert_eq!(interaction.notices.len(), 1);
    assert!(interaction.notices[0].contains("remaining defaults"));
    assert!(
        !interaction
            .titles
            .iter()
            .any(|title| title == "Context size" || title == "Profile")
    );
    interaction.assert_consumed();
}

#[tokio::test]
async fn tab_never_bypasses_the_final_create_or_cancel_decision() {
    let mut interaction = tab_at("Worktree", [1], [] as [&str; 0]);
    let result = walkthrough(
        &test_paths(),
        "/repo".into(),
        create_args(&["coco", "create", "review/api", "-i"]),
        &mut interaction,
    )
    .await;
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("creation cancelled")
    );
    assert_eq!(interaction.titles, ["Worktree", "Create workspace"]);
    interaction.assert_consumed();
}

#[tokio::test]
#[ignore = "manual creation defaults probe; requires an interactive PTY, no daemon or model calls"]
async fn interactive_defaults_walkthrough_probe() {
    let mut interaction = crate::cli::prompt::TerminalInteraction::detect(false);
    let result = walkthrough(
        &test_paths(),
        "/unreachable/coco-defaults-probe".into(),
        create_args(&["coco", "create", "probe/defaults", "-i"]),
        &mut interaction,
    )
    .await;
    assert!(!crossterm::terminal::is_raw_mode_enabled().unwrap());
    if let Ok(args) = &result {
        assert_eq!(args.context.as_deref(), Some("thread:probe-context"));
        assert!(!args.compact_context && !args.temporary && !args.jump);
        assert!(args.model.is_none() && args.profile.is_none());
    }
    eprintln!("defaults-result={result:?}");
}
