use std::path::PathBuf;

use clap::Parser;

use crate::protocol::{
    WorkspaceBaseRequest, WorkspaceChangesRequest, WorkspaceContextRequest, WorkspaceContextSource,
    WorkspaceWorktreeRequest,
};

use super::super::args::{Cli, Command};
use super::super::commands::normalize_create_args;

fn create_args(arguments: &[&str]) -> super::super::args::CreateArgs {
    let cli = Cli::try_parse_from(arguments).unwrap();
    let Command::Create(args) = cli.command else {
        panic!("expected create command");
    };
    args
}

#[test]
fn normalizes_independent_base_context_and_worktree_choices() {
    let args = create_args(&[
        "coco",
        "create",
        "review",
        "--base",
        "main",
        "--context",
        "investigation",
        "--branch",
        "review/custom",
        "--carry-changes",
        "--send",
        "Review the result",
        "--jump",
    ]);

    let (params, message, jump) =
        normalize_create_args(PathBuf::from("/repo"), args, "create-operation".to_owned()).unwrap();

    assert_eq!(
        params.context,
        WorkspaceContextRequest::Fork {
            source: WorkspaceContextSource::Reference {
                reference: "investigation".to_owned(),
            },
            compact: false,
        }
    );
    assert_eq!(
        params.worktree,
        WorkspaceWorktreeRequest::NewBranch {
            branch: Some("review/custom".to_owned()),
            base: WorkspaceBaseRequest::Revision {
                revision: "main".to_owned(),
            },
        }
    );
    assert_eq!(params.changes, WorkspaceChangesRequest::CarryTracked);
    assert_eq!(message.as_deref(), Some("Review the result"));
    assert!(jump);
}

#[test]
fn unified_context_flag_supports_the_compact_short_cluster() {
    let explicit = create_args(&["coco", "create", "child", "--context", "parent"]);
    assert_eq!(explicit.context.as_deref(), Some("parent"));
    assert!(!explicit.compact_context);

    let compact = create_args(&["coco", "create", "child", "-Cc", "0199-thread"]);
    assert_eq!(compact.context.as_deref(), Some("0199-thread"));
    assert!(compact.compact_context);

    assert!(Cli::try_parse_from(["coco", "create", "child", "-cC", "0199-thread"]).is_err());
    assert!(Cli::try_parse_from(["coco", "create", "child", "-w", "parent"]).is_err());
    assert!(Cli::try_parse_from(["coco", "create", "child", "-t", "0199-thread"]).is_err());
}

#[test]
fn dirty_selects_all_visible_changes_independently_from_detached_mode() {
    let args = create_args(&[
        "coco",
        "create",
        "scratch",
        "--base-workspace",
        "source",
        "--context",
        "0199-native-thread",
        "--compact-context",
        "--dirty",
        "--detached",
    ]);

    let (params, message, jump) =
        normalize_create_args(PathBuf::from("/repo"), args, "create-dirty".to_owned()).unwrap();

    assert_eq!(
        params.context,
        WorkspaceContextRequest::Fork {
            source: WorkspaceContextSource::Reference {
                reference: "0199-native-thread".to_owned(),
            },
            compact: true,
        }
    );
    assert_eq!(
        params.worktree,
        WorkspaceWorktreeRequest::Detached {
            base: WorkspaceBaseRequest::Workspace {
                workspace: "source".to_owned(),
            },
        }
    );
    assert_eq!(
        params.changes,
        WorkspaceChangesRequest::CarryTrackedAndUntracked
    );
    assert!(message.is_none());
    assert!(!jump);
}

#[test]
fn checkout_selects_an_existing_branch_without_an_independent_base() {
    let args = create_args(&["coco", "create", "feat/login", "--checkout", "feat/login"]);
    let (params, _, _) =
        normalize_create_args(PathBuf::from("/repo"), args, "create-existing".to_owned()).unwrap();
    assert_eq!(
        params.worktree,
        WorkspaceWorktreeRequest::ExistingBranch {
            branch: "feat/login".to_owned(),
        }
    );
    assert_eq!(params.context, WorkspaceContextRequest::Fresh);

    assert!(
        Cli::try_parse_from([
            "coco",
            "create",
            "invalid",
            "--checkout",
            "feat/login",
            "--base",
            "main",
        ])
        .is_err()
    );
}

#[test]
fn legacy_fork_from_remains_a_hidden_coupled_alias() {
    let args = create_args(&["coco", "create", "child", "--fork-from", "source"]);
    let (params, _, _) =
        normalize_create_args(PathBuf::from("/repo"), args, "create-child".to_owned()).unwrap();
    assert_eq!(
        params.worktree,
        WorkspaceWorktreeRequest::NewBranch {
            branch: None,
            base: WorkspaceBaseRequest::Workspace {
                workspace: "source".to_owned(),
            },
        }
    );
    assert_eq!(
        params.context,
        WorkspaceContextRequest::Fork {
            source: WorkspaceContextSource::Workspace {
                workspace: "source".to_owned(),
            },
            compact: false,
        }
    );
}

#[test]
fn clap_rejects_ambiguous_workspace_creation_modes() {
    for arguments in [
        vec![
            "coco",
            "create",
            "invalid",
            "--context",
            "one",
            "--fork-from",
            "two",
        ],
        vec!["coco", "create", "invalid", "--branch", "one", "--detached"],
        vec![
            "coco",
            "create",
            "invalid",
            "--checkout",
            "one",
            "--detached",
        ],
    ] {
        assert!(Cli::try_parse_from(arguments).is_err());
    }
}

#[test]
fn dirty_and_detached_short_flags_remain_independent() {
    let branch_args = create_args(&["coco", "create", "branch-backed", "-d"]);
    let (branch_params, _, _) = normalize_create_args(
        PathBuf::from("/repo"),
        branch_args,
        "create-branch-backed".to_owned(),
    )
    .unwrap();
    assert!(matches!(
        branch_params.worktree,
        WorkspaceWorktreeRequest::NewBranch { .. }
    ));
    assert_eq!(
        branch_params.changes,
        WorkspaceChangesRequest::CarryTrackedAndUntracked
    );

    let detached_args = create_args(&["coco", "create", "detached", "-dD"]);
    let (detached_params, _, _) = normalize_create_args(
        PathBuf::from("/repo"),
        detached_args,
        "create-detached".to_owned(),
    )
    .unwrap();
    assert!(matches!(
        detached_params.worktree,
        WorkspaceWorktreeRequest::Detached { .. }
    ));
    assert_eq!(
        detached_params.changes,
        WorkspaceChangesRequest::CarryTrackedAndUntracked
    );
}

#[test]
fn carry_untracked_requires_an_explicit_dirty_state_transfer() {
    let args = create_args(&["coco", "create", "invalid", "--carry-untracked"]);
    assert!(
        normalize_create_args(PathBuf::from("/repo"), args, "create-invalid".to_owned(),).is_err()
    );

    let args = create_args(&[
        "coco",
        "create",
        "valid",
        "--carry-changes",
        "--carry-untracked",
    ]);
    let (params, _, _) =
        normalize_create_args(PathBuf::from("/repo"), args, "create-valid".to_owned()).unwrap();
    assert_eq!(
        params.changes,
        WorkspaceChangesRequest::CarryTrackedAndUntracked
    );
}
