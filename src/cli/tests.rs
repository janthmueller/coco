use std::ffi::OsStr;
use std::path::PathBuf;

use clap::Parser;
use serde_json::json;

use crate::paths::CocoPaths;

use super::args::{Cli, Command, RepoCommand};
use super::commands::validate_scope_selection;
use super::jump::{jump_command, load_jump_target};
use super::output::phase_label;
use super::status::follow_stops_at;

#[test]
fn parses_workspace_creation_with_an_optional_profile() {
    let minimal = Cli::try_parse_from(["coco", "create", "auth"]);
    assert!(minimal.is_ok());

    let configured = Cli::try_parse_from([
        "coco",
        "create",
        "auth",
        "--base",
        "main",
        "--profile",
        "dev",
        "--model",
        "gpt-explicit",
    ]);
    let configured = configured.unwrap();
    let Command::Create(configured) = configured.command else {
        panic!("create did not parse as the create command");
    };
    assert_eq!(configured.model.as_deref(), Some("gpt-explicit"));

    let short_model = Cli::try_parse_from(["coco", "create", "auth", "-m", "gpt-short"]).unwrap();
    let Command::Create(short_model) = short_model.command else {
        panic!("create -m did not parse as the create command");
    };
    assert_eq!(short_model.model.as_deref(), Some("gpt-short"));

    let combined = Cli::try_parse_from([
        "coco",
        "create",
        "auth",
        "--send",
        "Fix the login flow",
        "--jump",
    ])
    .unwrap();
    let Command::Create(combined) = combined.command else {
        panic!("create did not parse as the create command");
    };
    assert_eq!(combined.send.as_deref(), Some("Fix the login flow"));
    assert!(combined.jump);

    let jump_only = Cli::try_parse_from(["coco", "create", "review", "-j"]).unwrap();
    let Command::Create(jump_only) = jump_only.command else {
        panic!("create -j did not parse as the create command");
    };
    assert!(jump_only.send.is_none());
    assert!(jump_only.jump);

    let send_only = Cli::try_parse_from(["coco", "create", "review", "-s", "Review it"]).unwrap();
    let Command::Create(send_only) = send_only.command else {
        panic!("create -s did not parse as the create command");
    };
    assert_eq!(send_only.send.as_deref(), Some("Review it"));
    assert!(!send_only.jump);

    assert!(Cli::try_parse_from(["coco", "create", "auth", "--goal", "work"]).is_err());
    assert!(Cli::try_parse_from(["coco", "create", "auth", "--context", "fresh"]).is_err());
    assert!(Cli::try_parse_from(["coco", "create", "auth", "--send", "  "]).is_err());
    assert!(Cli::try_parse_from(["coco", "create", "auth", "--model", "  "]).is_err());
    assert!(Cli::try_parse_from(["coco", "send", "auth", ""]).is_err());
    assert!(Cli::try_parse_from(["coco", "new", "auth"]).is_err());
}

#[test]
fn parses_native_workspace_forks_and_requires_an_explicit_source_for_compaction() {
    let fork = Cli::try_parse_from([
        "coco",
        "create",
        "review/follow-up",
        "--fork-from",
        "feat/source",
        "--compact",
        "-s",
        "Continue from the review",
    ])
    .unwrap();
    let Command::Create(fork) = fork.command else {
        panic!("workspace fork did not parse as create");
    };
    assert_eq!(fork.base, "HEAD");
    assert_eq!(fork.fork_from.as_deref(), Some("feat/source"));
    assert!(fork.compact);
    assert_eq!(fork.send.as_deref(), Some("Continue from the review"));

    assert!(
        Cli::try_parse_from(["coco", "create", "child", "--compact"]).is_err(),
        "compaction without a fork source must be rejected"
    );
    assert!(
        Cli::try_parse_from([
            "coco",
            "create",
            "child",
            "--fork-from",
            "source",
            "--base",
            "main",
        ])
        .is_err(),
        "a fork must derive its base from the source workspace"
    );
}

#[test]
fn repository_registration_is_a_nested_repo_command() {
    assert!(Cli::try_parse_from(["coco", "repo", "add"]).is_ok());
    assert!(Cli::try_parse_from(["coco", "repo", "add", "../source"]).is_ok());
    let listed = Cli::try_parse_from(["coco", "repo", "list", "--json"]).unwrap();
    assert!(matches!(
        listed.command,
        Command::Repo {
            command: RepoCommand::List { json: true }
        }
    ));
    assert!(Cli::try_parse_from(["coco", "init"]).is_err());
}

#[test]
fn model_discovery_is_a_daemon_wide_command() {
    let human = Cli::try_parse_from(["coco", "models"]).unwrap();
    assert!(matches!(human.command, Command::Models { json: false }));
    let json = Cli::try_parse_from(["coco", "models", "--json"]).unwrap();
    assert!(matches!(json.command, Command::Models { json: true }));
}

