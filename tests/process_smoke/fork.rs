use super::app_server::*;
use super::support::*;
use super::*;
use tokio::task::JoinHandle;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_daemon_and_cli_create_a_compacted_native_fork() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let paths = TestPaths::new(temporary.path());
    let repository = temporary.path().join("repository");
    let daemon_log = temporary.path().join("cocod-fork.log");
    prepare_repository(&repository)?;
    write_fake_codex(&paths.fake_codex)?;

    let mut daemon = spawn_daemon(&paths, &daemon_log)?;
    wait_for_file(&paths.codex_args, &mut daemon, &daemon_log).await?;
    let arguments = read_arguments(&paths.codex_args)?;
    let endpoint = verify_app_server_arguments(&arguments, &paths.token)?;
    let capability_token = fs::read_to_string(&paths.token)?;
    let address = endpoint
        .strip_prefix("ws://")
        .context("fork App Server endpoint was not a ws:// URL")?
        .parse::<SocketAddr>()
        .context("fork App Server endpoint had an invalid socket address")?;
    let listener = TcpListener::bind(address).await?;
    let observed_authorization = Arc::new(Mutex::new(Vec::new()));
    let observed_requests = Arc::new(Mutex::new(Vec::new()));
    let app_server = tokio::spawn(run_fake_fork_server(
        listener,
        Arc::clone(&observed_authorization),
        Arc::clone(&observed_requests),
    ));

    wait_for_file(&paths.socket, &mut daemon, &daemon_log).await?;
    run_cli(&paths, &repository, &["repo", "add", "."]).await?;
    run_cli(
        &paths,
        &repository,
        &[
            "create",
            FORK_SOURCE_WORKSPACE,
            "--send",
            "Prepare the source context",
        ],
    )
    .await?;
    wait_for_named_workspace_phase(&paths, &repository, FORK_SOURCE_WORKSPACE, "idle").await?;
    run_cli(
        &paths,
        &repository,
        &[
            "create",
            FORK_CHILD_WORKSPACE,
            "--base-workspace",
            FORK_SOURCE_WORKSPACE,
            "--context",
            FORK_SOURCE_WORKSPACE,
            "--compact-context",
            "--model",
            MODEL_OVERRIDE,
            "--send",
            FORK_CHILD_MESSAGE,
        ],
    )
    .await?;

    let child = cli_json(
        &run_cli(
            &paths,
            &repository,
            &["status", FORK_CHILD_WORKSPACE, "--json"],
        )
        .await?,
    )?;
    assert_eq!(child["workspace"]["phase"], "active");
    assert_eq!(child["workspace"]["contextMode"], "fork");
    assert_eq!(
        child["workspace"]["context"]["resolved"]["context"]["compact"],
        true
    );
    assert_eq!(
        child["workspace"]["context"]["resolved"]["context"]["source"]["workspaceName"],
        FORK_SOURCE_WORKSPACE
    );
    assert_eq!(child["workspace"]["parentThreadId"], FORK_SOURCE_THREAD_ID);
    assert_eq!(child["workspace"]["codexThreadId"], FORK_CHILD_THREAD_ID);
    assert_eq!(
        child["workspace"]["profile"]["modelOverride"],
        MODEL_OVERRIDE
    );
    let child_worktree = PathBuf::from(
        child["workspace"]["worktreePath"]
            .as_str()
            .context("forked workspace had no worktree path")?,
    );
    ensure!(child_worktree.is_dir(), "forked worktree does not exist");

    stop_fork_processes(daemon, &daemon_log, app_server).await?;

    verify_fork_requests(
        &observed_requests
            .lock()
            .expect("fork request capture mutex was poisoned"),
        &child_worktree,
    )?;
    let authorizations = observed_authorization
        .lock()
        .expect("fork authorization capture mutex was poisoned");
    assert_eq!(authorizations.len(), 1);
    assert_eq!(authorizations[0], format!("Bearer {capability_token}"));
    Ok(())
}

async fn stop_fork_processes(
    mut daemon: Child,
    daemon_log: &Path,
    app_server: JoinHandle<Result<()>>,
) -> Result<()> {
    interrupt(&daemon).await?;
    let daemon_status = timeout(PROCESS_TIMEOUT, daemon.wait())
        .await
        .context("cocod did not stop after fork smoke test")??;
    ensure!(
        daemon_status.success(),
        "cocod exited with {daemon_status}: {}",
        read_log(daemon_log)
    );
    timeout(PROCESS_TIMEOUT, app_server)
        .await
        .context("fork App Server did not stop")?
        .context("fork App Server panicked")??;
    Ok(())
}
