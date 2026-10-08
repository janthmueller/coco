#[tokio::main]
async fn main() -> anyhow::Result<std::process::ExitCode> {
    coco::run_cli_from_env().await
}
