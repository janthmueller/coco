use clap::Parser;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "cocod", version, about = "Run the CoCo orchestration daemon")]
struct Args {}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    Args::parse();
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
    coco::run_daemon_from_env().await
}
