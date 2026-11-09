use super::*;
use crate::mcp_client::McpClient;
use sha2::{Digest, Sha256};

#[path = "setup/provider.rs"]
mod provider;

#[path = "setup/approval.rs"]
mod approval;

struct SetupFixture {
    _temporary: tempfile::TempDir,
    paths: TestPaths,
    binary: PathBuf,
    repository: PathBuf,
    worker: PathBuf,
    python: String,
    log: PathBuf,
}

async fn fixture() -> Result<SetupFixture> {
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
    let worker = temporary.path().join("never-persist-recipe-command.py");
    fs::write(&worker, include_str!("../support/worker_mcp.py"))?;
    let python = Command::new("python3")
        .args(["-c", "import sys; print(sys.executable)"])
        .output()
        .await?;
    ensure!(
        python.status.success(),
        "python3 is required for this opt-in fixture"
    );
    let python = String::from_utf8(python.stdout)?.trim().to_owned();
    let log = paths.data_dir.join("public-setup.log");
    Ok(SetupFixture {
        _temporary: temporary,
        paths,
        binary,
        repository,
        worker,
        python,
        log,
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires COCO_RUN_REAL_CODEX_COMPAT=1, selected Codex and python3; scripted loopback only, no real model"]
async fn public_setup_keeps_two_worker_grants_separate_across_start_resume_and_restart()
-> Result<()> {
    let SetupFixture {
        _temporary,
        paths,
        binary,
        repository,
        worker,
        python,
        log,
    } = fixture().await?;
    let provider = provider::ScriptedProvider::start(&paths, 6).await?;
    let a = setup_input(&paths, &worker, &python, "run-a")?;
    let b = setup_input(&paths, &worker, &python, "run-b")?;
    let mut daemon = spawn_daemon(&paths, &binary, &log)?;
    wait_for_file(&paths.socket, &mut daemon, &log).await?;
    assert_readonly(&paths, &binary, &repository, &a).await?;
    let mut control = client(&paths, &binary, &repository, true).await?;
    let created_a = control.call("workspaces.create", a.clone()).await?;
    let created_b = control.call("workspaces.create", b.clone()).await?;
    ensure!(
        created_a["state"] == "configured",
        "creation rejected: {created_a}"
    );
    ensure!(
        created_b["state"] == "configured",
        "creation rejected: {created_b}"
    );
    ensure!(created_a["codexThreadId"].is_null() && created_b["codexThreadId"].is_null());
    assert_fresh_opening_is_blocked(&paths, &repository).await?;
    assert_eq!(
        control.call("workspaces.create", a.clone()).await?,
        created_a
    );
    let sent_a = send(&mut control, "run-a", "send-a-1").await?;
    let attached = daemon_request(
        &paths,
        "workspace.attach",
        json!({"scope": {"kind": "repository", "path": repository}, "workspace": "run-a"}),
    )
    .await?;
    ensure!(attached["launch"]["kind"] == "resume");
    release_returned_attach(&paths, &attached).await?;
    let sent_b = send(&mut control, "run-b", "send-b-1").await?;
    ensure!(fs::read_to_string(paths.data_dir.join("run-a.submissions"))? == "run-a\n");
    ensure!(fs::read_to_string(paths.data_dir.join("run-b.submissions"))? == "run-b\n");
    let mut remote = RemoteAppServer::connect_with_capabilities(&paths, true).await?;
    remote
        .request(
            "thread/resume",
            json!({"threadId": sent_a["workspace"]["codexThreadId"], "excludeTurns": true}),
        )
        .await?;
    let denied = remote
        .request(
            "mcpServer/tool/call",
            json!({"threadId": sent_a["workspace"]["codexThreadId"],
        "server": "worker", "tool": "run.submit", "arguments": {"runId": "run-b"}}),
        )
        .await?;
    ensure!(
        denied["isError"] == true,
        "worker A accepted worker B's grant"
    );
    ensure!(sent_a["workspace"]["codexThreadId"] != sent_b["workspace"]["codexThreadId"]);
    remote
        .close(sent_b["workspace"]["codexThreadId"].as_str().unwrap())
        .await?;
    control.stop().await?;
    stop_daemon(&mut daemon, &log).await?;
    let mut daemon = spawn_daemon(&paths, &binary, &log)?;
    wait_for_file(&paths.socket, &mut daemon, &log).await?;
    let mut control = client(&paths, &binary, &repository, true).await?;
    let restored = control
        .call(
            "workspaceCreations.get",
            json!({"operationId": "setup-run-a"}),
        )
        .await?;
    assert_eq!(restored["workspaceId"], created_a["workspaceId"]);
    assert_eq!(
        restored["codexThreadId"],
        sent_a["workspace"]["codexThreadId"]
    );
    send(&mut control, "run-a", "send-a-2").await?;
    ensure!(fs::read_to_string(paths.data_dir.join("run-a.submissions"))? == "run-a\nrun-a\n");
    control.stop().await?;
    stop_daemon(&mut daemon, &log).await?;
    provider.finish().await?;
    assert_no_recipe_persistence(&paths, &worker, &log)?;
    verify_runtime_cleanup(&paths)
}

async fn assert_fresh_opening_is_blocked(paths: &TestPaths, repository: &Path) -> Result<()> {
    let denied = daemon_request(
        paths,
        "workspace.attach",
        json!({"scope": {"kind": "repository", "path": repository}, "workspace": "run-a"}),
    )
    .await;
    let error = denied.expect_err("fresh worker-bound jump must not strand an empty native thread");
    ensure!(error.to_string().contains("WORKER_MCP_FIRST_SEND_REQUIRED"));
    Ok(())
}

async fn assert_readonly(
    paths: &TestPaths,
    binary: &Path,
    repository: &Path,
    input: &Value,
) -> Result<()> {
    let mut reader = client(paths, binary, repository, false).await?;
    let tools = reader.request("tools/list", json!({})).await?;
    ensure!(
        !tools["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "workspaces.create")
    );
    let denied = reader
        .request(
            "tools/call",
            json!({"name": "workspaces.create", "arguments": input}),
        )
        .await;
    ensure!(
        match denied {
            Err(_) => true,
            Ok(value) => value["isError"] == true,
        },
        "read-only host created a workspace"
    );
    ensure!(
        reader
            .call(
                "workspaceCreations.get",
                json!({"operationId": "setup-run-a"})
            )
            .await?["state"]
            == "unknown"
    );
    reader.stop().await
}

fn assert_no_recipe_persistence(paths: &TestPaths, worker: &Path, log: &Path) -> Result<()> {
    let connection = rusqlite::Connection::open(&paths.database)?;
    for (table, column) in [
        ("workspace_creations", "receipt_json"),
        ("audit_events", "details_json"),
        ("events", "payload_json"),
    ] {
        let mut statement = connection.prepare(&format!("SELECT {column} FROM {table}"))?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        for json in rows {
            let json = json?;
            ensure!(
                !json.contains(worker.to_str().unwrap()),
                "recipe leaked into {table}"
            );
            ensure!(
                !json.contains("mcp_servers"),
                "raw MCP configuration persisted in {table}"
            );
        }
    }
    ensure!(
        !fs::read_to_string(log)?.contains(worker.to_str().unwrap()),
        "recipe leaked into logs"
    );
    Ok(())
}

fn setup_input(paths: &TestPaths, worker: &Path, python: &str, grant: &str) -> Result<Value> {
    let bytes = serde_json::to_vec(&json!({"mcp_servers": {"worker": {
        "command": python, "args": [worker, grant, paths.data_dir.join(format!("{grant}.submissions"))],
        "enabled_tools": ["run.submit"], "default_tools_approval_mode": "approve", "required": true
    }}}))?;
    let file = paths.data_dir.join(format!("{grant}.json"));
    fs::write(&file, &bytes)?;
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600))?;
    Ok(
        json!({"operationId": format!("setup-{grant}"), "name": grant,
        "worktree": {"kind": "newBranch", "base": {"kind": "revision", "revision": "HEAD"}},
        "changes": "reject", "profile": "default", "model": "local-worker-proof",
        "workerMcp": {"path": file, "sha256": hex::encode(Sha256::digest(&bytes))}}),
    )
}

async fn client(
    paths: &TestPaths,
    binary: &Path,
    repository: &Path,
    create: bool,
) -> Result<McpClient> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_coco-mcp"));
    paths.apply(&mut command, binary);
    command.arg("--repository").arg(repository);
    if create {
        command.args(["--allow-create", "--allow-send"]);
    }
    McpClient::spawn(&mut command).await
}

async fn send(client: &mut McpClient, workspace: &str, operation: &str) -> Result<Value> {
    let sent = client.call("workspaces.send", json!({"workspace": workspace, "operationId": operation, "message": "Submit the isolated fixture result."})).await?;
    timeout(COMPATIBILITY_TIMEOUT, async {
        loop {
            let turn = client
                .call(
                    "turns.get",
                    json!({"workspace": workspace, "operationId": operation}),
                )
                .await?;
            if turn["state"] == "finished" {
                ensure!(
                    turn["outcome"] == "completed",
                    "scripted worker turn failed: {turn}"
                );
                return Ok::<(), anyhow::Error>(());
            }
            sleep(POLL_INTERVAL).await;
        }
    })
    .await
    .context("scripted turn did not complete (including unexpected approval/input)")??;
    Ok(sent)
}
