use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::watch;
use tracing::{error, info, warn};

use crate::codex::{CodexClient, CodexClientOptions, CodexEvent};
use crate::coordinator::{CodexWorker, Coordinator};
use crate::git::Git;
use crate::paths::CocoPaths;
use crate::rpc::{RpcHandler, RpcServer};
use crate::store::Store;

pub async fn run_from_env() -> Result<()> {
    let paths = CocoPaths::from_env()?;
    let codex_options = CodexClientOptions {
        app_server_socket: Some(paths.codex_socket_path.clone()),
        codex_home: Some(paths.codex_home.clone()),
        ..CodexClientOptions::default()
    };
    run(paths, codex_options).await
}

pub async fn run(paths: CocoPaths, codex_options: CodexClientOptions) -> Result<()> {
    let store = Arc::new(
        Store::open(&paths.database_path)
            .with_context(|| format!("could not open {}", paths.database_path.display()))?,
    );
    let reconciled = store
        .reconcile_unfinished()
        .context("could not reconcile unfinished tasks")?;
    if !reconciled.is_empty() {
        warn!(
            tasks = reconciled.len(),
            "marked unfinished tasks interrupted after daemon restart"
        );
    }

    let (codex, events) = CodexClient::spawn(codex_options)
        .await
        .context("could not start the Codex App Server")?;
    let coordinator = Arc::new(Coordinator::new(
        store,
        Git::default(),
        Arc::new(CodexWorker::new(codex.clone())),
        paths.worktrees_dir,
        paths.codex_home,
    ));
    let event_task = tokio::spawn(pump_codex_events(Arc::clone(&coordinator), events));

    let handler: Arc<dyn RpcHandler> = coordinator;
    let server = match RpcServer::bind(&paths.socket_path, handler).await {
        Ok(server) => server,
        Err(source) => {
            let _ = codex.close().await;
            let _ = event_task.await;
            return Err(source)
                .with_context(|| format!("could not bind {}", paths.socket_path.display()));
        }
    };
    info!(socket = %paths.socket_path.display(), "cocod is ready");

    let (shutdown_sender, shutdown_receiver) = watch::channel(false);
    let mut server_task = tokio::spawn(server.run(shutdown_receiver));
    let server_result = tokio::select! {
        signal = tokio::signal::ctrl_c() => {
            signal.context("could not listen for the shutdown signal")?;
            let _ = shutdown_sender.send(true);
            (&mut server_task).await.context("daemon RPC task panicked")?
        }
        result = &mut server_task => result.context("daemon RPC task panicked")?,
    };

    if let Err(source) = codex.close().await {
        error!(%source, "could not close the Codex App Server cleanly");
    }
    if let Err(source) = event_task.await {
        error!(%source, "Codex event task panicked");
    }
    server_result.context("daemon RPC server stopped with an error")
}

async fn pump_codex_events(
    coordinator: Arc<Coordinator>,
    mut events: tokio::sync::mpsc::Receiver<CodexEvent>,
) {
    while let Some(event) = events.recv().await {
        if let Err(source) = coordinator.record_codex_event(event) {
            error!(%source, "could not persist a Codex event");
        }
    }
}
