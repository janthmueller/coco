use super::support::*;
use super::*;

#[path = "temporary/native.rs"]
mod native;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn temporary_workspaces_cleanup_only_after_successful_jump_and_survive_restart() -> Result<()>
{
    let root = tempfile::tempdir()?;
    let paths = TestPaths::new(root.path());
    let repository = root.path().join("repo");
    prepare_repository(&repository)?;
    prepare_codex_profile(&paths)?;
    write_fake_codex(&paths.fake_codex)?;
    let log = root.path().join("daemon.log");
    let (mut daemon, server) = start(&paths, &log).await?;
    let created = run_cli(
        &paths,
        &repository,
        &[
            "create",
            "scratch/later",
            "--temporary",
            "--profile",
            PROFILE_NAME,
        ],
    )
    .await?;
    ensure!(
        String::from_utf8_lossy(&created.stdout)
            .lines()
            .any(
                |line| line.contains("Created scratch/later") && line.contains("cleanup after TUI")
            )
    );
    assert_armed(&paths, &repository, "scratch/later").await?;
    stop(&mut daemon, server).await?;

    let (mut daemon, server) = start(&paths, &log).await?;
    assert_armed(&paths, &repository, "scratch/later").await?;
    let output = tui(&paths, &repository, &["jump", "scratch/later"], 0, false).await?;
    ensure!(
        output.status.success(),
        "successful jump failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    ensure!(String::from_utf8_lossy(&output.stderr).contains("Removed scratch/later"));
    assert_empty(&paths, &repository).await?;

    for (name, missing_binary) in [("scratch/failed", false), ("scratch/missing", true)] {
        let output = tui(
            &paths,
            &repository,
            &[
                "create",
                name,
                "--temporary",
                "--profile",
                PROFILE_NAME,
                "--jump",
            ],
            1,
            missing_binary,
        )
        .await?;
        ensure!(
            !output.status.success(),
            "failed TUI unexpectedly succeeded"
        );
        assert_armed(&paths, &repository, name).await?;
        let output = tui(&paths, &repository, &["jump", name], 0, false).await?;
        ensure!(
            output.status.success(),
            "later normal jump failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_empty(&paths, &repository).await?;
    }
    let output = tui(
        &paths,
        &repository,
        &[
            "create",
            "scratch/now",
            "--temporary",
            "--profile",
            PROFILE_NAME,
            "--jump",
        ],
        0,
        false,
    )
    .await?;
    ensure!(
        output.status.success(),
        "create -j failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_empty(&paths, &repository).await?;
    stop(&mut daemon, server).await?;
    Ok(())
}

async fn assert_armed(paths: &TestPaths, repository: &Path, name: &str) -> Result<()> {
    let output = run_cli(paths, repository, &["status", name, "--json"]).await?;
    let status = cli_json(&output)?;
    ensure!(
        status["workspace"]["temporary"]["keepThread"] == false,
        "temporary policy was lost or cleanup was triggered by observation: {status}"
    );
    ensure!(
        Path::new(
            status["workspace"]["worktreePath"]
                .as_str()
                .context("missing worktree")?
        )
        .is_dir()
    );
    Ok(())
}

async fn assert_empty(paths: &TestPaths, repository: &Path) -> Result<()> {
    let result = cli_json(&run_cli(paths, repository, &["list", "--json"]).await?)?;
    ensure!(
        result["workspaces"].as_array().is_some_and(Vec::is_empty),
        "cleanup did not remove workspace: {result}"
    );
    Ok(())
}

async fn tui(
    paths: &TestPaths,
    repository: &Path,
    args: &[&str],
    code: u8,
    missing: bool,
) -> Result<Output> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_coco"));
    paths.apply(&mut command);
    command
        .args(args)
        .current_dir(repository)
        .env("COCO_TEST_EMPTY_TUI_EXIT", code.to_string())
        .env("COCO_TEST_FAKE_TUI_BINARY", std::env::current_exe()?)
        .env("COCO_TEST_FRESH_TUI_MODE", "empty")
        .kill_on_drop(true);
    if missing {
        command.env("COCO_CODEX_BINARY", paths.home.join("missing-codex"));
    }
    timeout(PROCESS_TIMEOUT, command.output())
        .await
        .with_context(|| format!("temporary CLI timed out: {}", args.join(" ")))?
        .context("temporary CLI failed")
}

async fn start(
    paths: &TestPaths,
    log: &Path,
) -> Result<(Child, tokio::task::JoinHandle<Result<()>>)> {
    if paths.codex_args.exists() {
        fs::remove_file(&paths.codex_args)?;
    }
    let mut daemon = spawn_daemon(paths, log)?;
    wait_for_file(&paths.codex_args, &mut daemon, log).await?;
    let endpoint = verify_app_server_arguments(&read_arguments(&paths.codex_args)?, &paths.token)?;
    let address: SocketAddr = endpoint
        .strip_prefix("ws://")
        .context("invalid endpoint")?
        .parse()?;
    let listener = TcpListener::bind(address).await?;
    let server = tokio::spawn(empty_server(listener, paths.codex_home.join("sessions")));
    wait_for_file(&paths.socket, &mut daemon, log).await?;
    Ok((daemon, server))
}

async fn stop(daemon: &mut Child, server: tokio::task::JoinHandle<Result<()>>) -> Result<()> {
    interrupt(daemon).await?;
    ensure!(timeout(PROCESS_TIMEOUT, daemon.wait()).await??.success());
    timeout(PROCESS_TIMEOUT, server).await???;
    Ok(())
}

async fn empty_server(listener: TcpListener, sessions: PathBuf) -> Result<()> {
    let threads = Arc::new(native::EmptyThreads::new(sessions));
    let (socket, _) = listener.accept().await?;
    let control = empty_connection(socket, Arc::clone(&threads));
    tokio::pin!(control);
    let mut relays = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            result = &mut control => return result,
            accepted = listener.accept() => {
                let (socket, _) = accepted?;
                relays.spawn(empty_connection(socket, Arc::clone(&threads)));
            }
            finished = relays.join_next(), if !relays.is_empty() => {
                finished.context("missing relay fixture")???;
            }
        }
    }
}

async fn empty_connection(
    socket: tokio::net::TcpStream,
    threads: Arc<native::EmptyThreads>,
) -> Result<()> {
    let mut websocket = tokio_tungstenite::accept_async(socket).await?;
    while let Some(frame) = websocket.next().await {
        let frame = match frame {
            Ok(frame) => frame,
            Err(tokio_tungstenite::tungstenite::Error::Protocol(
                tokio_tungstenite::tungstenite::error::ProtocolError::ResetWithoutClosingHandshake,
            )) => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        match frame {
            Message::Text(text) => {
                let request: Value = serde_json::from_str(&text)?;
                if let Some(response) = threads.reply(&request)? {
                    websocket
                        .send(Message::Text(response.to_string().into()))
                        .await?;
                }
            }
            Message::Close(_) => return Ok(()),
            Message::Ping(bytes) => websocket.send(Message::Pong(bytes)).await?,
            _ => {}
        }
    }
    Ok(())
}
