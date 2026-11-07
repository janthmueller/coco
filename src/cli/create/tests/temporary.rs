use super::*;

fn preset(extra: &[&str]) -> CreateArgs {
    let mut arguments = vec![
        "coco",
        "create",
        "scratch/review",
        "-i",
        "--detached",
        "--base",
        "HEAD",
        "--context",
        "thread:source",
        "--profile",
        "dev",
        "--model",
        "model-from-config",
    ];
    arguments.extend_from_slice(extra);
    create_args(&arguments)
}

#[test]
fn temporary_creation_does_not_require_jump_or_change_code_and_context() {
    for extra in [
        vec![],
        vec!["--keep-thread"],
        vec!["--send", "Inspect the code"],
    ] {
        let mut arguments = vec!["coco", "create", "scratch/review", "--temporary"];
        arguments.extend_from_slice(&extra);
        let args = create_args(&arguments);
        let (params, message, jump) =
            normalize_create_args(PathBuf::from("/repo"), args, "create-once".into()).unwrap();
        assert!(!jump);
        assert!(params.temporary.is_some());
        assert_eq!(
            params.temporary.unwrap().keep_thread,
            extra.contains(&"--keep-thread")
        );
        assert_eq!(message.is_some(), extra.contains(&"--send"));
        assert_eq!(params.context, WorkspaceContextRequest::Fresh);
        assert!(matches!(
            params.worktree,
            WorkspaceWorktreeRequest::NewBranch { .. }
        ));
    }
    assert!(Cli::try_parse_from(["coco", "create", "scratch", "--keep-thread"]).is_err());
}

#[tokio::test]
async fn walkthrough_defaults_to_opening_temporary_workspaces_but_can_only_prepare() {
    for (action, keep_thread) in [(0, false), (1, true)] {
        let mut interaction =
            ScriptedInteraction::new([usize::from(keep_thread), action, 0], [] as [&str; 0]);
        interaction.cleanup_choice = Some(1);
        let args = walkthrough(
            &test_paths(),
            PathBuf::from("/repo"),
            preset(&[]),
            &mut interaction,
        )
        .await
        .unwrap();
        assert!(args.temporary);
        assert_eq!(args.keep_thread, keep_thread);
        assert_eq!(args.jump, action == 0);
        assert_eq!(
            choices_for_title(&interaction, "After create")[0].label,
            "Open Codex"
        );
        assert!(choices_for_title(&interaction, "After create")[0].is_default);
        assert_eq!(
            final_review_value(&interaction, "Cleanup"),
            "after TUI, when safe"
        );
        interaction.assert_consumed();
    }
}

#[tokio::test]
async fn explicit_temporary_keep_and_jump_flags_are_not_asked_again() {
    let mut interaction = ScriptedInteraction::new([0], [] as [&str; 0]);
    let args = walkthrough(
        &test_paths(),
        PathBuf::from("/repo"),
        preset(&["--temporary", "--keep-thread", "--jump"]),
        &mut interaction,
    )
    .await
    .unwrap();
    assert_eq!(interaction.titles, ["Create workspace"]);
    assert!(args.temporary && args.keep_thread && args.jump);
    assert_eq!(final_review_value(&interaction, "Conversation"), "keep");
    interaction.assert_consumed();
}

#[tokio::test]
async fn ordinary_walkthrough_stays_persistent_and_returns_to_shell_by_default() {
    let mut interaction = ScriptedInteraction::new([0, 0], [] as [&str; 0]);
    let args = walkthrough(
        &test_paths(),
        PathBuf::from("/repo"),
        preset(&[]),
        &mut interaction,
    )
    .await
    .unwrap();
    assert!(!args.temporary && !args.jump && !args.keep_thread);
    assert_eq!(
        choices_for_title(&interaction, "Cleanup")[0].label,
        "Keep workspace"
    );
    assert_eq!(
        choices_for_title(&interaction, "After create")[0].label,
        "Return to shell"
    );
    interaction.assert_consumed();
}
