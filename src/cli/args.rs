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
    /// Show a collection across every registered repository.
    #[arg(long, short = 'a')]
    pub(super) all_repos: bool,
    /// Resolve or choose one workspace across every registered repository.
    #[arg(long, short = 'g')]
    pub(super) global: bool,
    /// Never open interactive prompts or selectors.
    #[arg(long, global = true)]
    pub(super) no_input: bool,
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
    /// Inspect models available to this Codex installation.
    Model {
        #[command(subcommand)]
        command: ModelCommand,
    },
    /// Compatibility spelling for `coco model list`.
    #[command(hide = true)]
    Models {
        /// Emit stable, machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Create a Codex workspace in a separate worktree.
    Create(CreateArgs),
    /// List workspaces in the selected repository, or across all repositories.
    #[command(visible_alias = "ls")]
    List {
        /// List workspaces across every registered repository.
        #[arg(long, short = 'a')]
        all_repos: bool,
        /// Emit stable, machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Show detailed state for one workspace.
    Status(StatusArgs),
    /// Start the first or next turn for a ready workspace.
    Send {
        /// Workspace name or ID. Omit it to choose interactively.
        workspace: Option<String>,
        /// Resolve or choose the workspace across every registered repository.
        #[arg(long, short = 'g')]
        global: bool,
        /// Instruction to send to Codex. Omit it to enter one interactively.
        #[arg(value_parser = non_empty_message)]
        message: Option<String>,
        /// Reuse a previous send's operation ID after an interrupted CLI response.
        #[arg(long, value_name = "ID", value_parser = non_empty_operation_id)]
        operation_id: Option<String>,
    },
    /// Open the workspace's existing Codex thread in its managed worktree.
    /// Leaving with /quit or /exit does not cancel active work.
    Jump {
        /// Workspace name or ID. Omit it to choose interactively.
        workspace: Option<String>,
        /// Resolve or choose the workspace across every registered repository.
        #[arg(long, short = 'g')]
        global: bool,
    },
    /// Answer a pending Codex approval or question.
    Decide {
        /// Decision ID shown by `coco status`.
        decision: String,
        /// Submit one approval option non-interactively by its displayed number.
        #[arg(long, value_name = "NUMBER", value_parser = clap::value_parser!(u32).range(1..))]
        choice: Option<u32>,
    },
    /// Show a bounded tracked patch and untracked paths from the immutable base.
    Diff {
        /// Workspace name or ID. Omit it to choose interactively.
        workspace: Option<String>,
        /// Resolve or choose the workspace across every registered repository.
        #[arg(long, short = 'g')]
        global: bool,
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

    pub(super) fn requests_global_search(&self) -> bool {
        self.global || self.command.requests_global_search()
    }
}

impl Command {
    fn requests_all_repositories(&self) -> bool {
        match self {
            Self::List { all_repos, .. } => *all_repos,
            Self::Status(_)
            | Self::Repo { .. }
            | Self::Model { .. }
            | Self::Models { .. }
            | Self::Create(_)
            | Self::Send { .. }
            | Self::Jump { .. }
            | Self::Decide { .. }
            | Self::Diff { .. }
            | Self::Mcp { .. } => false,
        }
    }

    fn requests_global_search(&self) -> bool {
        match self {
            Self::Status(args) => args.global,
            Self::Send { global, .. } | Self::Jump { global, .. } | Self::Diff { global, .. } => {
                *global
            }
            Self::Repo { .. }
            | Self::Model { .. }
            | Self::Models { .. }
            | Self::Create(_)
            | Self::List { .. }
            | Self::Decide { .. }
            | Self::Mcp { .. } => false,
        }
    }
}

#[derive(Debug, Args)]
pub(super) struct StatusArgs {
    /// Workspace name or ID. Omit it to choose interactively.
    pub(super) workspace: Option<String>,
    /// Resolve or choose the workspace across every registered repository.
    #[arg(long, short = 'g')]
    pub(super) global: bool,
    /// Keep updating until the workspace becomes ready, pauses, or finishes.
    #[arg(long, conflicts_with = "json")]
    pub(super) follow: bool,
    /// Emit stable, machine-readable JSON.
    #[arg(long)]
    pub(super) json: bool,
}

#[derive(Debug, Subcommand)]
pub(super) enum RepoCommand {
    /// Register the Git repository containing PATH.
    Add {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// List every repository known to CoCo.
    #[command(visible_alias = "ls")]
    List {
        /// Emit stable, machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
pub(super) enum ModelCommand {
    /// List the models reported by Codex.
    #[command(visible_alias = "ls")]
    List {
        /// Emit stable, machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Args)]
pub(super) struct CreateArgs {
    /// Workspace name, such as fix/login. Omit it to enter one interactively.
    pub(super) name: Option<String>,
    /// Git revision from which to prepare a new branch or detached worktree.
    #[arg(long, value_name = "REVISION", conflicts_with_all = ["base_workspace", "checkout"])]
    pub(super) base: Option<String>,
    /// Use the committed HEAD of another workspace as the Git base.
    #[arg(long, value_name = "WORKSPACE", conflicts_with_all = ["base", "checkout"])]
    pub(super) base_workspace: Option<String>,
    /// Fork Codex history from a CoCo workspace without selecting its code.
    #[arg(long, value_name = "WORKSPACE", conflicts_with_all = ["context_thread", "fork_from"])]
    pub(super) context_workspace: Option<String>,
    /// Fork Codex history from an existing native Codex thread ID.
    #[arg(long, value_name = "THREAD_ID", conflicts_with_all = ["context_workspace", "fork_from"])]
    pub(super) context_thread: Option<String>,
    /// Compatibility alias that selects both base and context from one workspace.
    #[arg(
        long,
        value_name = "WORKSPACE",
        hide = true,
        conflicts_with_all = ["base", "base_workspace", "context_workspace", "context_thread"]
    )]
    pub(super) fork_from: Option<String>,
    /// Compact the new fork before accepting its first message.
    #[arg(long)]
    pub(super) compact: bool,
    /// Create a new branch with this name instead of `coco/<workspace>`.
    #[arg(long, value_name = "BRANCH", conflicts_with_all = ["checkout", "detached"])]
    pub(super) branch: Option<String>,
    /// Check out an existing local branch instead of creating one.
    #[arg(
        long,
        value_name = "BRANCH",
        conflicts_with_all = [
            "branch",
            "detached",
            "base",
            "base_workspace",
            "fork_from"
        ]
    )]
    pub(super) checkout: Option<String>,
    /// Create the worktree at a detached HEAD without allocating a branch.
    #[arg(long, short = 'D', conflicts_with_all = ["branch", "checkout"])]
    pub(super) detached: bool,
    /// Carry tracked staged and unstaged changes from the invoking checkout.
    #[arg(long)]
    pub(super) carry_changes: bool,
    /// Also carry ordinary non-ignored untracked files.
    #[arg(long)]
    pub(super) carry_untracked: bool,
    /// Shortcut for `--carry-changes --carry-untracked`.
    #[arg(long, short = 'd')]
    pub(super) dirty: bool,
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

fn non_empty_operation_id(value: &str) -> Result<String, String> {
    if value.trim().is_empty() {
        Err("operation ID must not be empty".to_owned())
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