#[test]
fn parses_local_explicit_and_all_repository_scopes() {
    let local = Cli::try_parse_from(["coco", "ls"]).unwrap();
    assert!(local.scope_path.is_none());
    assert!(!local.all_repos);

    let explicit = Cli::try_parse_from(["coco", "../other", "status", "feat/login"]).unwrap();
    assert_eq!(explicit.scope_path, Some(PathBuf::from("../other")));
    assert!(!explicit.all_repos);

    let global = Cli::try_parse_from(["coco", "-a", "ls"]).unwrap();
    assert!(global.scope_path.is_none());
    assert!(global.all_repos);

    assert!(Cli::try_parse_from(["coco", "--all-repos", "status", "feat/login"]).is_ok());

    let conflicting = Cli::try_parse_from(["coco", "--all-repos", "../other", "ls"]).unwrap();
    assert!(
        validate_scope_selection(conflicting.scope_path.is_some(), conflicting.all_repos).is_err()
    );
    assert!(Cli::try_parse_from(["coco", "ls", "--all-repos"]).is_ok());
    let conflicting = Cli::try_parse_from(["coco", "../other", "ls", "-a"]).unwrap();
    assert!(
        validate_scope_selection(conflicting.scope_path.is_some(), conflicting.all_repos).is_err()
    );
}

#[test]
fn exposes_status_follow_and_jump_without_the_old_overlapping_commands() {
    assert!(Cli::try_parse_from(["coco", "status", "auth"]).is_ok());
    assert!(Cli::try_parse_from(["coco", "status", "auth", "--follow"]).is_ok());
    assert!(Cli::try_parse_from(["coco", "status", "auth", "--json"]).is_ok());
    assert!(Cli::try_parse_from(["coco", "status", "auth", "--follow", "--json"]).is_err());
    assert!(Cli::try_parse_from(["coco", "jump", "auth"]).is_ok());
    let decide = Cli::try_parse_from(["coco", "decide", "decision-123"]).unwrap();
    assert!(matches!(
        decide.command,
        Command::Decide { decision } if decision == "decision-123"
    ));
    assert!(Cli::try_parse_from(["coco", "show", "auth"]).is_err());
    assert!(Cli::try_parse_from(["coco", "watch", "auth"]).is_err());
}

#[test]
fn presents_stable_user_facing_workspace_states() {
    assert_eq!(phase_label("provisioning"), "Preparing worktree");
    assert_eq!(phase_label("active"), "Working");
    assert_eq!(phase_label("waiting_for_approval"), "Waiting for approval");
    assert_eq!(phase_label("idle"), "Ready");
    assert_eq!(phase_label("unavailable"), "Status unavailable");
    assert!(follow_stops_at("waiting_for_input"));
    assert!(follow_stops_at("system_error"));
    assert!(!follow_stops_at("active"));
}

#[tokio::test]
async fn builds_an_authenticated_jump_into_the_managed_worktree() {
    let directory = tempfile::tempdir().unwrap();
    let worktree = directory.path().join("worktree");
    std::fs::create_dir(&worktree).unwrap();
    let paths = CocoPaths {
        data_dir: directory.path().join("data"),
        database_path: directory.path().join("coco.db"),
        socket_path: directory.path().join("cocod.sock"),
        codex_endpoint_path: directory.path().join("codex-app-server.json"),
        codex_token_path: directory.path().join("codex-app-server.token"),
        worktrees_dir: directory.path().join("worktrees"),
        codex_home: directory.path().join("codex-home"),
    };
    std::fs::write(
        &paths.codex_endpoint_path,
        r#"{"schemaVersion":1,"url":"ws://127.0.0.1:45123"}"#,
    )
    .unwrap();
    std::fs::write(&paths.codex_token_path, "test-capability\n").unwrap();
    let response = json!({
        "workspace": {
            "worktreePath": worktree,
            "codexThreadId": "thread-123"
        }
    });

    let target = load_jump_target(&paths, &response).await.unwrap();
    let command = jump_command(&target, PathBuf::from("/opt/codex"));
    let command = command.as_std();
    let arguments = command
        .get_args()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    assert_eq!(command.get_program(), OsStr::new("/opt/codex"));
    assert_eq!(
        arguments,
        [
            "resume",
            "thread-123",
            "--remote",
            "ws://127.0.0.1:45123",
            "--remote-auth-token-env",
            "COCO_CODEX_REMOTE_CAPABILITY_TOKEN",
            "-C",
            target.worktree.to_str().unwrap(),
        ]
    );
    assert_eq!(command.get_current_dir(), Some(target.worktree.as_path()));
    assert!(command.get_envs().any(|(name, value)| {
        name == OsStr::new("COCO_CODEX_REMOTE_CAPABILITY_TOKEN")
            && value == Some(OsStr::new("test-capability"))
    }));
}
