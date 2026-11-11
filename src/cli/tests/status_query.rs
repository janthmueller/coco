use super::super::args::{Cli, Command};
use super::super::status::{CollectionQuery, StatusProjection};
use clap::Parser;

fn query(arguments: &[&str]) -> anyhow::Result<CollectionQuery> {
    let parsed = Cli::try_parse_from(arguments)?;
    let Command::Status(args) = parsed.command else {
        unreachable!()
    };
    CollectionQuery::new(
        &args.sort,
        &args.state,
        &args.exclude_state,
        StatusProjection {
            resources: args.resources,
            usage: args.usage,
            quota: args.quota,
            clients: args.clients,
        },
    )
}

#[test]
fn multi_key_sort_accepts_commas_repeated_flags_and_per_key_direction() {
    for arguments in [
        vec!["coco", "status", "--sort", "state,name"],
        vec!["coco", "status", "--sort", "state", "--sort", "name"],
        vec!["coco", "status", "-ar", "--sort=state,-memory,name"],
        vec!["coco", "status", "-r", "--sort", "-cpu"],
        vec!["coco", "status", "-u", "--sort", "-tokens,context"],
        vec!["coco", "status", "--sort", "repository,workspace"],
        vec!["coco", "status", "--sort", "recent"],
        vec!["coco", "status", "-af", "--sort", "state,recent"],
        vec!["coco", "status", "--sort=-recent,name"],
        vec!["coco", "status", "--sort", "state", "--sort", "recent"],
    ] {
        assert!(query(&arguments).is_ok(), "{arguments:?}");
    }
}

#[test]
fn invalid_unknown_duplicate_and_unrequested_metric_keys_fail_before_rpc() {
    for value in [
        "",
        "state,,name",
        "state,",
        "state,state",
        "repo,repository",
        "name,-name",
        "cost",
        "clients",
        "recent,recent",
        "recent,-recent",
        "--cpu",
        "memory",
        "cpu",
        "procs",
        "tokens",
        "context",
    ] {
        assert!(
            query(&["coco", "status", "--sort", value]).is_err(),
            "{value}"
        );
    }
}

#[test]
fn filters_repeat_combine_with_tree_follow_json_but_not_an_explicit_target() {
    for arguments in [
        vec!["coco", "status", "--exclude-state", "unloaded"],
        vec![
            "coco",
            "status",
            "--state",
            "working,needs-input",
            "--state",
            "needs-approval",
        ],
        vec![
            "coco",
            "status",
            "-atf",
            "--exclude-state",
            "unloaded,prepared",
        ],
        vec![
            "coco",
            "status",
            "-a",
            "--json",
            "--exclude-state",
            "not_loaded",
        ],
        vec![
            "coco",
            "status",
            "--state",
            "working",
            "--exclude-state",
            "working",
        ],
    ] {
        assert!(query(&arguments).is_ok(), "{arguments:?}");
    }
    for arguments in [
        vec!["coco", "status", "work", "--state", "ready"],
        vec!["coco", "status", "work", "--exclude-state", "unloaded"],
        vec!["coco", "status", "--tree", "--sort", "state"],
        vec!["coco", "status", "--tree", "--sort", "recent"],
        vec!["coco", "status", "work", "--sort", "recent"],
        vec!["coco", "list", "--sort", "recent"],
        vec!["coco", "status", "--state", "closed"],
        vec!["coco", "status", "--state", "invalid"],
        vec!["coco", "status", "--exclude-state", ""],
    ] {
        assert!(query(&arguments).is_err(), "{arguments:?}");
    }
}
