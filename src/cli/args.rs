use std::path::PathBuf;

use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};

use crate::domain::runtime::MAX_CPU_MILLICORES;

#[derive(Debug, Parser)]
#[command(
    name = "coco",
    version,
    about = "Coordinate persistent Codex workspaces across repositories",
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
    /// Close a workspace and remove its worktree while keeping it available to reopen.
    Close(CloseArgs),
    /// Recreate a closed workspace's worktree and continue using it.
    Reopen(ReopenArgs),
    /// Delete a workspace, its worktree, Codex thread, and CoCo-created branch.
    Delete(DeleteArgs),
    /// List workspaces in the selected repository, or across all repositories.
    #[command(visible_alias = "ls")]
    List {
        /// List workspaces across every registered repository.
        #[arg(long, short = 'a')]
        all_repos: bool,
        /// Emit stable, machine-readable JSON.
        #[arg(long)]
        json: bool,
        /// Show closed workspaces instead of open workspaces.
        #[arg(long)]
        closed: bool,
    },
    /// Show workspace state once or follow it live.
    Status(StatusArgs),
    /// Show cumulative Codex token usage and available cost estimates.
    Usage(UsageArgs),
    /// Inspect or change resource limits for a workspace runtime.
    Limits {
        #[command(subcommand)]
        command: LimitsCommand,
    },
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
        /// Wait for this turn and print its final Codex response.
        #[arg(long)]
        wait: bool,
    },
    /// Open the workspace in the Codex terminal UI, creating fresh context on first action.
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
    /// Read structured updates emitted by agents.
    Signal {
        #[command(subcommand)]
        command: super::signals::SignalCommand,
    },
    /// Manage hooks, guards, and delivery history.
    Hook {
        #[command(subcommand)]
        command: super::hooks::HookCommand,
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
            Self::Status(args) => args.all_repos,
            Self::Usage(args) => args.all_repos,
            Self::Signal { command } => command.all_repos(),
            Self::Repo { .. }
            | Self::Model { .. }
            | Self::Models { .. }
            | Self::Create(_)
            | Self::Limits { .. }
            | Self::Close(_)
            | Self::Reopen(_)
            | Self::Delete(_)
            | Self::Send { .. }
            | Self::Jump { .. }
            | Self::Decide { .. }
            | Self::Diff { .. }
            | Self::Mcp { .. }
            | Self::Hook { .. } => false,
        }
    }

    fn requests_global_search(&self) -> bool {
        match self {
            Self::Status(args) => args.global,
            Self::Usage(args) => args.global,
            Self::Limits { command } => command.global(),
            Self::Signal { command } => command.global(),
            Self::Send { global, .. } | Self::Jump { global, .. } | Self::Diff { global, .. } => {
                *global
            }
            Self::Close(args) => args.global,
            Self::Reopen(args) => args.global,
            Self::Delete(args) => args.global,
            Self::Repo { .. }
            | Self::Model { .. }
            | Self::Models { .. }
            | Self::Create(_)
            | Self::List { .. }
            | Self::Decide { .. }
            | Self::Mcp { .. }
            | Self::Hook { .. } => false,
        }
    }
}

#[derive(Debug, Subcommand)]
pub(super) enum LimitsCommand {
    /// Show the desired limits and whether they are active now.
    Show(LimitsTargetArgs),
    /// Change one or more desired limits.
    Set(LimitsSetArgs),
    /// Remove every CoCo-configured limit from a workspace.
    Reset(LimitsTargetArgs),
}

impl LimitsCommand {
    const fn global(&self) -> bool {
        match self {
            Self::Show(args) | Self::Reset(args) => args.global,
            Self::Set(args) => args.global,
        }
    }
}

#[derive(Debug, Args)]
pub(super) struct LimitsTargetArgs {
    /// Workspace name or ID. Omit it to choose interactively.
    pub(super) workspace: Option<String>,
    /// Resolve or choose the workspace across every registered repository.
    #[arg(long, short = 'g')]
    pub(super) global: bool,
    /// Emit stable, machine-readable JSON.
    #[arg(long)]
    pub(super) json: bool,
}

