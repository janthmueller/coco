use clap::{CommandFactory, Parser};

use super::super::args::{Cli, Command};

#[test]
fn parses_safe_close_reopen_and_clustered_delete_options() {
    let close =
        Cli::try_parse_from(["coco", "close", "feat/login", "-ty", "--discard-changes"]).unwrap();
    assert!(matches!(
        close.command,
        Command::Close(args)
            if args.workspace.as_deref() == Some("feat/login")
                && args.archive_thread
                && args.discard_changes
                && args.yes
    ));

    let reopen = Cli::try_parse_from(["coco", "reopen", "-g"]).unwrap();
    assert!(matches!(
        reopen.command,
        Command::Reopen(args) if args.workspace.is_none() && args.global
    ));

    let delete = Cli::try_parse_from(["coco", "delete", "feat/login", "-y"]).unwrap();
    assert!(matches!(
        delete.command,
        Command::Delete(args)
            if !args.keep_thread && !args.keep_branch && args.yes && !args.dry_run
    ));

    let closed = Cli::try_parse_from(["coco", "list", "--closed"]).unwrap();
    assert!(matches!(closed.command, Command::List { closed: true, .. }));
}

#[test]
fn retirement_help_keeps_lossy_options_explicit_and_bulk_scope_absent() {
    let command = Cli::command();
    let close = command.find_subcommand("close").unwrap();
    let close_names = close
        .get_arguments()
        .filter_map(|argument| argument.get_long())
        .collect::<Vec<_>>();
    assert!(close_names.contains(&"discard-changes"));
    assert!(close_names.contains(&"archive-thread"));
    assert!(!close_names.contains(&"all-repos"));
    let close_yes_help = close
        .get_arguments()
        .find(|argument| argument.get_long() == Some("yes"))
        .and_then(|argument| argument.get_help())
        .expect("close --yes should have help")
        .to_string();
    assert!(close_yes_help.contains("does not authorize discarding local changes"));

    let delete = command.find_subcommand("delete").unwrap();
    let delete_names = delete
        .get_arguments()
        .filter_map(|argument| argument.get_long())
        .collect::<Vec<_>>();
    assert!(delete_names.contains(&"keep-thread"));
    assert!(delete_names.contains(&"keep-branch"));
    assert!(delete_names.contains(&"discard-changes"));
    assert!(delete_names.contains(&"discard-unretained-commits"));
    assert!(!delete_names.contains(&"delete-thread"));
    assert!(!delete_names.contains(&"delete-branch"));
    assert!(!delete_names.contains(&"all-repos"));
    let delete_yes_help = delete
        .get_arguments()
        .find(|argument| argument.get_long() == Some("yes"))
        .and_then(|argument| argument.get_help())
        .expect("delete --yes should have help")
        .to_string();
    assert!(
        delete_yes_help
            .contains("does not authorize discarding local changes or unretained commits")
    );
}

#[test]
fn delete_retention_flags_are_explicit_and_old_effect_flags_are_rejected() {
    let parsed = Cli::try_parse_from([
        "coco",
        "delete",
        "fix/login",
        "--keep-thread",
        "--keep-branch",
        "--discard-unretained-commits",
        "-gn",
    ])
    .unwrap();
    assert!(matches!(parsed.command, Command::Delete(args) if
        args.keep_thread && args.keep_branch && args.discard_unretained_commits
            && args.global && args.dry_run));
    for old_flag in [
        "-t",
        "-b",
        "--delete-thread",
        "--delete-branch",
        "--discard-commits",
    ] {
        assert!(Cli::try_parse_from(["coco", "delete", "fix/login", old_flag]).is_err());
    }
}
