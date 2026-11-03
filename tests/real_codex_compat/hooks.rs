use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires COCO_RUN_REAL_CODEX_COMPAT=1 and the selected local Codex executable"]
async fn installed_codex_runs_native_session_hooks_through_app_server() -> Result<()> {
    require_explicit_opt_in()?;
    let codex_binary = env::var_os(CODEX_BINARY_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("codex"));
    let temporary = tempfile::tempdir()?;
    let paths = TestPaths::new(temporary.path());
    fs::create_dir_all(&paths.home)?;
    fs::create_dir_all(&paths.codex_home)?;
    fs::create_dir_all(&paths.data_dir)?;
    verify_codex_version(&codex_binary, &paths).await?;

    let repository = temporary.path().join("repository");
    prepare_repository(&repository)?;
    let capture = temporary.path().join("native-session-start.json");
    let script = temporary.path().join("native-session-start-hook");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\nset -eu\nIFS= read -r payload || true\nprintf '%s\\n' \"$payload\" > {}\n",
            shell_word(&capture)
        ),
    )?;
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700))?;
    fs::write(
        paths.codex_home.join("hooks.json"),
        serde_json::to_vec(&json!({
            "hooks": {
                "SessionStart": [{
                    "matcher": "startup",
                    "hooks": [{
                        "type": "command",
                        "command": shell_word(&script),
                        "timeout": 5,
                    }],
                }],
            },
        }))?,
    )?;
    install_provider(&paths)?;

    let mut server =
        RealAppServer::spawn(&paths, &codex_binary, &repository, "native-session-hook").await?;
    let started = server
        .request(
            "thread/start",
            json!({
                "cwd": &repository,
                "config": {
                    "bypass_hook_trust": true,
                    "model_provider": "hook-proof",
                    "model_providers": {
                        "hook-proof": {
                            "name": "Hook proof",
                            "base_url": "http://127.0.0.1:1/v1",
                            "wire_api": "responses",
                            "requires_openai_auth": false,
                            "supports_websockets": false,
                        },
                    },
                },
                "ephemeral": false,
                "model": "hook-proof-model",
            }),
        )
        .await?;
    let thread_id = started
        .pointer("/thread/id")
        .and_then(Value::as_str)
        .context("thread/start returned no thread id")?;
    server
        .request(
            "turn/start",
            json!({
                "threadId": thread_id,
                "cwd": &repository,
                "input": [{"type": "text", "text": "Run the hook compatibility proof."}],
            }),
        )
        .await?;
    let deadline = Instant::now() + COMPATIBILITY_TIMEOUT;
    while !capture.exists() && Instant::now() < deadline {
        sleep(POLL_INTERVAL).await;
    }
    let event: Value = serde_json::from_slice(&fs::read(&capture).with_context(|| {
        format!(
            "native SessionStart hook did not write {}; App Server log:\n{}",
            capture.display(),
            read_log(&server.log)
        )
    })?)?;
    ensure!(
        event["hook_event_name"] == "SessionStart"
            && event["source"] == "startup"
            && event["session_id"] == thread_id
            && event["cwd"].as_str() == repository.to_str(),
        "native SessionStart hook received an unexpected event: {event}"
    );
    server.stop().await
}

fn install_provider(paths: &TestPaths) -> Result<()> {
    fs::write(
        paths.codex_home.join("config.toml"),
        "model_provider = \"hook-proof\"\n[model_providers.hook-proof]\n\
         name = \"Hook proof\"\nbase_url = \"http://127.0.0.1:1/v1\"\n\
         wire_api = \"responses\"\nrequires_openai_auth = false\nsupports_websockets = false\n",
    )?;
    Ok(())
}
