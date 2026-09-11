use clap::{CommandFactory, Parser};

use super::super::args::{Cli, Command, LimitField, LimitsCommand};

#[test]
fn limits_support_local_and_global_workspace_selection() {
    for arguments in [
        vec!["coco", "limits", "show", "review/api"],
        vec!["coco", "limits", "show"],
        vec!["coco", "limits", "show", "review/api", "-g"],
        vec!["coco", "-g", "limits", "show", "review/api"],
        vec!["coco", "limits", "reset", "review/api"],
    ] {
        let parsed = Cli::try_parse_from(&arguments)
            .unwrap_or_else(|error| panic!("valid limits form failed for {arguments:?}: {error}"));
        assert!(matches!(parsed.command, Command::Limits { .. }));
    }

    let global = Cli::try_parse_from(["coco", "limits", "show", "review/api", "-g"])
        .expect("nested global scope must parse");
    assert!(global.requests_global_search());
    assert!(!global.requests_all_repositories());

    assert!(Cli::try_parse_from(["coco", "limits", "show", "review/api", "-a"]).is_err());
    assert!(Cli::try_parse_from(["coco", "limits", "set", "review/api", "-a"]).is_err());
}

#[test]
fn limits_parse_human_sizes_cpu_capacity_and_selective_clears() {
    let parsed = Cli::try_parse_from([
        "coco",
        "limits",
        "set",
        "review/api",
        "--memory-high",
        "1.5GiB",
        "--memory-max",
        "2GB",
        "--cpu-max",
        "1.505",
        "--cpu-weight",
        "321",
        "--tasks-max",
        "128",
        "--clear",
        "cpu-weight",
        "--json",
    ])
    .expect("valid resource limits must parse");
    let Command::Limits {
        command: LimitsCommand::Set(args),
    } = parsed.command
    else {
        panic!("limits set parsed as a different command");
    };
    assert_eq!(args.workspace.as_deref(), Some("review/api"));
    assert_eq!(args.memory_high, Some(1_610_612_736));
    assert_eq!(args.memory_max, Some(2_000_000_000));
    assert_eq!(args.cpu_max, Some(1_505));
    assert_eq!(args.cpu_weight, Some(321));
    assert_eq!(args.tasks_max, Some(128));
    assert_eq!(args.clear, vec![LimitField::CpuWeight]);
    assert!(args.json);

    let fractional = Cli::try_parse_from([
        "coco",
        "limits",
        "set",
        "review/api",
        "--memory-max",
        "1.1GiB",
    ])
    .expect("fractional binary sizes must parse");
    let Command::Limits {
        command: LimitsCommand::Set(fractional),
    } = fractional.command
    else {
        panic!("limits set parsed as a different command");
    };
    assert_eq!(fractional.memory_max, Some(1_181_116_006));

    for invalid in [
        vec!["coco", "limits", "set", "review/api", "--memory-max", "0"],
        vec!["coco", "limits", "set", "review/api", "--memory-max", "1XB"],
        vec!["coco", "limits", "set", "review/api", "--cpu-max", "0"],
        vec!["coco", "limits", "set", "review/api", "--cpu-max", "1.0001"],
        vec![
            "coco",
            "limits",
            "set",
            "review/api",
            "--cpu-weight",
            "10001",
        ],
        vec!["coco", "limits", "set", "review/api", "--tasks-max", "0"],
    ] {
        assert!(
            Cli::try_parse_from(&invalid).is_err(),
            "invalid limit parsed: {invalid:?}"
        );
    }
}

#[test]
fn limits_help_explains_each_control_without_exposing_systemd_syntax() {
    let command = Cli::command();
    let limits = command
        .find_subcommand("limits")
        .expect("limits command must exist");
    for name in ["show", "set", "reset"] {
        assert!(limits.find_subcommand(name).is_some(), "missing {name}");
    }
    let help = limits
        .find_subcommand("set")
        .expect("limits set must exist")
        .clone()
        .render_long_help()
        .to_string();
    for option in [
        "--memory-high",
        "--memory-max",
        "--cpu-max",
        "--cpu-weight",
        "--tasks-max",
        "--clear",
    ] {
        assert!(help.contains(option), "limits help omitted {option}");
    }
    assert!(!help.contains("systemctl"));
}
