use std::path::PathBuf;

use clap::{CommandFactory, Parser};

use super::super::args::{Cli, Command, ModelCommand, RepoCommand};
use super::super::commands::validate_scope_selection;

#[test]
fn list_and_ls_are_visible_aliases_for_every_collection() {
    for spelling in ["list", "ls"] {
        let parsed = Cli::try_parse_from(["coco", spelling]).unwrap();
        assert!(matches!(parsed.command, Command::List { .. }));

        let parsed = Cli::try_parse_from(["coco", "repo", spelling]).unwrap();
        assert!(matches!(
            parsed.command,
            Command::Repo {
                command: RepoCommand::List { .. }
            }
        ));

        let parsed = Cli::try_parse_from(["coco", "model", spelling]).unwrap();
        assert!(matches!(
            parsed.command,
            Command::Model {
                command: ModelCommand::List { .. }
            }
        ));
    }

    let command = Cli::command();
    let list = command
        .find_subcommand("list")
        .expect("list subcommand must exist");
    assert!(list.get_visible_aliases().any(|alias| alias == "ls"));
    assert!(
        command
            .find_subcommand("models")
            .expect("compatibility models subcommand must exist")
            .is_hide_set(),
        "the old models spelling must not compete with model list in help"
    );

    for group in ["repo", "model"] {
        let list = command
            .find_subcommand(group)
            .unwrap_or_else(|| panic!("{group} subcommand must exist"))
            .find_subcommand("list")
            .unwrap_or_else(|| panic!("{group} list subcommand must exist"));
        assert!(
            list.get_visible_aliases().any(|alias| alias == "ls"),
            "{group} list must expose ls as a visible alias"
        );
    }
}

#[test]
fn repository_and_model_lists_keep_their_json_forms() {
    assert!(Cli::try_parse_from(["coco", "repo", "add"]).is_ok());
    assert!(Cli::try_parse_from(["coco", "repo", "add", "../source"]).is_ok());

    let repositories = Cli::try_parse_from(["coco", "repo", "ls", "--json"]).unwrap();
    assert!(matches!(
        repositories.command,
        Command::Repo {
            command: RepoCommand::List { json: true }
        }
    ));

    let models = Cli::try_parse_from(["coco", "model", "ls", "--json"]).unwrap();
    assert!(matches!(
        models.command,
        Command::Model {
            command: ModelCommand::List { json: true }
        }
    ));

    let compatibility = Cli::try_parse_from(["coco", "models", "--json"]).unwrap();
    assert!(matches!(
        compatibility.command,
        Command::Models { json: true }
    ));
    assert!(Cli::try_parse_from(["coco", "init"]).is_err());
}

#[test]
fn help_assigns_all_repos_to_overviews_and_global_to_single_targets() {
    let mut command = Cli::command();
    let list_help = command
        .find_subcommand_mut("list")
        .expect("list subcommand must exist")
        .render_long_help()
        .to_string();
    assert!(list_help.contains("--all-repos"));

    for name in ["status", "send", "jump", "diff"] {
        let mut command = Cli::command();
        let help = command
            .find_subcommand_mut(name)
            .unwrap_or_else(|| panic!("{name} subcommand must exist"))
            .render_long_help()
            .to_string();
        assert!(help.contains("--global"));
        assert!(!help.contains("--all-repos"));
    }

    for name in ["repo", "model", "create", "decide", "mcp"] {
        let mut command = Cli::command();
        let help = command
            .find_subcommand_mut(name)
            .unwrap_or_else(|| panic!("{name} subcommand must exist"))
            .render_long_help()
            .to_string();
        assert!(!help.contains("--all-repos"));
        assert!(!help.contains("--global"));
    }
}

#[test]
fn parses_repository_overviews_and_global_workspace_searches_separately() {
    let local = Cli::try_parse_from(["coco", "list"]).unwrap();
    assert!(local.scope_path.is_none());
    assert!(!local.requests_all_repositories());
    assert!(!local.requests_global_search());

    let explicit = Cli::try_parse_from(["coco", "../other", "status", "feat/login"]).unwrap();
    assert_eq!(explicit.scope_path, Some(PathBuf::from("../other")));
    assert!(!explicit.requests_all_repositories());
    assert!(!explicit.requests_global_search());

    for arguments in [["coco", "-a", "list"], ["coco", "list", "-a"]] {
        let parsed = Cli::try_parse_from(arguments).unwrap();
        assert!(parsed.requests_all_repositories());
        assert!(!parsed.requests_global_search());
    }

    for arguments in [
        vec!["coco", "-g", "status"],
        vec!["coco", "status", "-g"],
        vec!["coco", "-g", "status", "feat/login"],
        vec!["coco", "status", "feat/login", "-g"],
        vec!["coco", "-g", "jump"],
        vec!["coco", "jump", "-g"],
        vec!["coco", "-g", "jump", "feat/login"],
        vec!["coco", "jump", "feat/login", "-g"],
        vec!["coco", "-g", "diff"],
        vec!["coco", "diff", "-g"],
        vec!["coco", "-g", "diff", "feat/login"],
        vec!["coco", "diff", "feat/login", "-g"],
        vec!["coco", "-g", "send"],
        vec!["coco", "send", "-g"],
        vec!["coco", "-g", "send", "feat/login", "continue"],
        vec!["coco", "send", "feat/login", "continue", "-g"],
    ] {
        let parsed = Cli::try_parse_from(arguments).unwrap();
        assert!(!parsed.requests_all_repositories());
        assert!(parsed.requests_global_search());
    }
}

