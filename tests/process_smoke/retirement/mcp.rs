use super::*;
use crate::mcp_client::McpClient;

fn command(paths: &TestPaths, repository: &Path, grants: bool) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_coco-mcp"));
    paths.apply(&mut command);
    command.arg("--repository").arg(repository);
    if grants {
        command.args(["--allow-close", "--allow-reopen", "--allow-delete"]);
    }
    command
}

pub(super) async fn exercise(paths: &TestPaths, repository: &Path) -> Result<()> {
    let name = "cleanup/mcp";
    run_cli(paths, repository, &["create", name]).await?;
    let mut client = McpClient::spawn(&mut command(paths, repository, true)).await?;
    let preview = client
        .call("workspaces.close", json!({"workspace":name,"dryRun":true}))
        .await?;
    assert_eq!(preview["applied"], false);
    let id = preview["plan"]["workspaceId"].clone();
    let path = PathBuf::from(preview["plan"]["worktreePath"].as_str().unwrap());
    let closed = client
        .call(
            "workspaces.close",
            json!({"workspace":name,"expectedPlanSha256":preview["planSha256"]}),
        )
        .await?;
    assert_eq!(closed["workspace"]["availability"], "closed");
    ensure!(!path.exists());
    let reopened = client
        .call("workspaces.reopen", json!({"workspace":id}))
        .await?;
    assert_eq!(reopened["workspace"]["id"], id);
    ensure!(path.is_dir());
    let input = json!({"workspace":name,"operationId":"process-delete-key"});
    let mut dry_run = input.clone();
    dry_run["dryRun"] = json!(true);
    let preview = client.call("workspaces.delete", dry_run).await?;
    assert_eq!(preview["state"], "preview");
    let deleted = client.call("workspaces.delete", input.clone()).await?;
    assert_eq!(deleted["state"], "deleted");
    assert_eq!(deleted["plan"]["workspaceId"], id);
    ensure!(!path.exists());
    run_cli(paths, repository, &["create", name]).await?;
    let replacement = named_workspace_status(paths, repository, name).await?;
    assert_ne!(replacement["workspace"]["id"], id);
    assert_eq!(client.call("workspaces.delete", input).await?, deleted);
    ensure!(path.is_dir(), "replay deleted a replacement workspace");
    client.stop().await?;
    check_read_only_and_foreign_scope(paths, repository, &replacement["workspace"]["id"], &deleted)
        .await?;
    ensure!(
        path.is_dir(),
        "foreign calls changed the protected workspace"
    );
    run_cli(paths, repository, &["delete", name, "--yes"]).await?;
    Ok(())
}

async fn check_read_only_and_foreign_scope(
    paths: &TestPaths,
    repository: &Path,
    original_id: &Value,
    deleted: &Value,
) -> Result<()> {
    let mut read_only = McpClient::spawn(&mut command(paths, repository, false)).await?;
    assert_eq!(
        read_only
            .call(
                "workspaceDeletions.get",
                json!({"operationId":"process-delete-key"})
            )
            .await?,
        *deleted
    );
    let tools = read_only.request("tools/list", json!({})).await?;
    for tool in ["workspaces.delete", "workspaces.close", "workspaces.reopen"] {
        assert!(
            !tools["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["name"] == tool)
        );
        let mut input = json!({"workspace":original_id});
        if tool == "workspaces.delete" {
            input["operationId"] = json!("forbidden-delete");
        }
        let denied = read_only
            .request("tools/call", json!({"name":tool,"arguments":input}))
            .await;
        ensure!(
            denied
                .as_ref()
                .map_or(true, |result| result["isError"] == true)
        );
    }
    read_only.stop().await?;
    let other = repository.parent().unwrap().join("foreign-repository");
    prepare_repository(&other)?;
    run_cli(paths, &other, &["repo", "add", "."]).await?;
    let mut foreign = McpClient::spawn(&mut command(paths, &other, true)).await?;
    assert_eq!(
        foreign
            .call(
                "workspaceDeletions.get",
                json!({"operationId":"process-delete-key"})
            )
            .await?["state"],
        "unknown"
    );
    let rejected = foreign
        .request(
            "tools/call",
            json!({"name":"workspaces.delete","arguments":{
        "workspace":original_id,"operationId":"foreign-delete"}}),
        )
        .await?;
    assert_eq!(
        rejected["structuredContent"]["error"]["code"],
        "WORKSPACE_NOT_FOUND"
    );
    for tool in ["workspaces.close", "workspaces.reopen"] {
        let rejected = foreign
            .request(
                "tools/call",
                json!({"name":tool,"arguments":{"workspace":original_id}}),
            )
            .await?;
        assert_eq!(
            rejected["structuredContent"]["error"]["code"],
            "WORKSPACE_NOT_FOUND"
        );
    }
    foreign.stop().await
}
