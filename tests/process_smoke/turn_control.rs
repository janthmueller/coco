use super::support::*;
use super::*;
use crate::mcp_client::McpClient;

fn command(paths: &TestPaths, repository: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_coco-mcp"));
    paths.apply(&mut command);
    command.arg("--repository").arg(repository);
    command
}

pub(super) async fn wait_for_receipt(
    paths: &TestPaths,
    repository: &Path,
    operation: &str,
) -> Result<Value> {
    let mut client = McpClient::spawn(&mut command(paths, repository)).await?;
    let receipt = timeout(PROCESS_TIMEOUT, async {
        loop {
            let result = client
                .request(
                    "tools/call",
                    json!({"name": "turns.get",
                "arguments": {"workspace": WORKSPACE_NAME, "operationId": operation}}),
                )
                .await?;
            if result["structuredContent"]["state"] == "finished" {
                return Ok::<_, anyhow::Error>(result["structuredContent"].clone());
            }
            sleep(POLL_INTERVAL).await;
        }
    })
    .await
    .context("lost start response was not recoverable")??;
    client.stop().await?;
    Ok(receipt)
}

pub(super) async fn verify_receipt(
    paths: &TestPaths,
    repository: &Path,
    operation: &str,
) -> Result<Value> {
    let mut client = McpClient::spawn(&mut command(paths, repository)).await?;
    let receipt = client
        .call(
            "turns.get",
            json!({"workspace": WORKSPACE_NAME, "operationId": operation}),
        )
        .await?;
    assert_eq!(receipt["operationId"], operation);
    assert_eq!(receipt["codexThreadId"], super::multi_client::SECOND_THREAD);
    assert_eq!(receipt["codexTurnId"], "second-turn-2");
    assert_eq!(receipt["dispatch"], "accepted");
    assert_eq!(receipt["state"], "finished");
    assert_eq!(receipt["outcome"], "completed");
    assert!(receipt["outcomeObservedAtMs"].is_i64());
    assert!(receipt["unavailableReason"].is_null());
    client.stop().await?;
    Ok(receipt)
}

pub(super) async fn exercise(
    paths: &TestPaths,
    repository: &Path,
    foreign_repo: &Path,
    operation: &str,
) -> Result<()> {
    let receipt = verify_receipt(paths, repository, operation).await?;
    let input = json!({"workspace": WORKSPACE_NAME, "operationId": operation});
    let mut read_only = McpClient::spawn(&mut command(paths, repository)).await?;
    let listed = read_only.request("tools/list", json!({})).await?;
    let tools = listed["tools"].as_array().context("tools missing")?;
    ensure!(tools.iter().any(|tool| tool["name"] == "turns.get"));
    ensure!(!tools.iter().any(|tool| tool["name"] == "turns.interrupt"));
    for arguments in [
        input.clone(),
        json!({"workspace": WORKSPACE_NAME, "operationId": operation, "mode": "bestEffort"}),
    ] {
        let denied = read_only
            .request(
                "tools/call",
                json!({"name": "turns.interrupt", "arguments": arguments}),
            )
            .await;
        ensure!(
            denied
                .as_ref()
                .map_or(true, |result| result["isError"] == true)
        );
    }
    read_only.stop().await?;

    let mut interrupt_command = command(paths, repository);
    interrupt_command.arg("--allow-interrupt");
    let mut interrupter = McpClient::spawn(&mut interrupt_command).await?;
    let result = interrupter.call("turns.interrupt", input).await?;
    assert_eq!(result["interruption"], "alreadyFinished");
    assert_eq!(result["turn"], receipt);
    let listed = interrupter.request("tools/list", json!({})).await?;
    ensure!(
        !listed["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "workspaces.send")
    );
    interrupter.stop().await?;

    verify_repository_isolation(paths, foreign_repo, &receipt, operation).await
}

async fn verify_repository_isolation(
    paths: &TestPaths,
    repository: &Path,
    receipt: &Value,
    operation: &str,
) -> Result<()> {
    let mut cmd = command(paths, repository);
    cmd.arg("--allow-interrupt");
    let mut foreign = McpClient::spawn(&mut cmd).await?;
    for tool in ["turns.get", "turns.interrupt"] {
        let wrong_workspace = foreign
            .request(
                "tools/call",
                json!({"name": tool,
            "arguments": {"workspace": receipt["workspaceId"], "operationId": operation}}),
            )
            .await?;
        assert_eq!(wrong_workspace["isError"], true);
        assert_eq!(
            wrong_workspace["structuredContent"]["error"]["code"],
            "WORKSPACE_NOT_FOUND"
        );
        let wrong_operation = foreign
            .request(
                "tools/call",
                json!({"name": tool,
            "arguments": {"workspace": WORKSPACE_NAME, "operationId": operation}}),
            )
            .await?;
        assert_eq!(wrong_operation["isError"], true);
        assert_eq!(
            wrong_operation["structuredContent"]["error"]["code"],
            "TURN_OPERATION_NOT_FOUND"
        );
    }
    foreign.stop().await
}
