use std::ffi::OsStr;
use std::path::PathBuf;

use clap::{CommandFactory, Parser};

use crate::protocol::{WorkspaceAttachLaunch, WorkspaceDiffResult};

use super::args::{Cli, Command};
use super::jump::{JumpTarget, fresh_command, resume_command};
use super::output::{phase_label, render_diff};

mod collections;
mod creation;
mod retirement;

#[test]
fn create_help_describes_the_codex_named_profile_file() {
    let mut command = Cli::command();
    let help = command
        .find_subcommand_mut("create")
        .expect("create subcommand must exist")
        .render_long_help()
        .to_string();

    assert!(help.contains("$CODEX_HOME/<PROFILE>.config.toml"));
    assert!(!help.contains("[profiles.<PROFILE>]"));
    for option in [
        "--base-workspace",
        "--context",
        "--compact-context",
        "--branch",
        "--checkout",
        "--detached",
        "--carry-changes",
        "--carry-untracked",
        "--dirty",
    ] {
        assert!(help.contains(option), "create help omitted {option}");
    }
    assert!(help.contains("-c, --context"));
    assert!(help.contains("-C, --compact-context"));
    assert!(!help.contains("--fork-from"));
}

#[test]
fn help_describes_separate_worktrees_without_implying_security_isolation() {
    let mut command = Cli::command();
    let root_help = command.render_long_help().to_string();
    assert!(root_help.contains("separate worktrees"));
    assert!(!root_help.contains("isolated"));

    let mut command = Cli::command();
    let create_help = command
        .find_subcommand_mut("create")
        .expect("create subcommand must exist")
        .render_long_help()
        .to_string();
    assert!(create_help.contains("separate worktree"));
    assert!(!create_help.contains("isolated"));
}

#[test]
fn diff_help_and_human_output_disclose_bounded_patches() {
    let mut command = Cli::command();
    let help = command
        .find_subcommand_mut("diff")
        .expect("diff subcommand must exist")
        .render_long_help()
        .to_string();
    assert!(help.contains("bounded tracked patch"));
    assert!(!help.contains("Show all tracked"));

    let rendered = render_diff(&WorkspaceDiffResult {
        patch: "diff --git a/file b/file\n".to_owned(),
        patch_truncated: true,
        untracked_paths: vec![PathBuf::from("new.txt")],
    });
    assert_eq!(
        rendered,
        concat!(
            "diff --git a/file b/file\n",
            "Warning: tracked patch output was truncated.\n",
            "Untracked:\n",
            "new.txt\n",
        )
    );

    assert_eq!(
        render_diff(&WorkspaceDiffResult {
            patch: String::new(),
            patch_truncated: true,
            untracked_paths: Vec::new(),
        }),
        "Warning: tracked patch output was truncated.\n"
    );
    assert_eq!(
        render_diff(&WorkspaceDiffResult {
            patch: String::new(),
            patch_truncated: false,
            untracked_paths: Vec::new(),
        }),
        "No changes.\n"
    );
}

#[test]
fn parses_workspace_creation_with_an_optional_profile() {
    let minimal = Cli::try_parse_from(["coco", "create", "auth"]);
    assert!(minimal.is_ok());
    let prompted = Cli::try_parse_from(["coco", "create"]).unwrap();
    assert!(matches!(
        prompted.command,
        Command::Create(super::args::CreateArgs { name: None, .. })
    ));

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
    let with_context =
        Cli::try_parse_from(["coco", "create", "auth", "--context", "fresh"]).unwrap();
    assert!(matches!(
        with_context.command,
        Command::Create(super::args::CreateArgs {
            context: Some(reference),
            ..
        }) if reference == "fresh"
    ));
    assert!(Cli::try_parse_from(["coco", "create", "auth", "--context", "  "]).is_err());
    assert!(Cli::try_parse_from(["coco", "create", "auth", "--send", "  "]).is_err());
    assert!(Cli::try_parse_from(["coco", "create", "auth", "--model", "  "]).is_err());
    assert!(Cli::try_parse_from(["coco", "new", "auth"]).is_err());
}

