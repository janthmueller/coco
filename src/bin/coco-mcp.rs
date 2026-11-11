use std::path::PathBuf;

use clap::Parser;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "coco-mcp", version, about = "Expose CoCo tools over MCP stdio")]
struct Args {
    /// Fix all tools to this repository.
    #[arg(long)]
    repository: PathBuf,
    /// Allow workspace creation and executable worker MCP configuration.
    #[arg(long)]
    allow_create: bool,
    /// Advertise the mutating workspaces.send tool.
    #[arg(long)]
    allow_send: bool,
    /// Expose turns.interrupt; clients can explicitly select bestEffort.
    #[arg(long)]
    allow_interrupt: bool,
    /// Allow checked workspace deletion.
    #[arg(long)]
    allow_delete: bool,
    /// Allow closing workspaces and removing their worktrees.
    #[arg(long)]
    allow_close: bool,
    /// Allow restoring closed workspace worktrees.
    #[arg(long)]
    allow_reopen: bool,
    /// Ask the connected user to review workspace changes and interruptions.
    #[arg(long)]
    confirm_changes: bool,
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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();

    let args = Args::parse();
    coco::run_mcp_from_env(
        args.repository,
        coco::McpPermissions {
            allow_create: args.allow_create,
            allow_send: args.allow_send,
            allow_interrupt: args.allow_interrupt,
            allow_delete: args.allow_delete,
            allow_close: args.allow_close,
            allow_reopen: args.allow_reopen,
            confirm_changes: args.confirm_changes,
        },
        args.allowed_signals,
        args.signal_catalog,
    )
    .await
}
