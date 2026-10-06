use super::*;

use crate::domain::{CodexModel, CodexReasoningEffort};

#[tokio::test]
async fn preset_code_bases_never_offer_or_normalize_to_an_existing_branch() {
    let cases = [
        (
            vec!["coco", "create", "review/api", "-i", "--base", "main"],
            vec![0, 0, 0, 0, 0, 0],
        ),
        (
            vec![
                "coco",
                "create",
                "review/api",
                "-i",
                "--base-workspace",
                "source/api",
            ],
            vec![0, 0, 0, 0, 0, 0],
        ),
        (
            vec![
                "coco",
                "create",
                "review/api",
                "-i",
                "--fork-from",
                "source/api",
            ],
            vec![0, 0, 0, 0, 0],
        ),
    ];

    for (arguments, choices) in cases {
        let args = create_args(&arguments);
        let mut interaction = ScriptedInteraction::new(choices, [] as [&str; 0]);
        let args = walkthrough(
            &test_paths(),
            PathBuf::from("/repo"),
            args,
            &mut interaction,
        )
        .await
        .unwrap();

        let worktree_choices = interaction
            .seen_choices
            .iter()
            .zip(&interaction.titles)
            .find_map(|(choices, title)| (title == "Worktree").then_some(choices))
            .unwrap();
        assert!(
            worktree_choices
                .iter()
                .all(|choice| choice.label != "Existing branch")
        );
        let confirmation = final_create_choice(&interaction);
        assert_eq!(confirmation, &Choice::new("Create", None));
        if let Some(source) = args.fork_from.as_deref() {
            assert_eq!(
                final_review_value(&interaction, "Context"),
                format!("workspace {source}")
            );
        }
        let (params, _, _) =
            normalize_create_args(PathBuf::from("/repo"), args, "preset-base".to_owned()).unwrap();
        assert!(!matches!(
            params.worktree,
            WorkspaceWorktreeRequest::ExistingBranch { .. }
        ));
        interaction.assert_consumed();
    }
}

#[test]
fn normalization_rejects_a_checkout_added_after_clap_validated_the_base() {
    for arguments in [
        vec!["coco", "create", "review/api", "--base", "main"],
        vec![
            "coco",
            "create",
            "review/api",
            "--base-workspace",
            "source/api",
        ],
        vec!["coco", "create", "review/api", "--fork-from", "source/api"],
    ] {
        let mut args = create_args(&arguments);
        args.checkout = Some("review/existing".to_owned());
        let error =
            normalize_create_args(PathBuf::from("/repo"), args, "invalid-checkout".to_owned())
                .unwrap_err();
        assert_eq!(
            error.to_string(),
            "--checkout cannot be combined with --base, --base-workspace, or --fork-from"
        );
    }
}

#[tokio::test]
async fn invalid_local_change_presets_fail_before_the_walkthrough_starts() {
    let args = create_args(&["coco", "create", "review/api", "-i", "--carry-untracked"]);
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
        "--carry-untracked requires --carry-changes or --dirty"
    );
    assert!(interaction.titles.is_empty());
    interaction.assert_consumed();
}

