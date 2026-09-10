use super::app_server::{CaptureAuthorization, send_result};
use super::support::*;
use super::*;

const RETIREMENT_WORKSPACE: &str = "cleanup/process";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_closes_reopens_and_deletes_a_prepared_workspace_safely() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let paths = TestPaths::new(temporary.path());
    let repository = temporary.path().join("repository");
    let daemon_log = temporary.path().join("cocod.log");
    prepare_repository(&repository)?;
    prepare_codex_profile(&paths)?;
    write_fake_codex(&paths.fake_codex)?;
    super::hooks::prepare(&paths)?;
    super::hooks::verify_offline_validation(&paths, &repository).await?;

    let mut daemon = spawn_daemon(&paths, &daemon_log)?;
    wait_for_file(&paths.codex_args, &mut daemon, &daemon_log).await?;
    let endpoint = verify_app_server_arguments(&read_arguments(&paths.codex_args)?, &paths.token)?;
    let address = endpoint
        .strip_prefix("ws://")
        .context("retirement endpoint was not ws://")?
        .parse::<SocketAddr>()?;
    let listener = TcpListener::bind(address).await?;
    let authorization = Arc::new(Mutex::new(Vec::new()));
    let server = tokio::spawn(run_retirement_server(listener, Arc::clone(&authorization)));
    wait_for_file(&paths.socket, &mut daemon, &daemon_log).await?;

    run_cli(&paths, &repository, &["repo", "add", "."]).await?;
    run_cli(&paths, &repository, &["create", RETIREMENT_WORKSPACE]).await?;
    let initial = named_workspace_status(&paths, &repository, RETIREMENT_WORKSPACE).await?;
    let worktree = PathBuf::from(
        initial
            .pointer("/workspace/worktreePath")
            .and_then(Value::as_str)
            .context("workspace had no worktree path")?,
    );
    let branch = initial
        .pointer("/workspace/branchName")
        .and_then(Value::as_str)
        .context("workspace had no branch")?
        .to_owned();
    ensure!(worktree.is_dir(), "create did not make the worktree");

    let preview = run_cli(
        &paths,
        &repository,
        &["close", RETIREMENT_WORKSPACE, "--dry-run"],
    )
    .await?;
    ensure!(
        String::from_utf8_lossy(&preview.stdout).contains("Close cleanup/process plan"),
        "close dry-run did not render its plan"
    );
    run_cli(&paths, &repository, &["close", RETIREMENT_WORKSPACE]).await?;
    ensure!(!worktree.exists(), "close retained the managed worktree");
    assert_closed_workspace_listing(&paths, &repository).await?;

    run_cli(&paths, &repository, &["reopen", RETIREMENT_WORKSPACE]).await?;
    ensure!(worktree.is_dir(), "reopen did not restore the worktree");
    fs::write(worktree.join("local.txt"), "local state\n")?;
    let refused = capture_cli(&paths, &repository, &["close", RETIREMENT_WORKSPACE], None).await?;
    ensure!(
        !refused.status.success()
            && String::from_utf8_lossy(&refused.stderr)
                .contains("confirmation is required without interactive input"),
        "non-interactive close did not refuse implicit local-state loss"
    );
    ensure!(worktree.join("local.txt").is_file());
    run_cli(
        &paths,
        &repository,
        &["close", RETIREMENT_WORKSPACE, "--discard-changes", "--yes"],
    )
    .await?;
    ensure!(!worktree.exists(), "confirmed close retained the worktree");

    run_cli(
        &paths,
        &repository,
        &["delete", RETIREMENT_WORKSPACE, "--delete-branch", "--yes"],
    )
    .await?;
    let after = cli_json(&run_cli(&paths, &repository, &["list", "--closed", "--json"]).await?)?;
    assert_eq!(after["workspaces"].as_array().map(Vec::len), Some(0));
    assert_branch_missing(&repository, &branch).await?;
    verify_retirement_hooks(&paths, &repository).await?;

    interrupt(&daemon).await?;
    let daemon_status = timeout(PROCESS_TIMEOUT, daemon.wait()).await??;
    ensure!(daemon_status.success(), "cocod did not exit cleanly");
    timeout(PROCESS_TIMEOUT, server).await???;
    ensure!(
        !authorization
            .lock()
            .expect("authorization mutex was poisoned")
            .is_empty(),
        "daemon did not authenticate to the App Server"
    );
    Ok(())
}

