use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires COCO_RUN_REAL_CODEX_COMPAT=1 and the pinned local Codex executable"]
async fn installed_codex_keeps_mcp_scopes_separate_on_start_fork_and_resume() -> Result<()> {
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
    let log = paths.data_dir.join("mcp-compat.log");
    let mut daemon = spawn_daemon(&paths, &binary, &log)?;
    wait_for_file(&paths.socket, &mut daemon, &log).await?;
    let first_repo = temporary.path().join("first-repo");
    let second_repo = temporary.path().join("second-repo");
    let first_lease = prepare(&paths, &binary, &first_repo).await?;
    let second_lease = prepare(&paths, &binary, &second_repo).await?;
    let mut remote = RemoteAppServer::connect_with_capabilities(&paths, true).await?;
    let first_config = mcp_config(&paths, &first_repo);
    let second_config = mcp_config(&paths, &second_repo);
    let first = start(&mut remote, &first_lease.cwd, &first_config).await?;
    let second = start(&mut remote, &second_lease.cwd, &second_config).await?;
    assert_ne!(first, second);
    assert_scope(&mut remote, &first, &first_lease.workspace_id).await?;
    assert_scope(&mut remote, &second, &second_lease.workspace_id).await?;
    // Starting B must not alter A's identically named MCP server.
    assert_scope(&mut remote, &first, &first_lease.workspace_id).await?;
    materialize(&mut remote, &first).await?;
    materialize(&mut remote, &second).await?;
    wait_for_remote_adoption(&paths, &first_lease, &first).await?;
    wait_for_remote_adoption(&paths, &second_lease, &second).await?;
    let first_signal = assert_signal(&mut remote, &first, &first_lease.workspace_id).await?;
    let second_signal = assert_signal(&mut remote, &second, &second_lease.workspace_id).await?;
    assert_ne!(
        first_signal["id"], second_signal["id"],
        "different senders shared idempotency state"
    );
    release_attach(&paths, &first_lease).await?;
    release_attach(&paths, &second_lease).await?;

    let forked = remote
        .request(
            "thread/fork",
            json!({
                "threadId": first, "cwd": second_repo, "config": second_config,
                "excludeTurns": true, "deferGoalContinuation": true
            }),
        )
        .await?;
    let child = forked
        .pointer("/thread/id")
        .and_then(Value::as_str)
        .context("fork ID missing")?
        .to_owned();
    assert_scope(&mut remote, &child, &second_lease.workspace_id).await?;
    let denied = emit(&mut remote, &child).await?;
    ensure!(
        denied["isError"] == true,
        "unbound native fork could emit as another workspace"
    );
    assert_scope(&mut remote, &first, &first_lease.workspace_id).await?;
    remote
        .request("thread/unsubscribe", json!({"threadId": first}))
        .await?;
    remote
        .request("thread/unsubscribe", json!({"threadId": second}))
        .await?;
    remote.close(&child).await?;
    verify_bound_fork(&paths, &binary, &second_repo, &first).await?;
    stop_daemon(&mut daemon, &log).await?;

    let mut daemon = spawn_daemon(&paths, &binary, &log)?;
    wait_for_file(&paths.socket, &mut daemon, &log).await?;
    let mut remote = RemoteAppServer::connect_with_capabilities(&paths, true).await?;
    attach_workspace(&paths, &first_repo).await?;
    attach_workspace(&paths, &second_repo).await?;
    // CoCo must restore the named profile itself before any direct resume.
    assert_eq!(
        assert_signal(&mut remote, &first, &first_lease.workspace_id).await?,
        first_signal
    );
    assert_eq!(
        assert_signal(&mut remote, &second, &second_lease.workspace_id).await?,
        second_signal
    );
    resume(&mut remote, &first, &first_lease.cwd, &first_config).await?;
    resume(&mut remote, &second, &second_lease.cwd, &second_config).await?;
    assert_eq!(
        assert_signal(&mut remote, &first, &first_lease.workspace_id).await?,
        first_signal
    );
    assert_eq!(
        assert_signal(&mut remote, &second, &second_lease.workspace_id).await?,
        second_signal
    );
    assert_scope(&mut remote, &first, &first_lease.workspace_id).await?;
    assert_scope(&mut remote, &second, &second_lease.workspace_id).await?;
    remote
        .request("thread/unsubscribe", json!({"threadId": first}))
        .await?;
    remote.close(&second).await?;
    stop_daemon(&mut daemon, &log).await?;
    verify_runtime_cleanup(&paths)?;
    Ok(())
}

async fn prepare(paths: &TestPaths, binary: &Path, repository: &Path) -> Result<AttachLease> {
    prepare_repository(repository)?;
    run_cli(paths, binary, repository, &["repo", "add", "."]).await?;
    let catalog = paths.data_dir.join("signal-catalog");
    fs::create_dir_all(&catalog)?;
    fs::write(
        catalog.join("review.requested@1.json"),
        serde_json::to_vec(&json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "description": "Request review", "type": "object",
            "properties": {"pr": {"type": "integer"}}, "required": ["pr"], "additionalProperties": false
        }))?,
    )?;
    let profile = repository
        .file_name()
        .and_then(|name| name.to_str())
        .context("profile name missing")?;
    fs::write(
        paths.codex_home.join(format!("{profile}.config.toml")),
        toml::to_string(&mcp_config(paths, repository))?,
    )?;
    run_cli(
        paths,
        binary,
        repository,
        &["create", WORKSPACE_NAME, "--profile", profile],
    )
    .await?;
    begin_fresh_attach(paths, repository).await
}

