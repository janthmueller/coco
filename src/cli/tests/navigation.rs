use super::*;
use crate::cli::args::{NavigationArgs, ShellCommand, ShellKind, WorkspaceSort};

#[test]
fn path_and_cd_follow_existing_local_global_and_uuid_target_shapes() {
    for command in ["path", "cd"] {
        let parsed = Cli::try_parse_from(["coco", command]).unwrap();
        assert!(matches!(
            parsed.command,
            Command::Path(NavigationArgs {
                workspace: None,
                ..
            }) | Command::Cd(NavigationArgs {
                workspace: None,
                ..
            })
        ));
        for args in [
            vec!["coco", command, "fix/login", "-g"],
            vec!["coco", "-g", command, "fix/login"],
        ] {
            let parsed = Cli::try_parse_from(args).unwrap();
            assert!(parsed.requests_global_search());
        }
        let parsed = Cli::try_parse_from(["coco", "../repo", command, "fix/login"]).unwrap();
        assert_eq!(parsed.scope_path, Some(PathBuf::from("../repo")));
        assert!(Cli::try_parse_from(["coco", command, "-a"]).is_err());
        for (args, expected) in [
            (vec![], WorkspaceSort::Recent),
            (vec!["--sort", "recent"], WorkspaceSort::Recent),
            (vec!["--sort", "name"], WorkspaceSort::Name),
        ] {
            let mut argv = vec!["coco", command];
            argv.extend(args);
            let parsed = Cli::try_parse_from(argv).unwrap();
            let (Command::Path(args) | Command::Cd(args)) = parsed.command else {
                panic!("expected navigation");
            };
            assert_eq!(args.sort, expected);
        }
        assert!(Cli::try_parse_from(["coco", command, "--sort", "state"]).is_err());
    }
}

#[test]
fn shell_initialization_requires_an_explicit_supported_shell() {
    for name in ["bash", "zsh", "fish"] {
        assert!(matches!(
            Cli::try_parse_from(["coco", "shell", "init", name])
                .unwrap()
                .command,
            Command::Shell {
                command: ShellCommand::Init { .. }
            }
        ));
    }
    for args in [
        vec!["coco", "shell", "init"],
        vec!["coco", "shell", "init", "sh"],
    ] {
        assert!(Cli::try_parse_from(args).is_err());
    }
    assert!(matches!(
        Cli::try_parse_from(["coco", "shell", "init", "fish"])
            .unwrap()
            .command,
        Command::Shell {
            command: ShellCommand::Init {
                shell: ShellKind::Fish
            }
        }
    ));
}