#[derive(Debug, Args)]
pub(super) struct LimitsSetArgs {
    /// Workspace name or ID. Omit it to choose interactively.
    pub(super) workspace: Option<String>,
    /// Resolve or choose the workspace across every registered repository.
    #[arg(long, short = 'g')]
    pub(super) global: bool,
    /// Start reclaim and throttle allocation above SIZE, such as 2GiB.
    #[arg(long, value_name = "SIZE", value_parser = parse_byte_size)]
    pub(super) memory_high: Option<u64>,
    /// Set the last-resort hard memory ceiling to SIZE, such as 4GiB.
    #[arg(long, value_name = "SIZE", value_parser = parse_byte_size)]
    pub(super) memory_max: Option<u64>,
    /// Limit total CPU bandwidth in logical cores, such as 0.5 or 2.
    #[arg(long, value_name = "CORES", value_parser = parse_cpu_max)]
    pub(super) cpu_max: Option<u32>,
    /// Set relative CPU share under contention (1-10000, default weight 100).
    #[arg(long, value_name = "WEIGHT", value_parser = clap::value_parser!(u16).range(1..=10_000))]
    pub(super) cpu_weight: Option<u16>,
    /// Limit the total number of processes and threads in the runtime.
    #[arg(long, value_name = "COUNT", value_parser = clap::value_parser!(u64).range(1..))]
    pub(super) tasks_max: Option<u64>,
    /// Remove one configured field while retaining the others.
    #[arg(long, value_name = "FIELD", value_enum, action = ArgAction::Append)]
    pub(super) clear: Vec<LimitField>,
    /// Emit stable, machine-readable JSON.
    #[arg(long)]
    pub(super) json: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ValueEnum)]
pub(super) enum LimitField {
    MemoryHigh,
    MemoryMax,
    CpuMax,
    CpuWeight,
    TasksMax,
}

fn parse_byte_size(value: &str) -> Result<u64, String> {
    let trimmed = value.trim();
    let split = trimmed
        .find(|character: char| !character.is_ascii_digit() && character != '.')
        .unwrap_or(trimmed.len());
    let (number, suffix) = trimmed.split_at(split);
    let multiplier = match suffix.to_ascii_lowercase().as_str() {
        "" | "b" => 1_u64,
        "k" | "kb" => 1_000,
        "kib" => 1024,
        "m" | "mb" => 1_000_000,
        "mib" => 1024 * 1024,
        "g" | "gb" => 1_000_000_000,
        "gib" => 1024 * 1024 * 1024,
        "t" | "tb" => 1_000_000_000_000,
        "tib" => 1024_u64.pow(4),
        _ => {
            return Err("size suffix must be B, KB, MB, GB, TB, KiB, MiB, GiB, or TiB".to_owned());
        }
    };
    parse_scaled_decimal(number, multiplier, 3).and_then(|value| {
        u64::try_from(value)
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| "size must be greater than zero and fit in 64 bits".to_owned())
    })
}

fn parse_cpu_max(value: &str) -> Result<u32, String> {
    let millicores = parse_scaled_decimal(value.trim(), 1_000, 3).and_then(|value| {
        u32::try_from(value).map_err(|_| "CPU maximum is too large".to_owned())
    })?;
    if (1..=MAX_CPU_MILLICORES).contains(&millicores) {
        Ok(millicores)
    } else {
        Err(format!(
            "CPU maximum must be between 0.001 and {} cores",
            MAX_CPU_MILLICORES / 1_000
        ))
    }
}

fn parse_scaled_decimal(value: &str, scale: u64, decimals: usize) -> Result<u128, String> {
    let (whole, fractional) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fractional.bytes().all(|byte| byte.is_ascii_digit())
        || fractional.len() > decimals
        || (value.contains('.') && fractional.is_empty())
    {
        return Err(format!(
            "expected a positive decimal with at most {decimals} fractional digits"
        ));
    }
    let denominator = 10_u128.pow(u32::try_from(fractional.len()).unwrap_or(u32::MAX));
    let whole = whole
        .parse::<u128>()
        .map_err(|_| "numeric value is too large".to_owned())?;
    let fractional = if fractional.is_empty() {
        0
    } else {
        fractional
            .parse::<u128>()
            .map_err(|_| "numeric value is too large".to_owned())?
    };
    let numerator = whole
        .checked_mul(denominator)
        .and_then(|whole| whole.checked_add(fractional))
        .and_then(|value| value.checked_mul(u128::from(scale)))
        .ok_or_else(|| "numeric value is too large".to_owned())?;
    Ok(numerator / denominator)
}

