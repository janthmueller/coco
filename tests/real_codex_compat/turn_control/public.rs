use super::super::*;
use crate::mcp_client::McpClient;

#[path = "public/provider.rs"]
mod provider;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires COCO_RUN_REAL_CODEX_COMPAT=1 and selected Codex; isolated loopback fixture, no real model"]
async fn public_turn_tools_keep_strict_and_explicit_best_effort_attempts_separate_across_restart()
-> Result<()> {
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
    let _provider = provider::PausedProvider::start(&paths).await?;
    let repository = temporary.path().join("repository");
    prepare_repository(&repository)?;
    let log = temporary.path().join("public-turn-control.log");
    let mut daemon = spawn_daemon(&paths, &binary, &log)?;
    wait_for_file(&paths.socket, &mut daemon, &log).await?;
    run_cli(
        &paths,
        &binary,
        &repository,
        &["create", WORKSPACE_NAME, "--model", "local-turn-proof"],
    )
    .await?;
    let mut client = spawn_client(&paths, &binary, &repository, true).await?;
    let first = start(&mut client, "public-first").await?;
    assert_eq!(get(&mut client, "public-first").await?["state"], "running");
    reject_unsafe_interrupt(&mut client, "public-first").await?;
    let requested = request_best_effort(&mut client, "public-first").await?;
    ensure!(requested["interruption"] == "requested" || requested["interruption"] == "confirmed");
    let completed = terminal(&mut client, "public-first").await?;
    assert_eq!(completed["outcome"], "interrupted");
    assert_eq!(completed["codexTurnId"], first["codexTurnId"]);
    assert!(completed["interruptRequestedAtMs"].is_i64());
    assert_eq!(
        completed["interruptAttempt"],
        json!({"mode": "bestEffort", "state": "requested"})
    );
    let second = start(&mut client, "public-second").await?;
    assert_ne!(second["codexTurnId"], first["codexTurnId"]);
    assert_eq!(
        client
            .call("turns.interrupt", input("public-first"))
            .await?["interruption"],
        "confirmed"
    );
    assert_eq!(get(&mut client, "public-second").await?["state"], "running");
    let replay = request_best_effort(&mut client, "public-first").await?;
    assert_eq!(replay["turn"], completed);
    assert_eq!(replay["interruption"], "confirmed");
    assert_eq!(get(&mut client, "public-second").await?["state"], "running");
    reject_unsafe_interrupt(&mut client, "public-second").await?;
    finish_native_fixture(&paths, &second).await?;
    let second_completed = terminal(&mut client, "public-second").await?;
    assert_eq!(second_completed["outcome"], "interrupted");
    client.stop().await?;
    stop_daemon(&mut daemon, &log).await?;
    let mut daemon = spawn_daemon(&paths, &binary, &log)?;
    wait_for_file(&paths.socket, &mut daemon, &log).await?;
    let mut client = spawn_client(&paths, &binary, &repository, false).await?;
    assert_eq!(get(&mut client, "public-first").await?, completed);
    assert_eq!(get(&mut client, "public-second").await?, second_completed);
    client.stop().await?;
    stop_daemon(&mut daemon, &log).await?;
    verify_runtime_cleanup(&paths)
}

async fn request_best_effort(client: &mut McpClient, operation: &str) -> Result<Value> {
    let mut arguments = input(operation);
    arguments["mode"] = json!("bestEffort");
    client.call("turns.interrupt", arguments).await
}

async fn reject_unsafe_interrupt(client: &mut McpClient, operation: &str) -> Result<()> {
    let rejected = client
        .request(
            "tools/call",
            json!({"name": "turns.interrupt", "arguments": input(operation)}),
        )
        .await?;
    ensure!(
        rejected["isError"] == true,
        "unsafe mutation was accepted: {rejected}"
    );
    assert_eq!(
        rejected["structuredContent"]["error"]["code"],
        "TURN_INTERRUPT_UNSUPPORTED"
    );
    let untouched = get(client, operation).await?;
    assert_eq!(untouched["state"], "running");
    assert!(untouched["interruptRequestedAtMs"].is_null());
    Ok(())
}

async fn finish_native_fixture(paths: &TestPaths, sent: &Value) -> Result<()> {
    // Test-only cleanup: this thread has a single controlled writer and no
    // concurrent new turn. This is not a CoCo cancellation implementation.
    let thread = sent["workspace"]["codexThreadId"]
        .as_str()
        .context("thread missing")?;
    let mut remote = RemoteAppServer::connect_with_capabilities(paths, true).await?;
    remote
        .request(
            "turn/interrupt",
            json!({"threadId": thread, "turnId": sent["codexTurnId"]}),
        )
        .await?;
    remote.close(thread).await
}

async fn spawn_client(
    paths: &TestPaths,
    binary: &Path,
    repository: &Path,
    writable: bool,
) -> Result<McpClient> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_coco-mcp"));
    paths.apply(&mut command, binary);
    command.arg("--repository").arg(repository);
    if writable {
        command.args(["--allow-send", "--allow-interrupt"]);
    }
    McpClient::spawn(&mut command).await
}

fn input(operation: &str) -> Value {
    json!({"workspace": WORKSPACE_NAME, "operationId": operation})
}

async fn start(client: &mut McpClient, operation: &str) -> Result<Value> {
    client
        .call(
            "workspaces.send",
            json!({"workspace": WORKSPACE_NAME,
        "message": "Isolated fixture waiting for interruption", "operationId": operation}),
        )
        .await
}

async fn get(client: &mut McpClient, operation: &str) -> Result<Value> {
    client.call("turns.get", input(operation)).await
}

async fn terminal(client: &mut McpClient, operation: &str) -> Result<Value> {
    timeout(COMPATIBILITY_TIMEOUT, async {
        loop {
            let state = get(client, operation).await?;
            if state["state"] == "finished" {
                return Ok(state);
            }
            sleep(POLL_INTERVAL).await;
        }
    })
    .await
    .context("public turn outcome did not become terminal")?
}
