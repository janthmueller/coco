#[tokio::main]
async fn main() -> anyhow::Result<()> {
    coco::cli::run_from_env().await
}
