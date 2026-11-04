use super::*;

#[path = "turn_control/legacy.rs"]
mod legacy;
#[path = "turn_control/public.rs"]
mod public;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires COCO_RUN_REAL_CODEX_COMPAT=1 and selected Codex; shell-only, no model calls"]
async fn native_stale_id_check_and_terminal_metadata_survive_restart() -> Result<()> {
    require_explicit_opt_in()?;
    let binary = env::var_os(CODEX_BINARY_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| "codex".into());
    let temporary = tempfile::tempdir()?;
    let paths = TestPaths::new(temporary.path());
    for directory in [&paths.home, &paths.codex_home, &paths.data_dir] {
        fs::create_dir_all(directory)?;
    }
    fs::write(
        paths.codex_home.join("config.toml"),
        "model_provider = \"turn-proof\"\n\
         [model_providers.turn-proof]\nname = \"Local turn proof\"\n\
         base_url = \"http://127.0.0.1:9/v1\"\nwire_api = \"responses\"\n\
         requires_openai_auth = false\nsupports_websockets = false\n",
    )?;
    verify_codex_version(&binary, &paths).await?;
    let repository = temporary.path().join("repository");
    prepare_repository(&repository)?;
    let mut server = RealAppServer::spawn(&paths, &binary, &repository, "turn-control").await?;
    let started = server
        .request(
            "thread/start",
            json!({
                "cwd": repository, "model": "local-turn-proof", "ephemeral": false,
            }),
        )
        .await?;
    let thread = started
        .pointer("/thread/id")
        .and_then(Value::as_str)
        .context("native thread ID missing")?
        .to_owned();
    let first = start_shell(&mut server, &thread, "printf first-turn").await?;
    assert_terminal(&mut server, &thread, &first, "completed").await?;
    let second = start_shell(&mut server, &thread, "sleep 2; printf second-turn").await?;
    ensure!(first != second);
    let stale = server
        .request_frame(
            "turn/interrupt",
            json!({
                "threadId": thread, "turnId": first,
            }),
        )
        .await?;
    // This checks an ALREADY stale target, not atomic safety when the target
    // finishes between native validation and its queued interrupt. CoCo keeps
    // that mutation disabled; this exclusively owned native fixture is safe.
    assert_terminal(&mut server, &thread, &second, "completed")
        .await
        .with_context(|| format!("stale interrupt affected the newer turn: {stale}"))?;
    let third = start_shell(&mut server, &thread, "sleep 2; printf third-turn").await?;
    server
        .request(
            "turn/interrupt",
            json!({"threadId": thread, "turnId": third}),
        )
        .await?;
    assert_terminal(&mut server, &thread, &third, "interrupted").await?;
    server.stop().await?;
    let mut restored =
        RealAppServer::spawn(&paths, &binary, &repository, "turn-control-restored").await?;
    let page = restored
        .request(
            "thread/turns/list",
            json!({
                "threadId": thread, "limit": 10, "itemsView": "notLoaded", "sortDirection": "desc",
            }),
        )
        .await?;
    let turns = page["data"].as_array().context("native turns missing")?;
    for (id, status) in [
        (&first, "completed"),
        (&second, "completed"),
        (&third, "interrupted"),
    ] {
        let turn = turns
            .iter()
            .find(|turn| turn["id"] == *id)
            .with_context(|| format!("persisted exact turn {id} missing"))?;
        ensure!(turn["status"] == status, "wrong native status: {turn}");
        ensure!(
            turn["items"].as_array().is_some_and(Vec::is_empty),
            "unexpected full history"
        );
    }
    let summary = restored
        .request(
            "thread/read",
            json!({"threadId": thread, "includeTurns": false}),
        )
        .await?;
    ensure!(
        summary.pointer("/thread/status/type") == Some(&json!("notLoaded")),
        "metadata lookup loaded thread"
    );
    restored.stop().await
}

async fn start_shell(server: &mut RealAppServer, thread: &str, command: &str) -> Result<String> {
    server
        .request(
            "thread/shellCommand",
            json!({
                "threadId": thread, "command": command, "timeoutMs": 10000,
            }),
        )
        .await?;
    let started = server
        .wait_for_thread_notification("turn/started", thread)
        .await?;
    Ok(started
        .pointer("/params/turn/id")
        .and_then(Value::as_str)
        .context("shell turn ID missing")?
        .to_owned())
}

async fn assert_terminal(
    server: &mut RealAppServer,
    thread: &str,
    turn: &str,
    status: &str,
) -> Result<()> {
    let event = server
        .wait_for_thread_notification("turn/completed", thread)
        .await?;
    ensure!(
        event.pointer("/params/turn/id") == Some(&json!(turn)),
        "wrong turn completed: {event}"
    );
    ensure!(
        event.pointer("/params/turn/status") == Some(&json!(status)),
        "unexpected turn outcome: {event}"
    );
    Ok(())
}