fn mcp_config(paths: &TestPaths, repository: &Path) -> Value {
    json!({"mcp_servers": {"coco": {
        "command": env!("CARGO_BIN_EXE_coco-mcp"),
        "args": ["--repository", repository, "--signal-catalog", paths.data_dir.join("signal-catalog"), "--allow-emit", "review.requested"],
        "env": {"COCO_SOCKET_PATH": paths.socket},
        "startup_timeout_sec": 10
    }}})
}

async fn start(remote: &mut RemoteAppServer, cwd: &Path, config: &Value) -> Result<String> {
    let result = remote
        .request(
            "thread/start",
            json!({"cwd": cwd, "config": config, "ephemeral": false}),
        )
        .await?;
    result
        .pointer("/thread/id")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .context("start ID missing")
}

async fn resume(
    remote: &mut RemoteAppServer,
    thread: &str,
    cwd: &Path,
    config: &Value,
) -> Result<()> {
    let result = remote
        .request(
            "thread/resume",
            json!({"threadId": thread, "cwd": cwd, "config": config, "excludeTurns": true}),
        )
        .await?;
    assert_eq!(result.pointer("/thread/id"), Some(&json!(thread)));
    Ok(())
}

async fn materialize(remote: &mut RemoteAppServer, thread: &str) -> Result<()> {
    remote
        .request(
            "thread/shellCommand",
            json!({"threadId": thread, "command": "printf coco-mcp-compat"}),
        )
        .await?;
    let result = remote
        .wait_for_thread_notification("turn/completed", thread)
        .await?;
    ensure!(
        result.pointer("/params/turn/status") == Some(&json!("completed")),
        "model-free shell action failed"
    );
    Ok(())
}

async fn emit(remote: &mut RemoteAppServer, thread: &str) -> Result<Value> {
    remote.request("mcpServer/tool/call", json!({
        "threadId": thread, "server": "coco", "tool": "signals.emit",
        "arguments": {"name": "review.requested", "version": 1, "payload": {"pr": 12}, "idempotencyKey": "native-proof"},
        "_meta": {"threadId": "must-be-replaced-by-native-codex"}
    })).await
}

async fn assert_signal(
    remote: &mut RemoteAppServer,
    thread: &str,
    workspace: &str,
) -> Result<Value> {
    let result = emit(remote, thread).await?;
    ensure!(result["isError"] != true, "native signal failed: {result}");
    let record = &result["structuredContent"];
    assert_eq!(
        record["threadId"], thread,
        "native metadata did not identify its actual thread"
    );
    assert_eq!(
        record["workspaceId"], workspace,
        "signal crossed workspace identities"
    );
    let page = remote
        .request(
            "mcpServer/tool/call",
            json!({
        "threadId": thread, "server": "coco", "tool": "signals.list", "arguments": {"workspaceId": workspace}
            }),
        )
        .await?;
    assert_eq!(page["structuredContent"]["signals"], json!([record]));
    Ok(record.clone())
}

async fn assert_scope(remote: &mut RemoteAppServer, thread: &str, workspace: &str) -> Result<()> {
    let result = remote
        .request(
            "mcpServer/tool/call",
            json!({
                "threadId": thread, "server": "coco", "tool": "workspaces.list", "arguments": {}
            }),
        )
        .await?;
    ensure!(
        result["isError"] != true,
        "native MCP call failed: {result}"
    );
    let workspaces = result["structuredContent"]
        .as_array()
        .context("native MCP list was not an array")?;
    let expected = workspaces
        .iter()
        .find(|entry| entry["id"] == workspace)
        .context("native MCP call crossed repositories")?;
    ensure!(
        workspaces
            .iter()
            .all(|entry| entry["repositoryId"] == expected["repositoryId"]),
        "MCP listed another repository's workspaces"
    );
    Ok(())
}

async fn verify_bound_fork(
    paths: &TestPaths,
    binary: &Path,
    repository: &Path,
    source: &str,
) -> Result<()> {
    run_cli(
        paths,
        binary,
        repository,
        &[
            "create",
            "native-signal-child",
            "--context",
            source,
            "--profile",
            "second-repo",
        ],
    )
    .await?;
    let attached = daemon_request(
        paths,
        "workspace.attach",
        json!({
            "scope": {"kind": "repository", "path": repository}, "workspace": "native-signal-child"
        }),
    )
    .await?;
    let workspace = attached["workspace"]["id"]
        .as_str()
        .context("fork workspace ID missing")?;
    let thread = attached["workspace"]["codexThreadId"]
        .as_str()
        .context("fork thread ID missing")?;
    assert_ne!(thread, source);
    let mut remote = RemoteAppServer::connect(paths).await?;
    assert_signal(&mut remote, thread, workspace).await?;
    remote.close(thread).await?;
    release_returned_attach(paths, &attached).await?;
    Ok(())
}
