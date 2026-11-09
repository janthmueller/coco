use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires COCO_RUN_REAL_CODEX_COMPAT=1, Codex and python3; native base collision, no model calls"]
async fn native_base_slot_collision_is_rejected_instead_of_inheriting_old_fields() -> Result<()> {
    let fixture = fixture().await?;
    let paths = &fixture.paths;
    let provider = provider::ScriptedProvider::start(paths, 0).await?;
    let config_path = paths.codex_home.join("config.toml");
    let config = fs::read_to_string(&config_path)?;
    fs::write(
        &config_path,
        format!(
            "{config}\n[mcp_servers.worker]\ncommand = '/missing/old-worker'\n[mcp_servers.worker.env]\nOLD_GRANT = 'DO_NOT_LEAK_STALE_CONFIG'\n"
        ),
    )?;
    let input = setup_input(paths, &fixture.worker, &fixture.python, "run-a")?;
    let mut daemon = spawn_daemon(paths, &fixture.binary, &fixture.log)?;
    wait_for_file(&paths.socket, &mut daemon, &fixture.log).await?;
    let mut control = client(paths, &fixture.binary, &fixture.repository, true).await?;
    let receipt = control.call("workspaces.create", input).await?;
    ensure!(
        receipt["state"] == "rejected",
        "unsafe base collision accepted: {receipt}"
    );
    ensure!(receipt["errorCode"] == "WORKER_MCP_BINDING_INVALID");
    ensure!(receipt["workspaceId"].is_null() && !paths.worktrees.exists());
    ensure!(!receipt.to_string().contains("DO_NOT_LEAK_STALE_CONFIG"));
    control.stop().await?;
    stop_daemon(&mut daemon, &fixture.log).await?;
    provider.finish().await?;
    ensure!(!fs::read_to_string(&fixture.log)?.contains("DO_NOT_LEAK_STALE_CONFIG"));
    verify_runtime_cleanup(paths)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires COCO_RUN_REAL_CODEX_COMPAT=1, Codex and python3; scripted local approval, no real model"]
async fn native_auto_is_not_silently_upgraded_to_approve_for_a_mutating_worker_tool() -> Result<()>
{
    let fixture = fixture().await?;
    let paths = &fixture.paths;
    let provider = provider::ScriptedProvider::start(paths, 1).await?;
    let mut input = setup_input(paths, &fixture.worker, &fixture.python, "run-a")?;
    let path = paths.data_dir.join("run-a.json");
    let mut recipe: Value = serde_json::from_slice(&fs::read(&path)?)?;
    recipe["mcp_servers"]["worker"]["default_tools_approval_mode"] = json!("auto");
    let bytes = serde_json::to_vec(&recipe)?;
    fs::write(&path, &bytes)?;
    input["workerMcp"]["sha256"] = json!(hex::encode(Sha256::digest(&bytes)));
    let mut daemon = spawn_daemon(paths, &fixture.binary, &fixture.log)?;
    wait_for_file(&paths.socket, &mut daemon, &fixture.log).await?;
    let mut control = client(paths, &fixture.binary, &fixture.repository, true).await?;
    ensure!(control.call("workspaces.create", input).await?["state"] == "configured");
    let sent = control.call("workspaces.send", json!({"workspace": "run-a", "operationId": "approval-send", "message": "Submit the fixture result."})).await?;
    timeout(COMPATIBILITY_TIMEOUT, async {
        loop {
            let status = control
                .call("workspaces.status", json!({"workspace": "run-a"}))
                .await?;
            if status["workspace"]["phase"] == "waiting_for_approval" {
                break;
            }
            sleep(POLL_INTERVAL).await;
        }
        Ok::<(), anyhow::Error>(())
    })
    .await
    .context("native approval did not surface")??;
    ensure!(
        !paths.data_dir.join("run-a.submissions").exists(),
        "unapproved write ran"
    );
    // Explicit fixture cleanup of its sole known turn; not production logic.
    let mut remote = RemoteAppServer::connect_with_capabilities(paths, true).await?;
    remote
        .request(
            "turn/interrupt",
            json!({"threadId": sent["workspace"]["codexThreadId"], "turnId": sent["codexTurnId"]}),
        )
        .await?;
    remote
        .close(sent["workspace"]["codexThreadId"].as_str().unwrap())
        .await?;
    control.stop().await?;
    stop_daemon(&mut daemon, &fixture.log).await?;
    provider.finish().await?;
    verify_runtime_cleanup(paths)
}
