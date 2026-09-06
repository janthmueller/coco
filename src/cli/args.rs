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
    /// Prepare a fresh Codex task in an isolated worktree.
    New(NewArgs),
    /// List tasks in the current repository.
    Ls {
        /// Emit stable, machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Show a task's current state, optionally following it until it pauses.
    Status {
        /// Task name or ID.
        task: String,
        /// Keep updating until the task becomes ready, pauses, or finishes.
        #[arg(long, conflicts_with = "json")]
        follow: bool,
        /// Emit stable, machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Start the first or next turn for a ready task.
    Send {
        /// Task name or ID.
        task: String,
        /// Instruction to send to Codex.
        message: String,
    },
    /// Open the task's existing Codex thread in its managed worktree.
    /// Leaving with /quit or /exit does not cancel active work.
    Jump {
        /// Task name or ID.
        task: String,
    },
    /// Show all tracked and untracked changes from the immutable base.
    Diff { task: String },
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
pub(super) struct NewArgs {
    /// Short task name, also used to derive its branch and worktree.
    pub(super) name: String,
    /// Git revision from which to prepare the task.
    #[arg(long, default_value = "HEAD")]
    pub(super) base: String,
    /// Apply `[profiles.<PROFILE>]` from `$CODEX_HOME/config.toml` to the thread.
    #[arg(long, default_value = "default")]
    pub(super) profile: String,
}

#[derive(Debug, Subcommand)]
pub(super) enum McpCommand {
    /// Serve repository-scoped CoCo tools over stdio.
    Serve {
        #[arg(long)]
        repository: PathBuf,
        /// Advertise the mutating agents.send tool.
        #[arg(long)]
        allow_send: bool,
    },
}
