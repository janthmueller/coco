use super::*;

pub(super) struct PausedProvider(tokio::task::JoinHandle<Result<()>>);

impl PausedProvider {
    pub(super) async fn start(paths: &TestPaths) -> Result<Self> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        fs::write(
            paths.codex_home.join("config.toml"),
            format!(
                "model_provider = \"turn-proof\"\n[model_providers.turn-proof]\n\
            name = \"Paused loopback fixture\"\nbase_url = \"http://{address}/v1\"\n\
            wire_api = \"responses\"\nrequires_openai_auth = false\nsupports_websockets = false\n"
            ),
        )?;
        let task = tokio::spawn(async move {
            let mut held = Vec::new();
            loop {
                let (stream, _) = listener.accept().await?;
                ensure!(held.len() < 8, "unexpected fixture retry storm");
                // Keep requests local and unanswered until Codex interruption;
                // no model, authentication, or upstream endpoint is involved.
                held.push(stream);
            }
        });
        Ok(Self(task))
    }
}

impl Drop for PausedProvider {
    fn drop(&mut self) {
        self.0.abort();
    }
}