#[tokio::test]
async fn walkthrough_covers_custom_existing_and_detached_worktrees() {
    let args = create_args(&["coco", "create", "review/custom", "-i"]);
    let mut interaction =
        ScriptedInteraction::new([1, 2, 0, 0, 0, 0, 0], ["review/custom-branch", "release"]);
    let args = walkthrough(
        &test_paths(),
        PathBuf::from("/repo"),
        args,
        &mut interaction,
    )
    .await
    .unwrap();
    assert_eq!(
        final_create_choice(&interaction),
        &Choice::new("Create", None)
    );
    assert_eq!(
        final_review_value(&interaction, "Workspace"),
        "review/custom"
    );
    assert_eq!(
        final_review_value(&interaction, "Worktree"),
        "branch review/custom-branch"
    );
    let (params, _, _) =
        normalize_create_args(PathBuf::from("/repo"), args, "custom".to_owned()).unwrap();
    assert_eq!(
        params.worktree,
        WorkspaceWorktreeRequest::NewBranch {
            branch: Some("review/custom-branch".to_owned()),
            base: WorkspaceBaseRequest::Revision {
                revision: "release".to_owned(),
            },
        }
    );
    interaction.assert_consumed();

    let args = create_args(&["coco", "create", "review/existing", "-i"]);
    let mut interaction = ScriptedInteraction::new([2, 0, 0, 0, 0, 0], ["review/existing-branch"]);
    let args = walkthrough(
        &test_paths(),
        PathBuf::from("/repo"),
        args,
        &mut interaction,
    )
    .await
    .unwrap();
    assert_eq!(
        final_create_choice(&interaction),
        &Choice::new("Create", None)
    );
    assert_eq!(
        final_review_value(&interaction, "Workspace"),
        "review/existing"
    );
    assert_eq!(
        final_review_value(&interaction, "Worktree"),
        "checkout review/existing-branch"
    );
    assert_eq!(
        final_review_value(&interaction, "Code"),
        "branch review/existing-branch"
    );
    let (params, _, _) =
        normalize_create_args(PathBuf::from("/repo"), args, "existing".to_owned()).unwrap();
    assert_eq!(
        params.worktree,
        WorkspaceWorktreeRequest::ExistingBranch {
            branch: "review/existing-branch".to_owned(),
        }
    );
    interaction.assert_consumed();

    let args = create_args(&["coco", "create", "review/detached", "-i"]);
    let mut interaction = ScriptedInteraction::new([3, 0, 0, 0, 0, 0, 0, 0], [] as [&str; 0]);
    let args = walkthrough(
        &test_paths(),
        PathBuf::from("/repo"),
        args,
        &mut interaction,
    )
    .await
    .unwrap();
    assert_eq!(
        final_create_choice(&interaction),
        &Choice::new("Create", None)
    );
    assert_eq!(
        final_review_value(&interaction, "Workspace"),
        "review/detached"
    );
    assert_eq!(final_review_value(&interaction, "Worktree"), "detached");
    let (params, _, _) =
        normalize_create_args(PathBuf::from("/repo"), args, "detached".to_owned()).unwrap();
    assert_eq!(
        params.worktree,
        WorkspaceWorktreeRequest::Detached {
            base: WorkspaceBaseRequest::Revision {
                revision: "HEAD".to_owned(),
            },
        }
    );
    interaction.assert_consumed();
}

#[tokio::test]
async fn walkthrough_selects_workspace_code_and_each_local_change_policy() {
    let _rpc_test_guard = RPC_TEST_LOCK.lock().await;
    let temporary = tempdir().unwrap();
    let paths = test_paths_at(temporary.path().to_path_buf());
    let handler = Arc::new(WorkspaceListHandler {
        seen_params: Arc::new(Mutex::new(Vec::new())),
        items: vec![workspace_item()],
    });
    let server = RpcServer::bind(&paths.socket_path, handler).await.unwrap();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server_task = tokio::spawn(server.run(shutdown_rx));

    let args = create_args(&["coco", "create", "review/workspace-base", "-i"]);
    let mut interaction = ScriptedInteraction::new([0, 1, 0, 0, 0, 0, 0, 0], [] as [&str; 0]);
    let args = walkthrough(&paths, PathBuf::from("/repo"), args, &mut interaction)
        .await
        .unwrap();
    assert_eq!(args.base_workspace.as_deref(), Some("workspace-source"));
    interaction.assert_consumed();

    shutdown_tx.send(true).unwrap();
    server_task.await.unwrap().unwrap();

    for (local_choice, expected) in [
        (1, WorkspaceChangesRequest::CarryTracked),
        (2, WorkspaceChangesRequest::CarryTrackedAndUntracked),
    ] {
        let args = create_args(&["coco", "create", "review/local", "-i"]);
        let mut interaction =
            ScriptedInteraction::new([0, 0, local_choice, 0, 0, 0, 0, 0], [] as [&str; 0]);
        let args = walkthrough(
            &test_paths(),
            PathBuf::from("/repo"),
            args,
            &mut interaction,
        )
        .await
        .unwrap();
        let (params, _, _) = normalize_create_args(
            PathBuf::from("/repo"),
            args,
            format!("local-{local_choice}"),
        )
        .unwrap();
        assert_eq!(params.changes, expected);
        interaction.assert_consumed();
    }
}

