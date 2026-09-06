use std::ffi::OsStr;
use std::path::PathBuf;

use clap::Parser;
use serde_json::json;

use crate::paths::CocoPaths;

use super::args::Cli;
use super::jump::{jump_command, load_jump_target};
use super::output::phase_label;
use super::status::follow_stops_at;

#[test]
fn parses_task_preparation_with_an_optional_profile() {
    let minimal = Cli::try_parse_from(["coco", "new", "auth"]);
    assert!(minimal.is_ok());

    let configured =
        Cli::try_parse_from(["coco", "new", "auth", "--base", "main", "--profile", "dev"]);
    assert!(configured.is_ok());

    assert!(Cli::try_parse_from(["coco", "new", "auth", "--goal", "work"]).is_err());
    assert!(Cli::try_parse_from(["coco", "new", "auth", "--context", "fresh"]).is_err());
}

#[test]
fn repository_registration_is_a_nested_repo_command() {
    assert!(Cli::try_parse_from(["coco", "repo", "add"]).is_ok());
    assert!(Cli::try_parse_from(["coco", "repo", "add", "../source"]).is_ok());
    assert!(Cli::try_parse_from(["coco", "init"]).is_err());
}

#[test]
fn exposes_status_follow_and_jump_without_the_old_overlapping_commands() {
    assert!(Cli::try_parse_from(["coco", "status", "auth"]).is_ok());
    assert!(Cli::try_parse_from(["coco", "status", "auth", "--follow"]).is_ok());
    assert!(Cli::try_parse_from(["coco", "status", "auth", "--json"]).is_ok());
    assert!(Cli::try_parse_from(["coco", "status", "auth", "--follow", "--json"]).is_err());
    assert!(Cli::try_parse_from(["coco", "jump", "auth"]).is_ok());
    assert!(Cli::try_parse_from(["coco", "show", "auth"]).is_err());
    assert!(Cli::try_parse_from(["coco", "watch", "auth"]).is_err());
}

#[test]
fn presents_stable_user_facing_task_states() {
    assert_eq!(phase_label("provisioning"), "Preparing worktree");
    assert_eq!(phase_label("active"), "Working");
    assert_eq!(phase_label("waiting_for_approval"), "Waiting for approval");
    assert_eq!(phase_label("idle"), "Ready");
    assert!(follow_stops_at("waiting_for_input"));
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
        "task": {
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
