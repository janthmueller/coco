use super::*;

#[path = "context_capture/provider.rs"]
mod provider;

#[path = "context_capture/legacy.rs"]
mod legacy;

const LATER_MARKER: &str = "coco-later-source-turn";
const NEWEST_MARKER: &str = "coco-newest-completed-turn";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires COCO_RUN_REAL_CODEX_COMPAT=1, selected Codex, and tmux; no external model calls"]
async fn installed_codex_captures_active_source_and_restores_independent_child() -> Result<()> {
    require_explicit_opt_in()?;
    let binary = env::var_os(CODEX_BINARY_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| "codex".into());
    let temporary = tempfile::tempdir()?;
    let paths = TestPaths::new(temporary.path());
    for directory in [&paths.home, &paths.codex_home, &paths.data_dir] {
        fs::create_dir_all(directory)?;
    }
    tui::install_dummy_auth(&paths)?;
    verify_codex_version(&binary, &paths).await?;
    let repository = temporary.path().join("repository");
    prepare_repository(&repository)?;
    let provider = provider::LocalProvider::start(&paths).await?;
    let log = paths.data_dir.join("context-capture.log");
    let mut daemon = spawn_daemon(&paths, &binary, &log)?;
    wait_for_file(&paths.socket, &mut daemon, &log).await?;
    let source = prepare_source(&paths, &binary, &repository).await?;
    let child = capture_while_working(&paths, &binary, &repository, &source).await?;
    stop_daemon(&mut daemon, &log).await?;
    let mut daemon = spawn_daemon(&paths, &binary, &log)?;
    wait_for_file(&paths.socket, &mut daemon, &log).await?;
    verify_after_restart(&paths, &binary, &repository, &source, &child).await?;
    ensure!(
        provider.request_count() == 1,
        "capture/jump unexpectedly invoked the model"
    );
    stop_daemon(&mut daemon, &log).await?;
    verify_runtime_cleanup(&paths)
}

async fn prepare_source(paths: &TestPaths, binary: &Path, repository: &Path) -> Result<String> {
    run_cli(paths, binary, repository, &["create", WORKSPACE_NAME]).await?;
    assert_prepared_workspace(&workspace_status(paths, binary, repository).await?)?;
    let models = run_cli(paths, binary, repository, &["model", "list", "--json"]).await?;
    let model = select_default_model(&serde_json::from_slice(&models.stdout)?)?;
    materialize_workspace_through_remote_action(
        paths,
        binary,
        repository,
        &model.model,
        &model.reasoning_effort,
    )
    .await?;
    let status = workspace_status(paths, binary, repository).await?;
    let source = native_id(&status)?.to_owned();
    let mut remote = RemoteAppServer::connect_with_capabilities(paths, true).await?;
    remote
        .request(
            "thread/resume",
            json!({"threadId": source, "excludeTurns": true}),
        )
        .await?;
    remote
        .request("thread/shellCommand", json!({
            "threadId": source, "command": format!("printf {NEWEST_MARKER}"), "timeoutMs": 10000,
        }))
        .await?;
    remote
        .wait_for_thread_notification("turn/started", &source)
        .await?;
    remote
        .wait_for_thread_notification("turn/completed", &source)
        .await?;
    remote.close(&source).await?;
    let cwd = status
        .pointer("/workspace/worktreePath")
        .and_then(Value::as_str)
        .context("source worktree missing")?;
    tui::trust_projects(paths, &[repository, Path::new(cwd)])?;
    Ok(source)
}

async fn capture_while_working(
    paths: &TestPaths,
    binary: &Path,
    repository: &Path,
    source: &str,
) -> Result<String> {
    let mut remote = RemoteAppServer::connect_with_capabilities(paths, true).await?;
    remote
        .request(
            "thread/resume",
            json!({"threadId": source, "excludeTurns": true}),
        )
        .await?;
    let history = remote
        .request(
            "thread/read",
            json!({"threadId": source, "includeTurns": true}),
        )
        .await?;
    let cutoff = history
        .pointer("/thread/turns/1/id")
        .and_then(Value::as_str)
        .context("source completed boundary missing")?;
    let release = paths.data_dir.join("release-source-before-restart");
    start_waiting_source(&mut remote, source, &release).await?;
    wait_for_phase(paths, binary, repository, WORKSPACE_NAME, "active").await?;
    run_cli(
        paths,
        binary,
        repository,
        &[
            "create",
            FORK_WORKSPACE_NAME,
            "-c",
            WORKSPACE_NAME,
            "--profile",
            "capture-proof",
        ],
    )
    .await?;
    let captured = workspace_status_for(paths, binary, repository, FORK_WORKSPACE_NAME).await?;
    let child = native_id(&captured)?.to_owned();
    ensure!(child != source, "create reused the source thread");
    ensure!(
        captured.pointer("/workspace/context/resolved/context/lastTurnId") == Some(&json!(cutoff)),
        "create did not freeze the completed native boundary: {captured}"
    );
    assert_inactive_executor(&captured)?;
    assert_captured_history(&mut remote, &child, cutoff).await?;
    wait_for_phase(paths, binary, repository, WORKSPACE_NAME, "active").await?;
    fs::write(&release, "release")?;
    remote
        .wait_for_thread_notification("turn/completed", source)
        .await?;
    wait_for_phase(paths, binary, repository, WORKSPACE_NAME, "idle").await?;
    assert_captured_history(&mut remote, &child, cutoff).await?;
    remote.close(source).await?;
    Ok(child)
}