#[tokio::test]
async fn walkthrough_selects_a_named_profile_and_native_model() {
    let _rpc_test_guard = RPC_TEST_LOCK.lock().await;
    let temporary = tempdir().unwrap();
    let paths = test_paths_at(temporary.path().to_path_buf());
    let handler = Arc::new(ModelListHandler {
        models: vec![model("gpt-default", true), model("gpt-selected", false)],
    });
    let server = RpcServer::bind(&paths.socket_path, handler).await.unwrap();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server_task = tokio::spawn(server.run(shutdown_rx));

    let args = create_args(&["coco", "create", "review/model", "-i"]);
    let mut interaction = ScriptedInteraction::new([0, 0, 0, 0, 1, 1, 1, 0, 0], ["development"]);
    let args = walkthrough(&paths, PathBuf::from("/repo"), args, &mut interaction)
        .await
        .unwrap();

    assert_eq!(args.profile.as_deref(), Some("development"));
    assert_eq!(args.model.as_deref(), Some("gpt-selected"));
    interaction.assert_consumed();

    shutdown_tx.send(true).unwrap();
    server_task.await.unwrap().unwrap();
}

#[test]
fn model_choices_do_not_repeat_identical_names_and_selectors() {
    let matching = model("gpt-default", true);
    assert_eq!(
        model_choice(&matching),
        Choice::new("gpt-default", None).with_default_marker()
    );

    let mut distinct = model("gpt-default", true);
    distinct.display_name = "GPT Default".to_owned();
    assert_eq!(
        model_choice(&distinct),
        Choice::new("GPT Default", Some("gpt-default".to_owned())).with_default_marker()
    );
}

#[tokio::test]
async fn an_empty_model_catalog_returns_to_the_inherit_choice() {
    let _rpc_test_guard = RPC_TEST_LOCK.lock().await;
    let temporary = tempdir().unwrap();
    let paths = test_paths_at(temporary.path().to_path_buf());
    let handler = Arc::new(ModelListHandler { models: Vec::new() });
    let server = RpcServer::bind(&paths.socket_path, handler).await.unwrap();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server_task = tokio::spawn(server.run(shutdown_rx));

    let args = create_args(&["coco", "create", "review/model", "-i"]);
    let mut interaction = ScriptedInteraction::new([0, 0, 0, 0, 0, 1, 0, 0, 0], [] as [&str; 0]);
    let args = walkthrough(&paths, PathBuf::from("/repo"), args, &mut interaction)
        .await
        .unwrap();

    assert!(args.model.is_none());
    assert_eq!(
        interaction.notices,
        ["No selectable Codex models found. Use the configured model or try again later."]
    );
    assert_eq!(
        interaction
            .titles
            .iter()
            .filter(|title| title.as_str() == "Model")
            .count(),
        2
    );
    interaction.assert_consumed();

    shutdown_tx.send(true).unwrap();
    server_task.await.unwrap().unwrap();
}

#[tokio::test]
async fn walkthrough_covers_send_only_and_jump_only_actions() {
    let args = create_args(&["coco", "create", "review/send", "-i"]);
    let mut interaction = ScriptedInteraction::new([0, 0, 0, 0, 0, 0, 1, 0], ["Review the change"]);
    let args = walkthrough(
        &test_paths(),
        PathBuf::from("/repo"),
        args,
        &mut interaction,
    )
    .await
    .unwrap();
    assert_eq!(args.send.as_deref(), Some("Review the change"));
    assert!(!args.jump);
    interaction.assert_consumed();

    let args = create_args(&["coco", "create", "review/jump", "-i"]);
    let mut interaction = ScriptedInteraction::new([0, 0, 0, 0, 0, 0, 2, 0], [] as [&str; 0]);
    let args = walkthrough(
        &test_paths(),
        PathBuf::from("/repo"),
        args,
        &mut interaction,
    )
    .await
    .unwrap();
    assert!(args.send.is_none());
    assert!(args.jump);
    interaction.assert_consumed();
}

struct ModelListHandler {
    models: Vec<CodexModel>,
}

#[async_trait]
impl RpcHandler for ModelListHandler {
    async fn handle(&self, method: &str, params: Value) -> Result<Value, RpcErrorPayload> {
        assert_eq!(method, "model.list");
        serde_json::from_value::<ModelListParams>(params)
            .map_err(|error| RpcErrorPayload::new("INVALID_PARAMS", error.to_string()))?;
        serde_json::to_value(&self.models)
            .map_err(|error| RpcErrorPayload::new("INTERNAL", error.to_string()))
    }
}

fn model(name: &str, is_default: bool) -> CodexModel {
    CodexModel {
        id: name.to_owned(),
        model: name.to_owned(),
        display_name: name.to_owned(),
        description: String::new(),
        is_default,
        default_reasoning_effort: "medium".to_owned(),
        supported_reasoning_efforts: vec![CodexReasoningEffort {
            reasoning_effort: "medium".to_owned(),
            description: String::new(),
        }],
        input_modalities: vec!["text".to_owned()],
        supports_personality: false,
    }
}
