use std::path::PathBuf;

use clap::Parser;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "coco-mcp", version, about = "Expose CoCo tools over MCP stdio")]
struct Args {
    /// Fix all tools to this repository.
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
        args.allow_send,
        args.allowed_signals,
        args.signal_catalog,
    )
    .await
}
