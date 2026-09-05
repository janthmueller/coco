use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "coco-mcp", version, about = "Expose CoCo tools over MCP stdio")]
struct Args {
    /// Fix all tools to this repository.
    #[arg(long)]
    repository: PathBuf,
    /// Advertise the mutating agents.send tool.
    #[arg(long)]
    allow_send: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();

    let args = Args::parse();
    let paths = coco::paths::CocoPaths::from_env()?;
    let repository = if args.repository.is_absolute() {
        args.repository
    } else {
        std::env::current_dir()
            .context("could not determine current directory")?
            .join(args.repository)
    };
    coco::mcp::serve(repository, args.allow_send, paths.socket_path).await
}
