use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "coco", version, about = "Coordinate isolated Codex work")]
pub struct Cli {
    #[command(subcommand)]
    pub(super) command: Command,
}

#[derive(Debug, Subcommand)]
pub(super) enum Command {
    /// Manage repositories known to CoCo.
    Repo {
        #[command(subcommand)]
        command: RepoCommand,
    },
    /// Create a fresh Codex workspace in an isolated worktree.
    Create(CreateArgs),
    /// List workspaces in the current repository.
    Ls {
        /// Emit stable, machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Show a workspace's current state, optionally following it until it pauses.
    Status {
        /// Workspace name or ID.
        workspace: String,
        /// Keep updating until the workspace becomes ready, pauses, or finishes.
        #[arg(long, conflicts_with = "json")]
        follow: bool,
        /// Emit stable, machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Start the first or next turn for a ready workspace.
    Send {
        /// Workspace name or ID.
        workspace: String,
        /// Instruction to send to Codex.
        #[arg(value_parser = non_empty_message)]
        message: String,
    },
    /// Open the workspace's existing Codex thread in its managed worktree.
    /// Leaving with /quit or /exit does not cancel active work.
    Jump {
        /// Workspace name or ID.
        workspace: String,
    },
    /// Show all tracked and untracked changes from the immutable base.
    Diff { workspace: String },
    /// Run CoCo as a local MCP server.
    Mcp {
        #[command(subcommand)]
        command: McpCommand,
    },
}

#[derive(Debug, Subcommand)]
pub(super) enum RepoCommand {
    /// Register the Git repository containing PATH.
    Add {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
}

#[derive(Debug, Args)]
pub(super) struct CreateArgs {
    /// Short workspace name, also used to derive its branch and worktree.
    pub(super) name: String,
    /// Git revision from which to prepare the workspace.
    #[arg(long, default_value = "HEAD")]
    pub(super) base: String,
    /// Apply `[profiles.<PROFILE>]` from `$CODEX_HOME/config.toml` to the thread.
    #[arg(long, default_value = "default")]
    pub(super) profile: String,
    /// Start the first turn with MESSAGE after creating the workspace.
    #[arg(long, value_name = "MESSAGE", value_parser = non_empty_message)]
    pub(super) send: Option<String>,
    /// Open the workspace in the Codex terminal UI after creating it.
    #[arg(long)]
    pub(super) jump: bool,
}

fn non_empty_message(value: &str) -> Result<String, String> {
    if value.trim().is_empty() {
        Err("message must not be empty".to_owned())
    } else {
        Ok(value.to_owned())
    }
}

#[derive(Debug, Subcommand)]
pub(super) enum McpCommand {
    /// Serve repository-scoped CoCo tools over stdio.
    Serve {
        #[arg(long)]
        repository: PathBuf,
        /// Advertise the mutating workspaces.send tool.
        #[arg(long)]
        allow_send: bool,
    },
}
