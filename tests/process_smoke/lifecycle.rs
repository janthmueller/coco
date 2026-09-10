use super::app_server::*;
use super::support::*;
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[expect(
    clippy::too_many_lines,
    reason = "one end-to-end scenario keeps daemon, CLI, App Server, and cleanup assertions ordered"
)]
async fn real_daemon_and_cli_complete_a_fake_codex_turn() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let paths = TestPaths::new(temporary.path());
    let repository = temporary.path().join("repository");
    let daemon_log = temporary.path().join("cocod.log");
    let recovery_log = temporary.path().join("cocod-recovery.log");

    prepare_repository(&repository)?;
    prepare_codex_profile(&paths)?;
    write_fake_codex(&paths.fake_codex)?;
    super::hooks::prepare(&paths)?;

    let mut daemon = spawn_daemon(&paths, &daemon_log)?;

    wait_for_file(&paths.codex_args, &mut daemon, &daemon_log).await?;
    let arguments = read_arguments(&paths.codex_args)?;
    let endpoint = verify_app_server_arguments(&arguments, &paths.token)?;
    let address = endpoint
        .strip_prefix("ws://")
        .context("App Server endpoint was not a ws:// URL")?
        .parse::<SocketAddr>()
        .context("App Server endpoint had an invalid socket address")?;
    ensure!(
        address.ip().is_loopback(),
        "App Server was not loopback-only"
    );

    let capability_token = fs::read_to_string(&paths.token)?;
    ensure!(
        capability_token.len() == 64
            && capability_token
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit()),
        "App Server capability token was not a 64-character hex value"
    );
    assert_mode(&paths.token, 0o600)?;

    let listener = TcpListener::bind(address)
        .await
        .context("could not bind the fake App Server")?;
    let observed_authorization = Arc::new(Mutex::new(Vec::new()));
    let observed_requests = Arc::new(Mutex::new(Vec::new()));
    let observed_remote_requests = Arc::new(Mutex::new(Vec::new()));
    let (complete_sender, complete_receiver) = oneshot::channel();
    let app_server = tokio::spawn(run_fake_app_server(
        listener,
        Arc::clone(&observed_authorization),
        Arc::clone(&observed_requests),
        Arc::clone(&observed_remote_requests),
        complete_receiver,
    ));

    wait_for_file(&paths.socket, &mut daemon, &daemon_log).await?;
    wait_for_file(&paths.endpoint, &mut daemon, &daemon_log).await?;
    assert_mode(&paths.socket, 0o600)?;
    assert_mode(&paths.endpoint, 0o600)?;
    assert_mode(&paths.database, 0o600)?;
    super::hooks::verify_loaded(&paths, &repository).await?;

    let models = cli_json(&run_cli(&paths, &repository, &["model", "list", "--json"]).await?)?;
    assert_eq!(models["schemaVersion"], 8);
    assert_eq!(models["models"].as_array().map(Vec::len), Some(2));
    assert_eq!(models["models"][0]["model"], DEFAULT_MODEL);
    assert_eq!(models["models"][0]["isDefault"], true);
    assert_eq!(models["models"][1]["model"], MODEL_OVERRIDE);
    let human_models = run_cli(&paths, &repository, &["model", "ls"]).await?;
    let human_models = String::from_utf8_lossy(&human_models.stdout);
    ensure!(
        human_models.contains("MODEL")
            && human_models.contains("NAME")
            && human_models.contains("REASONING")
            && human_models.contains("(default)")
            && human_models.contains(DEFAULT_MODEL)
            && human_models.contains(MODEL_OVERRIDE),
        "coco model ls did not render the App Server catalog: {human_models}"
    );

    run_cli(&paths, &repository, &["repo", "add", "."]).await?;
    let repositories = cli_json(&run_cli(&paths, &repository, &["repo", "ls", "--json"]).await?)?;
    assert_eq!(repositories["schemaVersion"], 8);
    assert_eq!(
        repositories["repositories"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(
        repositories.pointer("/repositories/0/rootPath"),
        Some(&Value::String(
            repository.canonicalize()?.to_string_lossy().into_owned()
        ))
    );
    let human_repositories = run_cli(&paths, &repository, &["repo", "list"]).await?;
    let human_repositories = String::from_utf8_lossy(&human_repositories.stdout);
    ensure!(
        human_repositories.contains("REPOSITORY")
            && human_repositories.contains("PATH")
            && !human_repositories.contains(
                repositories["repositories"][0]["id"]
                    .as_str()
                    .context("registered repository had no ID")?
            )
            && !human_repositories.contains('\u{1b}'),
        "human repository output was noisy or terminal-dependent: {human_repositories}"
    );

    let empty_status = capture_cli(&paths, &repository, &["status"], None).await?;
    ensure!(
        empty_status.status.success()
            && String::from_utf8_lossy(&empty_status.stdout).contains("No workspaces."),
        "targetless status did not show the repository collection: stdout={} stderr={}",
        String::from_utf8_lossy(&empty_status.stdout),
        String::from_utf8_lossy(&empty_status.stderr)
    );
    let missing_name = capture_cli(&paths, &repository, &["create"], None).await?;
    ensure!(
        !missing_name.status.success()
            && String::from_utf8_lossy(&missing_name.stderr)
                .contains("workspace name is required without interactive input"),
        "non-terminal create unexpectedly prompted or returned an unclear error: {}",
        String::from_utf8_lossy(&missing_name.stderr)
    );

    let failed_jump = run_cli_with_jump_exit(
        &paths,
        &repository,
        &[
            "create",
            WORKSPACE_NAME,
            "--base",
            "HEAD",
            "--profile",
            PROFILE_NAME,
            "-m",
            MODEL_OVERRIDE,
            "-s",
            "Complete the process smoke test",
            "-j",
        ],
        23,
    )
    .await?;
    let failed_jump_output = String::from_utf8_lossy(&failed_jump.stdout);
    ensure!(
        failed_jump_output.contains(&format!("Created {WORKSPACE_NAME}"))
            && !failed_jump_output.contains(THREAD_ID)
            && !failed_jump_output.contains("profile:")
            && !failed_jump_output.contains('\u{1b}'),
        "create printed noisy or terminal-dependent output: {failed_jump_output}"
    );
    let failed_jump_error = String::from_utf8_lossy(&failed_jump.stderr);
    ensure!(
        failed_jump_error.contains(
            "workspace \"feat/process-smoke\" was created and its initial turn was accepted, but the Codex terminal UI did not open"
        ),
        "create did not explain its retained state after jump failure: {failed_jump_error}"
    );

    let listed = cli_json(&run_cli(&paths, &repository, &["list", "--json"]).await?)?;
    assert_eq!(listed["schemaVersion"], 8);
    let workspaces = listed["workspaces"]
        .as_array()
        .context("coco list did not return a workspaces array")?;
    ensure!(
        workspaces.len() == 1,
        "coco list returned an unexpected workspace count"
    );
    let workspace = &workspaces[0];
    let human_workspaces = run_cli(&paths, &repository, &["list"]).await?;
    let human_workspaces = String::from_utf8_lossy(&human_workspaces.stdout);
    ensure!(
        human_workspaces.contains("WORKSPACE")
            && human_workspaces.contains("STATE")
            && human_workspaces.contains("BRANCH")
            && human_workspaces.contains(WORKSPACE_NAME)
            && !human_workspaces.contains(
                workspace["id"]
                    .as_str()
                    .context("listed workspace had no ID")?
            )
            && !human_workspaces.contains('\u{1b}'),
        "human workspace output was noisy or terminal-dependent: {human_workspaces}"
    );
    let status_overview = cli_json(&run_cli(&paths, &repository, &["status", "--json"]).await?)?;
    assert_eq!(status_overview["schemaVersion"], 8);
    assert_eq!(
        status_overview["workspaces"].as_array().map(Vec::len),
        Some(1)
    );
    let followed_collection =
        run_cli_until_interrupt(&paths, &repository, &["status", "-a", "--follow"]).await?;
    let followed_collection = String::from_utf8_lossy(&followed_collection.stdout);
    ensure!(
        followed_collection.contains("REPOSITORY") && followed_collection.contains(WORKSPACE_NAME),
        "all-repository status follow did not print its initial state: {followed_collection}"
    );
    assert_eq!(workspace["name"], WORKSPACE_NAME);
    assert_eq!(workspace["repository"]["displayName"], "repository");
    assert_eq!(workspace["lifecycle"], "ready");
    ensure!(
        matches!(
            workspace["phase"].as_str(),
            Some("active" | "waiting_for_approval")
        ),
        "workspace exposed an unexpected phase while the approval arrived"
    );
    ensure!(
        matches!(
            workspace["threadRuntime"]["status"]["type"].as_str(),
            Some("idle" | "active")
        ),
        "workspace exposed an unexpected native thread status"
    );
    assert_eq!(workspace["threadRuntime"]["isFresh"], true);
    assert_eq!(workspace["codexThreadId"], THREAD_ID);
    assert_eq!(workspace["profile"]["name"], PROFILE_NAME);
    assert_eq!(workspace["profile"]["modelOverride"], MODEL_OVERRIDE);
    assert_eq!(
        workspace["profile"]["effectiveSettings"]["model"],
        MODEL_OVERRIDE
    );
    ensure!(
        workspace.get("goal").is_none(),
        "retired goal field was exposed"
    );
    let worktree = PathBuf::from(
        workspace["worktreePath"]
            .as_str()
            .context("prepared workspace had no worktree path")?,
    );
    ensure!(
        worktree.is_dir(),
        "prepared workspace worktree does not exist"
    );

    let missing_message = capture_cli(&paths, &repository, &["send", WORKSPACE_NAME], None).await?;
    ensure!(
        !missing_message.status.success()
            && String::from_utf8_lossy(&missing_message.stderr)
                .contains("message is required without interactive input"),
        "non-terminal send unexpectedly prompted or returned an unclear error: {}",
        String::from_utf8_lossy(&missing_message.stderr)
    );

    let waiting = wait_for_pending_decision(&paths, &repository).await?;
    assert_eq!(waiting["workspace"]["phase"], "waiting_for_approval");
    assert_eq!(waiting["workspace"]["waitReasons"], json!(["approval"]));
    let decision_id = waiting
        .pointer("/openDecisions/0/id")
        .and_then(Value::as_str)
        .context("status did not expose the pending decision ID")?
        .to_owned();
    ensure!(
        waiting
            .pointer("/openDecisions/0/nativeRequestId")
            .is_none(),
        "status leaked the native App Server request ID"
    );
    let human_status = run_cli(&paths, &repository, &["status", WORKSPACE_NAME]).await?;
    ensure!(
        String::from_utf8_lossy(&human_status.stdout)
            .contains(&format!("coco decide {decision_id}")),
        "human status did not show the decision command"
    );
    let decided = run_cli(
        &paths,
        &repository,
        &["decide", &decision_id, "--choice", "1"],
    )
    .await?;
    ensure!(
        String::from_utf8_lossy(&decided.stdout).contains("Response sent to Codex"),
        "coco decide did not confirm the response"
    );
    let active = wait_for_workspace_phase(&paths, &repository, "active").await?;
    assert_eq!(active["openDecisions"], json!([]));

    let repository_argument = repository.to_string_lossy().into_owned();
    let explicitly_scoped = cli_json(
        &run_cli(
            &paths,
            temporary.path(),
            &[&repository_argument, "status", WORKSPACE_NAME, "--json"],
        )
        .await?,
    )?;
    assert_eq!(explicitly_scoped["workspace"]["id"], workspace["id"]);

    let global_list =
        cli_json(&run_cli(&paths, temporary.path(), &["list", "-a", "--json"]).await?)?;
    assert_eq!(global_list["workspaces"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        global_list.pointer("/workspaces/0/repository/rootPath"),
        Some(&Value::String(
            repository.canonicalize()?.to_string_lossy().into_owned()
        ))
    );

    let globally_named = cli_json(
        &run_cli(
            &paths,
            temporary.path(),
            &["status", WORKSPACE_NAME, "-g", "--json"],
        )
        .await?,
    )?;
    assert_eq!(globally_named["workspace"]["id"], workspace["id"]);

    let workspace_id = workspace["id"].as_str().context("workspace had no ID")?;
    let globally_resolved = cli_json(
        &run_cli(
            &paths,
            temporary.path(),
            &["status", workspace_id, "--json"],
        )
        .await?,
    )?;
    assert_eq!(globally_resolved["workspace"]["id"], workspace["id"]);

    run_cli(&paths, &repository, &["jump", WORKSPACE_NAME]).await?;
    let jump_arguments = read_arguments(&paths.jump_args)?;
    verify_jump_arguments(&jump_arguments, &endpoint, &worktree)?;
    ensure!(
        !jump_arguments
            .iter()
            .any(|argument| argument == capability_token.trim()),
        "jump exposed the capability token in its arguments"
    );

    remote_tui_session(&endpoint, capability_token.trim(), true).await?;
    assert_eq!(
        workspace_status(&paths, &repository).await?["workspace"]["phase"],
        "active"
    );
    remote_tui_session(&endpoint, capability_token.trim(), false).await?;
    assert_eq!(
        workspace_status(&paths, &repository).await?["workspace"]["phase"],
        "active"
    );

    complete_sender
        .send(())
        .map_err(|_| anyhow::anyhow!("fake App Server stopped before turn completion"))?;
    let completed = wait_for_workspace_phase(&paths, &repository, "idle").await?;
    assert_eq!(completed["workspace"]["activeTurnId"], Value::Null);
    let followed =
        run_cli_until_interrupt(&paths, &repository, &["status", WORKSPACE_NAME, "--follow"])
            .await?;
    ensure!(
        !String::from_utf8_lossy(&followed.stdout).contains("Fake Codex completed the turn."),
        "status --follow leaked a completed Codex message"
    );
    let waited = run_cli(
        &paths,
        &repository,
        &["send", WORKSPACE_NAME, "Complete another turn", "--wait"],
    )
    .await?;
    assert_eq!(
        String::from_utf8_lossy(&waited.stdout),
        "Fake Codex completed the waited turn.\n"
    );
    let followed_after_wait =
        run_cli_until_interrupt(&paths, &repository, &["status", WORKSPACE_NAME, "--follow"])
            .await?;
    ensure!(
        !String::from_utf8_lossy(&followed_after_wait.stdout)
            .contains("Fake Codex completed the waited turn."),
        "status --follow repeated the response owned by send --wait"
    );
    let initial_generation = completed["workspace"]["threadRuntime"]["runtimeGeneration"]
        .as_str()
        .context("workspace had no initial runtime generation")?
        .to_owned();
    let second_worktree = super::multi_client::exercise(&paths, temporary.path()).await?;
    let hook_events =
        super::hooks::wait_for_kinds(&paths, &[("workspace.created", 2), ("signal.emitted", 2)])
            .await?;
    ensure!(
        hook_events.iter().all(|event| {
            event["schemaVersion"] == 1
                && event["id"].as_str().is_some_and(|id| !id.is_empty())
                && event.pointer("/workspace/id").is_some()
                && event.pointer("/repository/id").is_some()
        }),
        "hook commands did not receive complete versioned event envelopes: {hook_events:?}"
    );
    super::hooks::verify_history(&paths, &repository, 4).await?;

    interrupt(&daemon).await?;
    let daemon_status = timeout(PROCESS_TIMEOUT, daemon.wait())
        .await
        .context("cocod did not stop after SIGINT")??;
    ensure!(
        daemon_status.success(),
        "cocod exited with {daemon_status}: {}",
        read_log(&daemon_log)
    );

    let server_result = timeout(PROCESS_TIMEOUT, app_server)
        .await
        .context("fake App Server did not stop")?
        .context("fake App Server workspace panicked")?;
    server_result?;

    let expected_authorization = format!("Bearer {capability_token}");
    {
        let authorizations = observed_authorization
            .lock()
            .expect("authorization capture mutex was poisoned");
        assert_eq!(authorizations.len(), 3);
        assert!(
            authorizations
                .iter()
                .all(|authorization| authorization == &expected_authorization)
        );
    }
    verify_codex_requests(
        &observed_requests
            .lock()
            .expect("request capture mutex was poisoned"),
        &worktree,
    )?;
    verify_remote_tui_requests(
        &observed_remote_requests
            .lock()
            .expect("remote request capture mutex was poisoned"),
    )?;
    for runtime_file in [&paths.socket, &paths.endpoint, &paths.token] {
        ensure!(
            !runtime_file.exists(),
            "runtime file was not removed: {}",
            runtime_file.display()
        );
    }

    fs::remove_file(&paths.codex_args)?;
    let mut recovered_daemon = spawn_daemon(&paths, &recovery_log)?;
    wait_for_file(&paths.codex_args, &mut recovered_daemon, &recovery_log).await?;
    let recovery_arguments = read_arguments(&paths.codex_args)?;
    let recovery_endpoint = verify_app_server_arguments(&recovery_arguments, &paths.token)?;
    let recovery_capability_token = fs::read_to_string(&paths.token)?;
    ensure!(
        recovery_capability_token.len() == 64,
        "recovery App Server capability token had an unexpected length"
    );
    assert_mode(&paths.token, 0o600)?;
    let recovery_address = recovery_endpoint
        .strip_prefix("ws://")
        .context("recovery App Server endpoint was not a ws:// URL")?
        .parse::<SocketAddr>()
        .context("recovery App Server endpoint had an invalid socket address")?;
    let recovery_listener = TcpListener::bind(recovery_address)
        .await
        .context("could not bind the recovery App Server")?;
    let recovery_authorization = Arc::new(Mutex::new(Vec::new()));
    let recovery_requests = Arc::new(Mutex::new(Vec::new()));
    let recovery_server = tokio::spawn(run_fake_recovery_server(
        recovery_listener,
        Arc::clone(&recovery_authorization),
        Arc::clone(&recovery_requests),
        worktree.clone(),
        second_worktree,
    ));

    wait_for_file(&paths.socket, &mut recovered_daemon, &recovery_log).await?;
    super::multi_client::verify_replay(&paths, temporary.path()).await?;
    let recovered = workspace_status(&paths, &repository).await?;
    assert_eq!(recovered["workspace"]["phase"], "not_loaded");
    assert_eq!(recovered["workspace"]["threadRuntime"]["isFresh"], true);
    assert_ne!(
        recovered["workspace"]["threadRuntime"]["runtimeGeneration"],
        initial_generation
    );
    assert_eq!(recovered["workspace"]["codexThreadId"], THREAD_ID);
    ensure!(
        recovery_requests
            .lock()
            .expect("recovery request capture mutex was poisoned")
            .iter()
            .all(|request| request.get("method") != Some(&json!("thread/resume"))),
        "passive status eagerly resumed the persisted thread"
    );

    run_cli(&paths, &repository, &["jump", WORKSPACE_NAME]).await?;
    let loaded = workspace_status(&paths, &repository).await?;
    assert_eq!(loaded["workspace"]["phase"], "idle");
    assert_eq!(
        loaded["workspace"]["threadRuntime"]["runtimeGeneration"],
        recovered["workspace"]["threadRuntime"]["runtimeGeneration"]
    );

    interrupt(&recovered_daemon).await?;
    let recovered_daemon_status = timeout(PROCESS_TIMEOUT, recovered_daemon.wait())
        .await
        .context("recovered cocod did not stop after SIGINT")??;
    ensure!(
        recovered_daemon_status.success(),
        "recovered cocod exited with {recovered_daemon_status}: {}",
        read_log(&recovery_log)
    );
    let recovery_server_result = timeout(PROCESS_TIMEOUT, recovery_server)
        .await
        .context("recovery App Server did not stop")?
        .context("recovery App Server workspace panicked")?;
    recovery_server_result?;
    verify_recovery_requests(
        &recovery_requests
            .lock()
            .expect("recovery request capture mutex was poisoned"),
        &worktree,
    )?;
    let recovery_authorizations = recovery_authorization
        .lock()
        .expect("recovery authorization capture mutex was poisoned");
    assert_eq!(recovery_authorizations.len(), 1);
    assert_eq!(
        recovery_authorizations[0],
        format!("Bearer {recovery_capability_token}")
    );
    for runtime_file in [&paths.socket, &paths.endpoint, &paths.token] {
        ensure!(
            !runtime_file.exists(),
            "recovery runtime file was not removed: {}",
            runtime_file.display()
        );
    }
    Ok(())
}
