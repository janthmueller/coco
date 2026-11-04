use super::*;
use crate::cli::args::JumpSort;

#[test]
fn jump_defaults_to_recent_and_can_explicitly_select_name_order() {
    for (tail, expected) in [
        (vec![], JumpSort::Recent),
        (vec!["--sort", "recent"], JumpSort::Recent),
        (vec!["--sort", "name"], JumpSort::Name),
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
    assert!(Cli::try_parse_from(["coco", "send", "--sort", "recent"]).is_err());
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