async fn verify_after_restart(
    paths: &TestPaths,
    binary: &Path,
    repository: &Path,
    source: &str,
    child: &str,
) -> Result<()> {
    let status = workspace_status_for(paths, binary, repository, FORK_WORKSPACE_NAME).await?;
    ensure!(
        native_id(&status)? == child,
        "restart changed the captured child identity"
    );
    ensure!(
        status.pointer("/workspace/phase") == Some(&json!("not_loaded")),
        "restart did not leave the captured child unloaded: {status}"
    );
    assert_inactive_executor(&status)?;
    let cwd = status
        .pointer("/workspace/worktreePath")
        .and_then(Value::as_str)
        .context("child worktree missing")?;
    tui::trust_projects(paths, &[repository, Path::new(cwd)])?;
    let mut remote = RemoteAppServer::connect_with_capabilities(paths, true).await?;
    remote
        .request(
            "thread/resume",
            json!({"threadId": source, "excludeTurns": true}),
        )
        .await?;
    let release = paths.data_dir.join("release-source-after-restart");
    start_waiting_source(&mut remote, source, &release).await?;
    wait_for_phase(paths, binary, repository, WORKSPACE_NAME, "active").await?;
    tui::verify_inherited_context_resume(paths, binary, repository).await?;
    let sent = run_cli(
        paths,
        binary,
        repository,
        &[
            "send",
            FORK_WORKSPACE_NAME,
            "Verify independently captured context.",
            "--wait",
        ],
    )
    .await?;
    ensure!(
        String::from_utf8_lossy(&sent.stdout).contains("context-capture-proof"),
        "child send did not complete through the local test provider: {}",
        String::from_utf8_lossy(&sent.stdout)
    );
    wait_for_phase(paths, binary, repository, WORKSPACE_NAME, "active").await?;
    let history = remote
        .request(
            "thread/read",
            json!({"threadId": child, "includeTurns": true}),
        )
        .await?;
    ensure!(
        !history.to_string().contains(LATER_MARKER),
        "child inherited later source work"
    );
    let latest = workspace_status_for(paths, binary, repository, FORK_WORKSPACE_NAME).await?;
    ensure!(
        native_id(&latest)? == child,
        "jump/send replaced the captured conversation"
    );
    fs::write(&release, "release")?;
    remote
        .wait_for_thread_notification("turn/completed", source)
        .await?;
    remote.close(source).await
}

async fn start_waiting_source(
    remote: &mut RemoteAppServer,
    source: &str,
    release: &Path,
) -> Result<()> {
    remote.request("thread/shellCommand", json!({
        "threadId": source,
        "command": format!("while [ ! -f {} ]; do sleep 0.05; done; printf {LATER_MARKER}", shell_word(release)),
        "timeoutMs": 55000,
    })).await?;
    remote
        .wait_for_thread_notification("turn/started", source)
        .await?;
    Ok(())
}

async fn assert_captured_history(
    remote: &mut RemoteAppServer,
    child: &str,
    cutoff: &str,
) -> Result<()> {
    let history = remote
        .request(
            "thread/read",
            json!({"threadId": child, "includeTurns": true}),
        )
        .await?;
    let turns = history
        .pointer("/thread/turns")
        .and_then(Value::as_array)
        .context("child turns missing")?;
    ensure!(
        turns.len() == 2
            && turns[1]["id"] == cutoff
            && turns.iter().all(|turn| turn["status"] == "completed"),
        "fork did not capture exactly the completed source prefix: {history}"
    );
    ensure!(
        turns[0].to_string().contains("coco-native-history")
            && turns[1].to_string().contains(NEWEST_MARKER),
        "fork lost source context"
    );
    ensure!(
        !history.to_string().contains(LATER_MARKER),
        "fork included unfinished source work"
    );
    Ok(())
}

async fn wait_for_phase(
    paths: &TestPaths,
    binary: &Path,
    repository: &Path,
    name: &str,
    phase: &str,
) -> Result<()> {
    let deadline = Instant::now() + COMPATIBILITY_TIMEOUT;
    loop {
        let status = workspace_status_for(paths, binary, repository, name).await?;
        if status.pointer("/workspace/phase") == Some(&json!(phase)) {
            return Ok(());
        }
        ensure!(
            Instant::now() < deadline,
            "workspace never reached {phase}: {status}"
        );
        sleep(POLL_INTERVAL).await;
    }
}

fn native_id(status: &Value) -> Result<&str> {
    status
        .pointer("/workspace/codexThreadId")
        .and_then(Value::as_str)
        .context("create has no captured thread")
}

fn assert_inactive_executor(status: &Value) -> Result<()> {
    ensure!(
        status.pointer("/runtimeResources/state") == Some(&json!("inactive")),
        "context capture unexpectedly started an executor: {status}"
    );
    Ok(())
}
