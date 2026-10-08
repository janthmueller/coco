use super::*;

const LEGACY_THREAD: &str = "0192a058-0000-7000-8000-000000000123";
const REPAIRED_MARKER: &str = "coco-repaired-history";
const CHILD_NAME: &str = "legacy-context-child";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires COCO_RUN_REAL_CODEX_COMPAT=1 and selected Codex; no model calls"]
async fn installed_codex_rejects_legacy_boundary_before_git_and_accepts_a_new_turn() -> Result<()> {
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
    tui::trust_projects(&paths, &[&repository])?;
    write_legacy_history(&paths, &repository)?;
    let log = paths.data_dir.join("legacy-context.log");
    let mut daemon = spawn_daemon(&paths, &binary, &log)?;
    wait_for_file(&paths.socket, &mut daemon, &log).await?;
    let mut remote = RemoteAppServer::connect_with_capabilities(&paths, true).await?;
    let page = remote
        .request("thread/turns/list", json!({
            "threadId": LEGACY_THREAD, "itemsView": "notLoaded", "sortDirection": "desc", "limit": 50,
        }))
        .await?;
    ensure!(
        page["data"][0]["status"] == "completed",
        "legacy projection is not terminal: {page}"
    );
    ensure!(
        page["data"][0]["id"] == "rollout-2",
        "unexpected legacy boundary: {page}"
    );
    let error = run_cli(
        &paths,
        &binary,
        &repository,
        &["create", CHILD_NAME, "-c", LEGACY_THREAD],
    )
    .await
    .expect_err("synthetic boundary unexpectedly accepted");
    ensure!(
        error.to_string().contains("CONTEXT_SOURCE_UNSUPPORTED")
            && error
                .to_string()
                .contains("Finish a new turn in the source"),
        "legacy rejection is not actionable: {error}"
    );
    assert_no_child_artifacts(&paths, &binary, &repository).await?;
    let cutoff = complete_new_turn(&mut remote).await?;
    run_cli(
        &paths,
        &binary,
        &repository,
        &["create", CHILD_NAME, "-c", LEGACY_THREAD],
    )
    .await?;
    let child = workspace_status_for(&paths, &binary, &repository, CHILD_NAME).await?;
    let child_id = native_id(&child)?;
    ensure!(
        child_id != LEGACY_THREAD,
        "create adopted the source conversation"
    );
    ensure!(
        child.pointer("/workspace/context/resolved/context/lastTurnId") == Some(&json!(cutoff)),
        "create did not capture the new canonical boundary"
    );
    assert_inactive_executor(&child)?;
    assert_repaired_history(&mut remote, child_id, &cutoff).await?;
    remote.close(LEGACY_THREAD).await?;
    stop_daemon(&mut daemon, &log).await?;
    verify_runtime_cleanup(&paths)
}

async fn assert_repaired_history(
    remote: &mut RemoteAppServer,
    child_id: &str,
    cutoff: &str,
) -> Result<()> {
    let source = remote
        .request(
            "thread/read",
            json!({"threadId": LEGACY_THREAD, "includeTurns": true}),
        )
        .await?;
    let child = remote
        .request(
            "thread/read",
            json!({"threadId": child_id, "includeTurns": true}),
        )
        .await?;
    let source_turns = source
        .pointer("/thread/turns")
        .and_then(Value::as_array)
        .context("source turns missing")?;
    let child_turns = child
        .pointer("/thread/turns")
        .and_then(Value::as_array)
        .context("child turns missing")?;
    ensure!(
        source_turns.len() == 2
            && child_turns.len() == 2
            && source_turns[1]["id"] == cutoff
            && child_turns[1]["id"] == cutoff,
        "fork did not preserve the new canonical boundary"
    );
    // Legacy projection renumbers synthetic IDs after fork metadata is added.
    // Compare all native items/statuses instead; a host shell turn in legacy
    // history can have no items, so its text is not a valid persistence proof.
    ensure!(
        source_turns
            .iter()
            .zip(child_turns)
            .all(|(source, child)| source["status"] == "completed"
                && child["status"] == "completed"
                && source["items"] == child["items"]),
        "fork changed the native source conversation items"
    );
    ensure!(
        child_turns[0].to_string().contains("legacy-history"),
        "fork lost older history"
    );
    Ok(())
}

