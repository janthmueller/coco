use super::*;
use crate::cli::args::WorkspaceSort;

#[test]
fn jump_defaults_to_recent_and_can_explicitly_select_name_order() {
    for (tail, expected) in [
        (vec![], WorkspaceSort::Recent),
        (vec!["--sort", "recent"], WorkspaceSort::Recent),
        (vec!["--sort", "name"], WorkspaceSort::Name),
    ] {
        let mut args = vec!["coco", "jump", "-g"];
        args.extend(tail);
        let cli = Cli::try_parse_from(args).unwrap();
        assert!(cli.requests_global_search());
        assert!(
            matches!(cli.command, Command::Jump(args) if args.sort == expected && args.workspace.is_none())
        );
    }
    assert!(
        matches!(Cli::try_parse_from(["coco", "jump", "fix/login"]).unwrap().command,
        Command::Jump(args) if args.workspace.as_deref() == Some("fix/login"))
    );
    for invalid in ["state", "updated", "-recent", "name,recent"] {
        assert!(Cli::try_parse_from(["coco", "jump", "--sort", invalid]).is_err());
    }
    let mut command = Cli::command();
    let help = command
        .find_subcommand_mut("jump")
        .unwrap()
        .render_long_help()
        .to_string();
    assert!(help.contains("--sort"));
    assert!(help.contains("recent"));
    assert!(help.contains("name"));
}

#[test]
fn workspace_sort_options_are_consistent_across_interactive_commands() {
    use crate::cli::args::LimitsCommand;
    let commands: &[&[&str]] = &[
        &["create"],
        &["send"],
        &["diff"],
        &["close"],
        &["reopen"],
        &["delete"],
        &["limits", "show"],
        &["limits", "set"],
        &["limits", "reset"],
    ];
    for command in commands {
        for (tail, expected) in [
            (vec![], WorkspaceSort::Recent),
            (vec!["--sort", "recent"], WorkspaceSort::Recent),
            (vec!["--sort", "name"], WorkspaceSort::Name),
        ] {
            let mut args = vec!["coco"];
            args.extend_from_slice(command);
            args.extend(tail);
            let parsed = Cli::try_parse_from(args).unwrap();
            let actual = match parsed.command {
                Command::Create(args) => args.sort,
                Command::Send { sort, .. } | Command::Diff { sort, .. } => sort,
                Command::Close(args) => args.sort,
                Command::Reopen(args) => args.sort,
                Command::Delete(args) => args.sort,
                Command::Limits { command } => match command {
                    LimitsCommand::Show(args) | LimitsCommand::Reset(args) => args.sort,
                    LimitsCommand::Set(args) => args.sort,
                },
                other => panic!("unexpected command: {other:?}"),
            };
            assert_eq!(actual, expected, "{command:?}");
        }
        let mut invalid = vec!["coco"];
        invalid.extend_from_slice(command);
        invalid.extend(["--sort", "updated"]);
        assert!(Cli::try_parse_from(invalid).is_err());
    }
}