#[test]
fn parses_prompted_replayable_and_waited_sends() {
    assert!(Cli::try_parse_from(["coco", "send", "auth", ""]).is_err());
    let prompted_send = Cli::try_parse_from(["coco", "send"]).unwrap();
    assert!(matches!(
        prompted_send.command,
        Command::Send {
            workspace: None,
            message: None,
            ..
        }
    ));
    let prompted_message = Cli::try_parse_from(["coco", "send", "auth"]).unwrap();
    assert!(matches!(
        prompted_message.command,
        Command::Send {
            workspace: Some(workspace),
            message: None,
            ..
        } if workspace == "auth"
    ));
    let send = Cli::try_parse_from([
        "coco",
        "send",
        "auth",
        "continue",
        "--operation-id",
        "send-auth-1",
    ])
    .unwrap();
    assert!(matches!(
        send.command,
        Command::Send {
            operation_id: Some(operation_id),
            ..
        } if operation_id == "send-auth-1"
    ));
    let waited = Cli::try_parse_from(["coco", "send", "auth", "continue", "--wait"]).unwrap();
    assert!(matches!(waited.command, Command::Send { wait: true, .. }));
    assert!(
        Cli::try_parse_from(["coco", "send", "auth", "continue", "--operation-id", " "]).is_err()
    );
    assert!(Cli::try_parse_from(["coco", "--no-input", "status", "auth"]).is_ok());
    assert!(Cli::try_parse_from(["coco", "status", "auth", "--no-input"]).is_ok());
}

#[test]
fn parses_deterministic_approval_choices() {
    let explicit_decision =
        Cli::try_parse_from(["coco", "decide", "decision-123", "--choice", "2"]).unwrap();
    assert!(matches!(
        explicit_decision.command,
        Command::Decide {
            decision,
            choice: Some(2),
        } if decision == "decision-123"
    ));
    assert!(Cli::try_parse_from(["coco", "decide", "decision-123", "--choice", "0"]).is_err());
}

#[test]
fn parses_native_workspace_forks_and_requires_an_explicit_context_for_compaction() {
    let fork = Cli::try_parse_from([
        "coco",
        "create",
        "review/follow-up",
        "--fork-from",
        "feat/source",
        "--compact-context",
        "-s",
        "Continue from the review",
    ])
    .unwrap();
    let Command::Create(fork) = fork.command else {
        panic!("workspace fork did not parse as create");
    };
    assert_eq!(fork.base, None);
    assert_eq!(fork.fork_from.as_deref(), Some("feat/source"));
    assert!(fork.compact_context);
    assert_eq!(fork.send.as_deref(), Some("Continue from the review"));

    let no_source = Cli::try_parse_from(["coco", "create", "child", "--compact-context"]).unwrap();
    let Command::Create(no_source) = no_source.command else {
        panic!("create did not parse as the create command");
    };
    assert!(
        super::commands::normalize_create_args(
            PathBuf::from("/repo"),
            no_source,
            "create-child".to_owned(),
        )
        .is_err(),
        "compaction without a fork source must be rejected before RPC"
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
fn presents_stable_user_facing_workspace_states() {
    assert_eq!(phase_label("provisioning"), "Preparing worktree");
    assert_eq!(phase_label("active"), "Working");
    assert_eq!(phase_label("waiting_for_approval"), "Waiting for approval");
    assert_eq!(phase_label("idle"), "Ready");
    assert_eq!(phase_label("unavailable"), "Status unavailable");
    assert_eq!(phase_label("closed"), "Closed");
}

#[test]
fn builds_authenticated_resume_and_fresh_jump_commands() {
    let directory = tempfile::tempdir().unwrap();
    let worktree = directory.path().join("worktree");
    std::fs::create_dir(&worktree).unwrap();
    let target = JumpTarget {
        workspace_id: "workspace-123".to_owned(),
        worktree,
        profile: "dev".to_owned(),
        model: Some("gpt-explicit".to_owned()),
        launch: WorkspaceAttachLaunch::Resume {
            thread_id: "thread-123".to_owned(),
            lease_id: "lease-123".to_owned(),
        },
        endpoint_url: "ws://127.0.0.1:45123".to_owned(),
        capability_token: "test-capability".to_owned(),
        codex_home: directory.path().join("codex-home"),
    };

    let command = resume_command(&target, "thread-123", PathBuf::from("/opt/codex"));
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
    assert!(command.get_envs().any(|(name, value)| {
        name == OsStr::new("CODEX_HOME") && value == Some(target.codex_home.as_os_str())
    }));

    let command = fresh_command(
        &target,
        PathBuf::from("/opt/codex"),
        "ws://127.0.0.1:45234",
        "relay-capability",
    );
    let command = command.as_std();
    let arguments = command
        .get_args()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        arguments,
        [
            "--remote",
            "ws://127.0.0.1:45234",
            "--remote-auth-token-env",
            "COCO_CODEX_REMOTE_CAPABILITY_TOKEN",
            "-C",
            target.worktree.to_str().unwrap(),
            "--profile",
            "dev",
            "--model",
            "gpt-explicit",
        ]
    );
    assert!(command.get_envs().any(|(name, value)| {
        name == OsStr::new("COCO_CODEX_REMOTE_CAPABILITY_TOKEN")
            && value == Some(OsStr::new("relay-capability"))
    }));
}