async fn complete_new_turn(remote: &mut RemoteAppServer) -> Result<String> {
    remote
        .request(
            "thread/resume",
            json!({"threadId": LEGACY_THREAD, "excludeTurns": true}),
        )
        .await?;
    remote
        .request("thread/shellCommand", json!({
            "threadId": LEGACY_THREAD, "command": format!("printf {REPAIRED_MARKER}"), "timeoutMs": 10000,
        }))
        .await?;
    remote
        .wait_for_thread_notification("turn/started", LEGACY_THREAD)
        .await?;
    remote
        .wait_for_thread_notification("turn/completed", LEGACY_THREAD)
        .await?;
    let page = remote
        .request("thread/turns/list", json!({
            "threadId": LEGACY_THREAD, "itemsView": "notLoaded", "sortDirection": "desc", "limit": 50,
        }))
        .await?;
    ensure!(
        page["data"][0]["status"] == "completed",
        "repair turn did not complete: {page}"
    );
    let cutoff = page["data"][0]["id"]
        .as_str()
        .context("canonical boundary missing")?;
    ensure!(
        !cutoff.starts_with("rollout-"),
        "repair still has a synthetic boundary"
    );
    Ok(cutoff.to_owned())
}

async fn assert_no_child_artifacts(
    paths: &TestPaths,
    binary: &Path,
    repository: &Path,
) -> Result<()> {
    let listed = run_cli(paths, binary, repository, &["list", "--json"]).await?;
    let listed: Value = serde_json::from_slice(&listed.stdout)?;
    ensure!(
        listed["workspaces"].as_array().map(Vec::len) == Some(0),
        "failed validation retained a child: {listed}"
    );
    let branch = Command::new("git")
        .args([
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/coco/{CHILD_NAME}"),
        ])
        .current_dir(repository)
        .output()
        .await?;
    ensure!(
        branch.status.code() == Some(1),
        "failed validation created a branch"
    );
    let trees = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repository)
        .output()
        .await?;
    ensure!(trees.status.success(), "could not inspect worktrees");
    ensure!(
        String::from_utf8_lossy(&trees.stdout)
            .lines()
            .filter(|line| line.starts_with("worktree "))
            .count()
            == 1,
        "failed validation created a worktree"
    );
    Ok(())
}

fn write_legacy_history(paths: &TestPaths, repository: &Path) -> Result<()> {
    // Exact upstream legacy shape: messages precede persisted TurnStarted IDs.
    // This fixture is private temporary test data, never an existing rollout.
    let directory = paths.codex_home.join("sessions/2026/10/08");
    fs::create_dir_all(&directory)?;
    let records = [
        json!({"timestamp": "2026-10-08T12:00:00Z", "type": "session_meta", "payload": {
            "id": LEGACY_THREAD, "session_id": LEGACY_THREAD, "timestamp": "2026-10-08T12:00:00Z",
            "cwd": repository, "originator": "codex", "cli_version": "0.0.0",
            "source": "cli", "model_provider": "openai",
        }}),
        json!({"timestamp": "2026-10-08T12:00:01Z", "type": "response_item", "payload": {
            "type": "message", "role": "user", "content": [{"type": "input_text", "text": "legacy-history"}],
        }}),
        json!({"timestamp": "2026-10-08T12:00:01Z", "type": "event_msg", "payload": {
            "type": "user_message", "message": "legacy-history", "kind": "plain",
        }}),
    ];
    fs::write(
        directory.join(format!("rollout-2026-10-08T12-00-00-{LEGACY_THREAD}.jsonl")),
        records
            .iter()
            .map(|record| format!("{record}\n"))
            .collect::<String>(),
    )?;
    Ok(())
}
