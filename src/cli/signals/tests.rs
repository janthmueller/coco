use super::*;
use crate::cli::args::{Cli, Command};
use clap::{CommandFactory, Parser};

#[test]
fn signal_lists_have_consistent_aliases_and_combinable_short_flags() {
    for alias in ["list", "ls"] {
        let cli = Cli::try_parse_from(["coco", "signal", alias, "review/widget", "-gf", "--json"])
            .unwrap();
        assert!(cli.requests_global_search());
        assert!(!cli.requests_all_repositories());
        let Command::Signal {
            command: SignalCommand::List(args),
        } = cli.command
        else {
            panic!("expected signal list");
        };
        assert!(args.global && args.follow && args.json);
        assert!(!args.all_repos);
        assert_eq!(args.workspace.as_deref(), Some("review/widget"));
        assert_eq!(args.limit, 100);

        let cli = Cli::try_parse_from(["coco", "signal", alias, "-af"]).unwrap();
        assert!(cli.requests_all_repositories());
        assert!(!cli.requests_global_search());
        let Command::Signal {
            command: SignalCommand::List(args),
        } = cli.command
        else {
            panic!("expected signal list");
        };
        assert!(args.workspace.is_none() && args.follow);
    }
}

#[test]
fn signal_scope_matches_workspace_command_defaults_and_global_ids() {
    let repository = PathBuf::from("/selected/repo");
    let local = RepositoryScope::repository(repository.clone());
    let id = "0199253e-6ec9-7342-9329-b95c1c006af2";
    for (arguments, expected) in [
        (vec!["coco", "signal", "list"], local.clone()),
        (vec!["coco", "signal", "list", "review/widget"], local),
        (
            vec!["coco", "signal", "list", "-a"],
            RepositoryScope::AllRepositories,
        ),
        (
            vec!["coco", "-a", "signal", "ls"],
            RepositoryScope::AllRepositories,
        ),
        (
            vec!["coco", "signal", "list", "review/widget", "-g"],
            RepositoryScope::AllRepositories,
        ),
        (
            vec!["coco", "-g", "signal", "ls", "review/widget"],
            RepositoryScope::AllRepositories,
        ),
        (
            vec!["coco", "signal", "ls", id],
            RepositoryScope::AllRepositories,
        ),
    ] {
        let cli = Cli::try_parse_from(arguments).unwrap();
        let all = cli.requests_all_repositories();
        let global = cli.requests_global_search();
        let Command::Signal {
            command: SignalCommand::List(args),
        } = cli.command
        else {
            panic!("expected signal list");
        };
        assert_eq!(
            list_scope(&args, repository.clone(), all, global).unwrap(),
            expected
        );
    }
}

#[test]
fn signal_help_exposes_positional_workspace_and_visible_aliases() {
    let mut cli = Cli::command();
    let signal = cli.find_subcommand_mut("signal").unwrap();
    let list = signal.find_subcommand_mut("list").unwrap();
    assert!(list.get_visible_aliases().any(|alias| alias == "ls"));
    let help = list.render_long_help().to_string();
    assert!(help.contains("[WORKSPACE]"));
    assert!(help.contains("--all-repos"));
    assert!(help.contains("--global"));
    assert!(!help.contains("--workspace"));
    assert!(signal.find_subcommand("type").is_none());
}

#[test]
fn signal_cli_rejects_conflicting_scopes_and_does_not_add_a_status_command() {
    for arguments in [
        vec!["coco", "signal", "list", "review/widget", "-a"],
        vec!["coco", "signal", "list", "-g"],
        vec!["coco", "signal", "list", "review/widget", "-ag"],
        vec!["coco", "signal", "list", "-w", "review/widget"],
        vec!["coco", "signal", "list", "--workspace", "review/widget"],
        vec!["coco", "signal", "status"],
    ] {
        assert!(
            Cli::try_parse_from(&arguments).is_err(),
            "unexpectedly parsed {arguments:?}"
        );
    }
}

#[test]
fn signal_cli_rejects_unbounded_pages_and_imperative_registration() {
    for limit in ["0", "101"] {
        assert!(Cli::try_parse_from(["coco", "signal", "list", "--limit", limit]).is_err());
    }
    assert!(
        Cli::try_parse_from([
            "coco",
            "signal",
            "type",
            "register",
            "review.requested",
            "--description",
            "Review",
            "--version",
            "0",
        ])
        .is_err()
    );
}

#[test]
fn signal_grants_require_an_explicit_catalog_but_a_catalog_does_not_grant_emission() {
    let command = ["coco", "mcp", "serve", "--repository", "/repo"];
    assert!(Cli::try_parse_from(command).is_ok());
    assert!(
        Cli::try_parse_from(
            command
                .into_iter()
                .chain(["--allow-emit", "review.requested"])
        )
        .is_err()
    );
    assert!(
        Cli::try_parse_from(command.into_iter().chain(["--signal-catalog", "/schemas"])).is_ok()
    );
    assert!(
        Cli::try_parse_from(command.into_iter().chain([
            "--signal-catalog",
            "/schemas",
            "--allow-emit",
            "review.requested@2"
        ]))
        .is_ok()
    );
}
