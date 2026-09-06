#[tokio::main]
async fn main() -> anyhow::Result<()> {
    coco::run_cli_from_env().await
}
