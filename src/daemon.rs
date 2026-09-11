use std::collections::HashMap;
use std::env;
use std::fs::{File, OpenOptions, TryLockError};
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use tokio::sync::watch;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::codex::{CodexClient, CodexClientOptions, CodexEvent, SharedAppServerOptions};
use crate::coordinator::Coordinator;
use crate::domain::runtime::WorkspaceResourcePolicySnapshot;
use crate::git::Git;
use crate::hooks::{HookRegistry, run_dispatcher};
use crate::paths::CocoPaths;
use crate::rpc::{RpcHandler, RpcServer};
use crate::store::Store;

mod execution;
mod handler;
mod worker;

use execution::{
    WorkspaceContainment, WorkspaceExecutionMode, WorkspaceExecutors, initialize_containment,
};
use handler::DaemonHandler;
use worker::CodexWorker;

pub async fn run_from_env() -> Result<()> {
    let paths = CocoPaths::from_env()?;
    let codex_options = CodexClientOptions {
        codex_binary: env::var_os("COCO_CODEX_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("codex")),
        shared_app_server: Some(SharedAppServerOptions {
            endpoint_path: paths.codex_endpoint_path.clone(),
            token_path: paths.codex_token_path.clone(),
        }),
        codex_home: Some(paths.codex_home.clone()),
        ..CodexClientOptions::default()
    };
    run(paths, codex_options).await
}

pub async fn run(paths: CocoPaths, codex_options: CodexClientOptions) -> Result<()> {
    let _daemon_lock = acquire_daemon_lock(&paths.data_dir)?;
    let store = Arc::new(
        Store::open(&paths.database_path)
            .with_context(|| format!("could not open {}", paths.database_path.display()))?,
    );
    let hooks = Arc::new(
        HookRegistry::load(&paths.hooks_path)
            .with_context(|| format!("could not load {}", paths.hooks_path.display()))?,
    );
    reconcile_store_after_restart(&store)?;
    let workspace_execution_mode = WorkspaceExecutionMode::from_env()?;
    let workspace_executor = (
        codex_options.codex_binary.clone(),
        codex_options.codex_home.clone(),
    );
    let workspace_containment =
        prepare_workspace_containment(workspace_execution_mode, &paths.data_dir).await?;
    let workspace_resource_policies = store
        .workspace_resource_policies()
        .context("could not load workspace resource policies")?;
    let (codex, events) = CodexClient::spawn(codex_options)
        .await
        .context("could not start the Codex App Server")?;
    let workspace_executors = build_workspace_executors(
        workspace_execution_mode,
        codex.clone(),
        workspace_executor,
        workspace_containment,
        workspace_resource_policies,
    );
    let runtime_generation = Uuid::new_v4().to_string();
    let coordinator = Arc::new(Coordinator::new(
        Arc::clone(&store),
        Git::default(),
        Arc::new(CodexWorker::new(codex.clone(), workspace_executors.clone())),
        paths.worktrees_dir,
        paths.codex_home,
        Arc::clone(&hooks),
        runtime_generation,
    ));
    let event_task = tokio::spawn(pump_codex_events(Arc::clone(&coordinator), events));
    let recovered_retirements = coordinator.recover_workspace_retirements().await;
    if recovered_retirements > 0 {
        warn!(
            recovered_retirements,
            "recovered interrupted workspace retirement state"
        );
    }

    let handler: Arc<dyn RpcHandler> = Arc::new(DaemonHandler::new(Arc::clone(&coordinator)));
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
    let hook_task = tokio::spawn(run_dispatcher(
        Arc::clone(&store),
        Arc::clone(&hooks),
        shutdown_sender.subscribe(),
    ));
    let mut server_task = tokio::spawn(server.run(shutdown_receiver));
    let server_result = tokio::select! {
        signal = tokio::signal::ctrl_c() => {
            signal.context("could not listen for the shutdown signal")?;
            let _ = shutdown_sender.send(true);
            (&mut server_task).await.context("daemon RPC task panicked")?
        }
        result = &mut server_task => result.context("daemon RPC task panicked")?,
    };
    let _ = shutdown_sender.send(true);

    if let Err(source) = hook_task.await {
        error!(%source, "hook dispatcher panicked");
    }

    if let Some(workspace_executors) = workspace_executors {
        workspace_executors.close().await;
    }
    if let Err(source) = codex.close().await {
        error!(%source, "could not close the Codex App Server cleanly");
    }
    if let Err(source) = event_task.await {
        error!(%source, "Codex event workspace panicked");
    }
    server_result.context("daemon RPC server stopped with an error")
}

fn build_workspace_executors(
    mode: WorkspaceExecutionMode,
    codex: CodexClient,
    options: (PathBuf, Option<PathBuf>),
    containment: Option<WorkspaceContainment>,
    policies: HashMap<String, WorkspaceResourcePolicySnapshot>,
) -> Option<WorkspaceExecutors> {
    match mode {
        WorkspaceExecutionMode::ExecServer => Some(WorkspaceExecutors::new(
            codex,
            options.0,
            options.1,
            containment.expect("exec-server mode must initialize containment"),
            policies,
        )),
        WorkspaceExecutionMode::Shared => None,
    }
}

async fn prepare_workspace_containment(
    mode: WorkspaceExecutionMode,
    data_dir: &Path,
) -> Result<Option<WorkspaceContainment>> {
    match mode {
        WorkspaceExecutionMode::ExecServer => initialize_containment(data_dir)
            .await
            .context("could not initialize workspace execution containment")
            .map(Some),
        WorkspaceExecutionMode::Shared => Ok(None),
    }
}

fn reconcile_store_after_restart(store: &Store) -> Result<()> {
    let recovered_hook_deliveries = store
        .recover_hook_deliveries()
        .context("could not recover interrupted hook deliveries")?;
    if recovered_hook_deliveries > 0 {
        warn!(
            recovered_hook_deliveries,
            "requeued interrupted hook deliveries after daemon restart"
        );
    }
    let reconciled = store
        .reconcile_unfinished()
        .context("could not reconcile unfinished workspaces")?;
    if reconciled.total() > 0 {
        warn!(
            failed_workspace_preparations = reconciled.failed_workspace_preparations,
            uncertain_operations = reconciled.uncertain_operations,
            stale_thread_snapshots = reconciled.stale_thread_snapshots,
            "reconciled unfinished local state after daemon restart"
        );
    }
    Ok(())
}

fn acquire_daemon_lock(data_dir: &Path) -> Result<File> {
    std::fs::create_dir_all(data_dir)
        .with_context(|| format!("could not create {}", data_dir.display()))?;
    let metadata = std::fs::symlink_metadata(data_dir)
        .with_context(|| format!("could not inspect {}", data_dir.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!("refusing unsafe CoCo data directory {}", data_dir.display());
    }
    #[cfg(unix)]
    std::fs::set_permissions(data_dir, std::fs::Permissions::from_mode(0o700))
        .with_context(|| format!("could not secure {}", data_dir.display()))?;

    let lock_path = data_dir.join("cocod.lock");
    if let Ok(metadata) = std::fs::symlink_metadata(&lock_path)
        && (metadata.file_type().is_symlink() || !metadata.is_file())
    {
        bail!("refusing unsafe daemon lock path {}", lock_path.display());
    }
    let mut options = OpenOptions::new();
    options.create(true).read(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    let file = options
        .open(&lock_path)
        .with_context(|| format!("could not open {}", lock_path.display()))?;
    #[cfg(unix)]
    file.set_permissions(std::fs::Permissions::from_mode(0o600))
        .with_context(|| format!("could not secure {}", lock_path.display()))?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => {
            bail!("another cocod process already owns {}", lock_path.display())
        }
        Err(TryLockError::Error(error)) => {
            Err(error).with_context(|| format!("could not lock {}", lock_path.display()))
        }
    }
}

async fn pump_codex_events(
    coordinator: Arc<Coordinator>,
    mut events: tokio::sync::mpsc::Receiver<CodexEvent>,
) {
    while let Some(event) = events.recv().await {
        if let Err(source) = coordinator.record_codex_event(event) {
            error!(%source, "could not handle a Codex event");
        }
    }
    match coordinator.record_codex_disconnected() {
        Ok(0) => {}
        Ok(records) => warn!(
            records,
            "reconciled runtime records after App Server disconnect"
        ),
        Err(source) => error!(%source, "could not reconcile runtime records after disconnect"),
    }
}

#[cfg(test)]
mod tests {
    use super::acquire_daemon_lock;

    #[test]
    fn daemon_lock_excludes_a_second_owner_and_survives_a_stale_file() {
        let directory = tempfile::tempdir().unwrap();
        let first = acquire_daemon_lock(directory.path()).unwrap();
        let error = acquire_daemon_lock(directory.path()).unwrap_err();
        assert!(error.to_string().contains("another cocod process"));

        first.unlock().unwrap();
        drop(first);
        acquire_daemon_lock(directory.path()).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(directory.path().join("cocod.lock"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
    }
}