async fn verify_retirement_hooks(paths: &TestPaths, repository: &Path) -> Result<()> {
    let hook_events = super::hooks::wait_for_kinds(
        paths,
        &[
            ("workspace.created", 1),
            ("workspace.closed", 2),
            ("workspace.reopened", 1),
            ("workspace.deleted", 1),
        ],
    )
    .await?;
    ensure!(
        hook_events
            .iter()
            .any(|event| event["kind"] == "workspace.deleted"
                && event["workspace"]["name"] == RETIREMENT_WORKSPACE),
        "delete hook did not retain the deleted workspace identity"
    );
    super::hooks::verify_history(paths, repository, 5).await?;
    let guards = super::hooks::captured_guards(paths)?;
    let close_count = guards
        .iter()
        .filter(|request| request["action"] == "workspace.close")
        .count();
    let delete_count = guards
        .iter()
        .filter(|request| request["action"] == "workspace.delete")
        .count();
    ensure!(
        close_count == 2 && delete_count == 1,
        "guards did not run exactly once for applied operations: {guards:?}"
    );
    ensure!(
        guards.iter().all(|request| request["schemaVersion"] == 1
            && request["workspace"]["name"] == RETIREMENT_WORKSPACE
            && request["repository"]["path"].is_string()
            && request["data"]["plan"]["workspaceId"].is_string()),
        "guard request envelope was incomplete: {guards:?}"
    );
    Ok(())
}

async fn assert_closed_workspace_listing(paths: &TestPaths, repository: &Path) -> Result<()> {
    let open = cli_json(&run_cli(paths, repository, &["list", "--json"]).await?)?;
    assert_eq!(open["schemaVersion"], 7);
    assert_eq!(open["workspaces"].as_array().map(Vec::len), Some(0));
    let closed = cli_json(&run_cli(paths, repository, &["list", "--closed", "--json"]).await?)?;
    assert_eq!(closed["workspaces"].as_array().map(Vec::len), Some(1));
    assert_eq!(closed["workspaces"][0]["phase"], "closed");
    Ok(())
}

async fn assert_branch_missing(repository: &Path, branch: &str) -> Result<()> {
    let status = Command::new("git")
        .args([
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .current_dir(repository)
        .status()
        .await?;
    ensure!(!status.success(), "delete retained the requested branch");
    Ok(())
}

async fn run_retirement_server(
    listener: TcpListener,
    authorization: Arc<Mutex<Vec<String>>>,
) -> Result<()> {
    let (stream, peer) = listener.accept().await?;
    ensure!(
        peer.ip().is_loopback(),
        "retirement connection was not local"
    );
    let mut websocket = accept_hdr_async(stream, CaptureAuthorization(authorization)).await?;
    while let Some(message) = websocket.next().await {
        let frame = match message? {
            Message::Text(text) => serde_json::from_str::<Value>(&text)?,
            Message::Binary(bytes) => serde_json::from_slice::<Value>(&bytes)?,
            Message::Close(_) => return Ok(()),
            Message::Ping(payload) => {
                websocket.send(Message::Pong(payload)).await?;
                continue;
            }
            Message::Pong(_) | Message::Frame(_) => continue,
        };
        match frame.get("method").and_then(Value::as_str) {
            Some("initialize") => send_result(&mut websocket, &frame, json!({})).await?,
            Some("initialized") => {}
            Some(other) => bail!("unexpected retirement App Server method {other:?}"),
            None => bail!("retirement App Server frame had no method: {frame}"),
        }
    }
    Ok(())
}
