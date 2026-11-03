use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires COCO_RUN_REAL_CODEX_COMPAT=1 and selected Codex; no model calls"]
async fn installed_codex_checks_saved_provider_before_resume_or_input() -> Result<()> {
    require_explicit_opt_in()?;
    let binary = env::var_os(CODEX_BINARY_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| "codex".into());
    let temporary = tempfile::tempdir()?;
    let paths = TestPaths::new(temporary.path());
    for directory in [&paths.home, &paths.codex_home, &paths.data_dir] {
        fs::create_dir_all(directory)?;
    }
    verify_codex_version(&binary, &paths).await?;
    let repository = temporary.path().join("repository");
    prepare_repository(&repository)?;
    install_provider_config(&paths, "provider-current")?;
    let mut server =
        RealAppServer::spawn(&paths, &binary, &repository, "provider-contract").await?;
    let current = server
        .request(
            "thread/start",
            json!({
                "cwd": repository, "model": "provider-proof", "ephemeral": false,
            }),
        )
        .await?;
    ensure!(current["modelProvider"] == "provider-current");
    let other = server
        .request(
            "thread/start",
            json!({
                "cwd": repository, "model": "provider-proof", "ephemeral": false,
                "config": {"model_provider": "provider-other"},
            }),
        )
        .await?;
    ensure!(other["modelProvider"] == "provider-other");
    let thread_id = other
        .pointer("/thread/id")
        .and_then(Value::as_str)
        .context("provider test thread ID missing")?;
    // An empty thread cannot be resumed regardless of provider. Materialize
    // actual history without inference before testing the provider rejection.
    server.request("thread/shellCommand", json!({
        "threadId": thread_id, "command": "printf coco-provider-contract", "timeoutMs": 10000,
    })).await?;
    server
        .wait_for_thread_notification("turn/completed", thread_id)
        .await?;
    for (method, params) in [
        (
            "thread/resume",
            json!({"threadId": thread_id, "excludeTurns": true}),
        ),
        (
            "turn/start",
            json!({"threadId": thread_id,
            "input": [{"type": "text", "text": "Must not reach a model"}]}),
        ),
    ] {
        let response = server.request_frame(method, params).await?;
        ensure!(
            response.pointer("/error/code") == Some(&json!(-32600)),
            "{method} unexpectedly accepted a provider mismatch: {response}"
        );
        let data = &response["error"]["data"];
        ensure!(
            data["type"] == "modelProviderMismatch"
                && data["modelProvider"] == "provider-other"
                && data["currentModelProvider"] == "provider-current",
            "{method} returned unexpected mismatch evidence: {response}"
        );
    }
    server.stop().await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires COCO_RUN_REAL_CODEX_COMPAT=1 and selected Codex; no external model calls"]
async fn coco_reports_provider_rejection_and_can_retry_after_configuration_repair() -> Result<()> {
    require_explicit_opt_in()?;
    let binary = env::var_os(CODEX_BINARY_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| "codex".into());
    let temporary = tempfile::tempdir()?;
    let paths = TestPaths::new(temporary.path());
    for directory in [&paths.home, &paths.codex_home, &paths.data_dir] {
        fs::create_dir_all(directory)?;
    }
    verify_codex_version(&binary, &paths).await?;
    let repository = temporary.path().join("repository");
    prepare_repository(&repository)?;
    install_provider_config(&paths, "provider-current")?;
    fs::write(
        paths.codex_home.join("other.config.toml"),
        "model_provider = \"provider-other\"\n",
    )?;
    let log = temporary.path().join("cocod-provider-rejection.log");
    let mut daemon = spawn_daemon(&paths, &binary, &log)?;
    wait_for_file(&paths.socket, &mut daemon, &log)
        .await
        .context("initial provider-test daemon startup")?;
    run_cli(
        &paths,
        &binary,
        &repository,
        &[
            "create",
            "provider-rejection",
            "--profile",
            "other",
            "--model",
            "provider-proof",
        ],
    )
    .await?;
    for id in ["first-rejected", "first-rejected", "second-rejected"] {
        assert_send_rejected(
            &paths,
            &binary,
            &repository,
            id,
            "Must not reach a model",
            "CODEX_ERROR",
        )
        .await?;
        let status =
            workspace_status_for(&paths, &binary, &repository, "provider-rejection").await?;
        ensure!(
            status["workspace"]["phase"] != "unavailable",
            "rejection poisoned status: {status}"
        );
        ensure!(
            status["workspace"]["activeTurnId"].is_null(),
            "rejection left a turn guard: {status}"
        );
    }
    assert_send_rejected(
        &paths,
        &binary,
        &repository,
        "first-rejected",
        "Changed message",
        "IDEMPOTENCY_CONFLICT",
    )
    .await?;
    stop_daemon(&mut daemon, &log).await?;
    // Follow the public repair instructions without changing the saved profile.
    // The provider endpoint stays unreachable loopback: no external inference.
    install_provider_config(&paths, "provider-other")?;
    let restart_log = temporary.path().join("cocod-provider-repaired.log");
    let mut daemon = spawn_daemon(&paths, &binary, &restart_log)?;
    wait_for_file(&paths.socket, &mut daemon, &restart_log)
        .await
        .context("provider-test repaired daemon startup")?;
    run_cli(
        &paths,
        &binary,
        &repository,
        &[
            "send",
            "provider-rejection",
            "Must not reach a model",
            "--operation-id",
            "first-rejected",
        ],
    )
    .await?;
    stop_daemon(&mut daemon, &restart_log).await
}

async fn assert_send_rejected(
    paths: &TestPaths,
    binary: &Path,
    repository: &Path,
    id: &str,
    message: &str,
    expected: &str,
) -> Result<()> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_coco"));
    paths.apply(&mut command, binary);
    let output = timeout(
        COMPATIBILITY_TIMEOUT,
        command
            .current_dir(repository)
            .args([
                "send",
                "provider-rejection",
                message,
                "--operation-id",
                id,
                "--no-input",
            ])
            .kill_on_drop(true)
            .output(),
    )
    .await??;
    let error = String::from_utf8_lossy(&output.stderr);
    ensure!(
        !output.status.success() && error.contains(expected),
        "unexpected send result: {error}"
    );
    ensure!(
        !error.contains("OPERATION_UNCERTAIN"),
        "known rejection became ambiguous: {error}"
    );
    if expected == "CODEX_ERROR" {
        ensure!(
            error.contains("Match model_provider"),
            "provider guidance missing: {error}"
        );
        ensure!(
            !error.contains("provider-current") && !error.contains("provider-other"),
            "private provider names escaped into CLI error: {error}"
        );
    }
    Ok(())
}

fn install_provider_config(paths: &TestPaths, selected: &str) -> Result<()> {
    let mut config = format!("model_provider = \"{selected}\"\n");
    for name in ["provider-current", "provider-other"] {
        config.push_str(&format!(
            "[model_providers.{name}]\nname = \"{name}\"\n\
             base_url = \"http://127.0.0.1:1/v1\"\nwire_api = \"responses\"\n\
             requires_openai_auth = false\nsupports_websockets = false\n"
        ));
    }
    fs::write(paths.codex_home.join("config.toml"), config)?;
    Ok(())
}