#[test]
fn status_supports_interactive_selection_details_and_single_workspace_following() {
    for arguments in [
        vec!["coco", "status"],
        vec!["coco", "status", "--json"],
        vec!["coco", "status", "--follow"],
        vec!["coco", "status", "-g"],
        vec!["coco", "status", "auth"],
        vec!["coco", "status", "auth", "--json"],
        vec!["coco", "status", "auth", "--follow"],
        vec!["coco", "status", "auth", "-g"],
    ] {
        assert!(
            Cli::try_parse_from(&arguments).is_ok(),
            "valid status form failed: {arguments:?}"
        );
    }

    for arguments in [
        vec!["coco", "status", "-a"],
        vec!["coco", "status", "auth", "-a"],
        vec!["coco", "status", "auth", "--follow", "--json"],
    ] {
        assert!(
            Cli::try_parse_from(&arguments).is_err(),
            "invalid status form parsed: {arguments:?}"
        );
    }
}

#[test]
fn incompatible_repository_scopes_are_rejected_before_rpc() {
    for arguments in [
        vec!["coco", "--all-repos", "../other", "list"],
        vec!["coco", "../other", "list", "-a"],
        vec!["coco", "--global", "../other", "status", "auth"],
        vec!["coco", "../other", "status", "auth", "-g"],
        vec!["coco", "-a", "-g", "status", "auth"],
    ] {
        let parsed = Cli::try_parse_from(arguments).unwrap();
        assert!(
            validate_scope_selection(
                parsed.scope_path.is_some(),
                parsed.requests_all_repositories(),
                parsed.requests_global_search(),
            )
            .is_err()
        );
    }
}

#[test]
fn collection_and_reference_flags_do_not_parse_on_the_wrong_commands() {
    for arguments in [
        vec!["coco", "repo", "list", "-a"],
        vec!["coco", "model", "list", "-a"],
        vec!["coco", "create", "auth", "-a"],
        vec!["coco", "send", "auth", "continue", "-a"],
        vec!["coco", "jump", "auth", "-a"],
        vec!["coco", "diff", "auth", "-a"],
        vec!["coco", "status", "-a"],
        vec!["coco", "list", "-g"],
        vec!["coco", "repo", "list", "-g"],
        vec!["coco", "model", "list", "-g"],
        vec!["coco", "create", "auth", "-g"],
        vec!["coco", "decide", "decision-123", "-g"],
        vec!["coco", "mcp", "serve", "--repository", ".", "-g"],
    ] {
        assert!(
            Cli::try_parse_from(&arguments).is_err(),
            "wrong-scope flag parsed for {arguments:?}"
        );
    }
}

#[tokio::test]
async fn leading_scope_flags_are_rejected_by_incompatible_commands() {
    for arguments in [
        vec!["coco", "-a", "repo", "list"],
        vec!["coco", "-a", "model", "list"],
        vec!["coco", "-a", "create", "auth"],
        vec!["coco", "-a", "send", "auth", "continue"],
        vec!["coco", "-a", "status", "auth"],
        vec!["coco", "-g", "list"],
        vec!["coco", "-g", "repo", "list"],
        vec!["coco", "-g", "model", "list"],
        vec!["coco", "-g", "create", "auth"],
    ] {
        let cli = Cli::try_parse_from(&arguments).unwrap();
        let error = super::super::commands::run(cli)
            .await
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("does not accept")
                || error.contains("requires one repository")
                || error.contains("targets one workspace")
                || error.contains("not --global")
                || error.contains("requires a workspace")
                || error.contains("shows an overview"),
            "unexpected scope error for {arguments:?}: {error}"
        );
    }
}

#[test]
fn old_overlapping_status_commands_remain_absent() {
    assert!(Cli::try_parse_from(["coco", "jump", "auth"]).is_ok());
    let decide = Cli::try_parse_from(["coco", "decide", "decision-123"]).unwrap();
    assert!(matches!(
        decide.command,
        Command::Decide { decision, choice: None } if decision == "decision-123"
    ));
    assert!(Cli::try_parse_from(["coco", "show", "auth"]).is_err());
    assert!(Cli::try_parse_from(["coco", "watch", "auth"]).is_err());
}