#[derive(Debug, Args)]
pub(super) struct CloseArgs {
    /// Workspace name or ID. Omit it to choose interactively.
    pub(super) workspace: Option<String>,
    /// Resolve or choose the workspace across every registered repository.
    #[arg(long, short = 'g')]
    pub(super) global: bool,
    /// Archive the native Codex thread after closing the worktree.
    #[arg(long, short = 't')]
    pub(super) archive_thread: bool,
    /// Permanently discard tracked, untracked, and ignored worktree changes.
    #[arg(long)]
    pub(super) discard_changes: bool,
    /// Show the close plan without changing anything.
    #[arg(long, short = 'n')]
    pub(super) dry_run: bool,
    /// Skip confirmation; does not authorize discarding local changes.
    #[arg(long, short = 'y')]
    pub(super) yes: bool,
}

#[derive(Debug, Args)]
pub(super) struct ReopenArgs {
    /// Closed workspace name or ID. Omit it to choose interactively.
    pub(super) workspace: Option<String>,
    /// Resolve or choose the workspace across every registered repository.
    #[arg(long, short = 'g')]
    pub(super) global: bool,
}

#[derive(Debug, Args)]
pub(super) struct DeleteArgs {
    /// Open or closed workspace name or ID. Omit it to choose interactively.
    pub(super) workspace: Option<String>,
    /// Resolve or choose the workspace across every registered repository.
    #[arg(long, short = 'g')]
    pub(super) global: bool,
    /// Keep the Codex conversation after deleting the workspace.
    #[arg(long)]
    pub(super) keep_thread: bool,
    /// Keep the CoCo-created branch. Adopted branches are always kept.
    #[arg(long)]
    pub(super) keep_branch: bool,
    /// Permanently discard tracked, untracked, and ignored worktree changes.
    #[arg(long)]
    pub(super) discard_changes: bool,
    /// Allow commits to lose their last branch, tag, or remote-tracking reference.
    #[arg(long)]
    pub(super) discard_unretained_commits: bool,
    /// Show the checked deletion plan without changing anything.
    #[arg(long, short = 'n')]
    pub(super) dry_run: bool,
    /// Skip confirmation; does not authorize discarding local changes or unretained commits.
    #[arg(long, short = 'y')]
    pub(super) yes: bool,
}

#[derive(Debug, Args)]
pub(super) struct StatusArgs {
    /// Workspace name or ID. Omit it to show the selected repository.
    pub(super) workspace: Option<String>,
    /// Show workspaces across every registered repository.
    #[arg(long, short = 'a', conflicts_with = "workspace")]
    pub(super) all_repos: bool,
    /// Resolve one named workspace across every registered repository.
    #[arg(long, short = 'g')]
    pub(super) global: bool,
    /// Follow state changes until interrupted.
    #[arg(long, short = 'f', conflicts_with = "json")]
    pub(super) follow: bool,
    /// Include current memory, process, and CPU use.
    #[arg(long, short = 'r')]
    pub(super) resources: bool,
    /// Emit stable, machine-readable JSON.
    #[arg(long)]
    pub(super) json: bool,
}

#[derive(Debug, Args)]
pub(super) struct UsageArgs {
    /// Workspace name or ID. Omit it to show the selected repository.
    pub(super) workspace: Option<String>,
    /// Show workspaces across every registered repository.
    #[arg(long, short = 'a', conflicts_with = "workspace")]
    pub(super) all_repos: bool,
    /// Resolve one named workspace across every registered repository.
    #[arg(long, short = 'g')]
    pub(super) global: bool,
    /// Follow usage changes until interrupted.
    #[arg(long, short = 'f', conflicts_with = "json")]
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
    /// Fork Codex history from a workspace reference or native thread ID.
    #[arg(
        long,
        short = 'c',
        value_name = "REFERENCE",
        value_parser = non_empty_context_reference,
        conflicts_with = "fork_from"
    )]
    pub(super) context: Option<String>,
    /// Compatibility alias that selects both base and context from one workspace.
    #[arg(
        long,
        value_name = "WORKSPACE",
        hide = true,
        conflicts_with_all = ["base", "base_workspace", "context"]
    )]
    pub(super) fork_from: Option<String>,
    /// Compact the new fork before accepting its first message.
    #[arg(long, short = 'C')]
    pub(super) compact_context: bool,
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

fn non_empty_context_reference(value: &str) -> Result<String, String> {
    if value.trim().is_empty() {
        Err("context reference must not be empty".to_owned())
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
        /// Grant NAME@VERSION; a bare NAME grants version 1. Repeat for each grant.
        #[arg(
            long = "allow-emit",
            value_name = "SIGNAL",
            requires = "signal_catalog"
        )]
        allowed_signals: Vec<String>,
        /// Load NAME@VERSION.json schemas from this directory at startup.
        #[arg(long, value_name = "DIR")]
        signal_catalog: Option<PathBuf>,
    },
}
