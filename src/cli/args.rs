use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "coco",
    version,
    about = "Coordinate Codex work in separate worktrees",
    subcommand_precedence_over_arg = true
)]
pub struct Cli {
    /// Use a registered repository other than the current directory.
    #[arg(value_name = "REPOSITORY_PATH")]
    pub(super) scope_path: Option<PathBuf>,
    /// Search or list workspaces across every registered repository.
    #[arg(long, short = 'a')]
    pub(super) all_repos: bool,
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
    /// List the models available to this Codex installation.
    Models {
        /// Emit stable, machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Create a Codex workspace in a separate worktree.
    Create(CreateArgs),
    /// List workspaces in the selected repository, or across all repositories.
    Ls {
        /// List workspaces across every registered repository.
        #[arg(long, short = 'a')]
        all_repos: bool,
        /// Emit stable, machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Show a workspace's current state, optionally following it until it pauses.
    Status {
        /// Workspace name or ID.
        workspace: String,
        /// Resolve the workspace across every registered repository.
        #[arg(long, short = 'a')]
        all_repos: bool,
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
        /// Resolve the workspace across every registered repository.
        #[arg(long, short = 'a')]
        all_repos: bool,
        /// Instruction to send to Codex.
        #[arg(value_parser = non_empty_message)]
        message: String,
    },
    /// Open the workspace's existing Codex thread in its managed worktree.
    /// Leaving with /quit or /exit does not cancel active work.
    Jump {
        /// Workspace name or ID.
        workspace: String,
        /// Resolve the workspace across every registered repository.
        #[arg(long, short = 'a')]
        all_repos: bool,
    },
    /// Answer a pending Codex approval or question.
    Decide {
        /// Decision ID shown by `coco status`.
        decision: String,
    },
    /// Show a bounded tracked patch and untracked paths from the immutable base.
    Diff {
        /// Workspace name or ID.
        workspace: String,
        /// Resolve the workspace across every registered repository.
        #[arg(long, short = 'a')]
        all_repos: bool,
    },
    /// Run CoCo as a local MCP server.
    Mcp {
        #[command(subcommand)]
        command: McpCommand,
    },
}

impl Cli {
    pub(super) fn requests_all_repositories(&self) -> bool {
        self.all_repos || self.command.requests_all_repositories()
    }
}

impl Command {
    fn requests_all_repositories(&self) -> bool {
        match self {
            Self::Ls { all_repos, .. }
            | Self::Status { all_repos, .. }
            | Self::Send { all_repos, .. }
            | Self::Jump { all_repos, .. }
            | Self::Diff { all_repos, .. } => *all_repos,
            Self::Repo { .. }
            | Self::Models { .. }
            | Self::Create(_)
            | Self::Decide { .. }
            | Self::Mcp { .. } => false,
        }
    }
}

#[derive(Debug, Subcommand)]
pub(super) enum RepoCommand {
    /// Register the Git repository containing PATH.
    Add {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// List every repository known to CoCo.
    List {
        /// Emit stable, machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Args)]
pub(super) struct CreateArgs {
    /// Workspace name, such as fix/login, also used for its branch and worktree.
    pub(super) name: String,
    /// Git revision from which to prepare the workspace.
    #[arg(long, default_value = "HEAD", conflicts_with = "fork_from")]
    pub(super) base: String,
    /// Fork committed code and Codex history from an idle workspace in this repository.
    #[arg(long, value_name = "WORKSPACE")]
    pub(super) fork_from: Option<String>,
    /// Compact the new fork before accepting its first message.
    #[arg(long, requires = "fork_from")]
    pub(super) compact: bool,
    /// Layer `$CODEX_HOME/<PROFILE>.config.toml` onto the thread configuration.
    #[arg(long, default_value = "default")]
    pub(super) profile: String,
    /// Override the profile or default model for this workspace's Codex thread.
    #[arg(long, short = 'm', value_name = "MODEL", value_parser = non_empty_model)]
    pub(super) model: Option<String>,
    /// Start the first turn with MESSAGE after creating the workspace.
    #[arg(long, short = 's', value_name = "MESSAGE", value_parser = non_empty_message)]
    pub(super) send: Option<String>,
    /// Open the workspace in the Codex terminal UI after creating it.
    #[arg(long, short = 'j')]
    pub(super) jump: bool,
}

fn non_empty_message(value: &str) -> Result<String, String> {
    if value.trim().is_empty() {
        Err("message must not be empty".to_owned())
    } else {
        Ok(value.to_owned())
    }
}

fn non_empty_model(value: &str) -> Result<String, String> {
    if value.trim().is_empty() {
        Err("model must not be empty".to_owned())
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
