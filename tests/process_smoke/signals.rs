use super::multi_client::SECOND_THREAD;
use super::support::*;
use super::*;
use crate::mcp_client::McpClient;

const NAME: &str = "review.requested";

#[path = "signals/catalog.rs"]
mod catalog;
#[path = "signals/follow.rs"]
mod follow;
#[path = "signals/scoping.rs"]
mod scoping;

fn catalog_dir(paths: &TestPaths) -> PathBuf {
    paths.data_dir.join("signal-catalog")
}

fn catalog_command(paths: &TestPaths, repository: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_coco-mcp"));
    paths.apply(&mut command);
    command
        .arg("--repository")
        .arg(repository)
        .arg("--signal-catalog")
        .arg(catalog_dir(paths));
    command
}

async fn client(paths: &TestPaths, repository: &Path, grant: bool) -> Result<McpClient> {
    let mut command = catalog_command(paths, repository);
    if grant {
        command.args(["--allow-emit", NAME]);
    }
    McpClient::spawn(&mut command).await
}

fn emission(payload: Value, key: &str, thread: Option<&str>) -> Value {
    let mut request = json!({"name": "signals.emit", "arguments": {"name": NAME, "version": 1, "payload": payload, "idempotencyKey": key}});
    if let Some(thread) = thread {
        request["_meta"] = json!({"threadId": thread});
    }
    request
}

pub(super) async fn exercise(paths: &TestPaths, repository: &Path) -> Result<()> {
    catalog::verify(paths, repository).await?;
    let writer = verify_authority(paths, repository).await?;
    verify_delivery(paths, repository, writer).await
}

async fn verify_authority(paths: &TestPaths, repository: &Path) -> Result<McpClient> {
    let mut read_only = client(paths, repository, false).await?;
    let tools = read_only.request("tools/list", json!({})).await?;
    ensure!(
        !tools["tools"]
            .as_array()
            .context("missing tools")?
            .iter()
            .any(|tool| tool["name"] == "signals.emit")
    );
    let denied = read_only
        .request(
            "tools/call",
            emission(json!({"pr": 12}), "forbidden", Some(SECOND_THREAD)),
        )
        .await;
    ensure!(
        denied
            .as_ref()
            .map_or(true, |result| result["isError"] == true)
    );
    read_only.stop().await?;

    let mut writer = client(paths, repository, true).await?;
    let types = writer.call("signals.types", json!({})).await?;
    assert_eq!(types.as_array().map(Vec::len), Some(2));
    assert_eq!(types[0]["emitAllowed"], true);
    assert_eq!(types[1]["emitAllowed"], false);
    for (request, code) in [
        (
            emission(json!({"pr": 12}), "unbound", None),
            "SIGNAL_SENDER_INVALID",
        ),
        (
            emission(json!({"pr": 12}), "wrong-repo", Some(THREAD_ID)),
            "SIGNAL_SENDER_INVALID",
        ),
        (
            emission(json!({"pr": "wrong"}), "review-12", Some(SECOND_THREAD)),
            "SIGNAL_PAYLOAD_INVALID",
        ),
    ] {
        let denied = writer.request("tools/call", request).await?;
        ensure!(
            denied["isError"] == true,
            "invalid emission accepted: {denied}"
        );
        assert_eq!(denied["structuredContent"]["error"]["code"], code);
        if code == "SIGNAL_PAYLOAD_INVALID" {
            let message = denied["structuredContent"]["error"]["message"]
                .as_str()
                .context("validation message missing")?;
            ensure!(message.contains("/pr") && message.contains("/properties/pr/type"));
            ensure!(
                !message.contains("wrong"),
                "payload value leaked into error"
            );
        }
    }
    let mut denied = emission(json!({"pr": 12}), "ungranted", Some(SECOND_THREAD));
    denied["arguments"]["name"] = json!("merge.requested");
    let result = writer.request("tools/call", denied).await?;
    assert_eq!(
        result["structuredContent"]["error"]["code"],
        "SIGNAL_NOT_GRANTED"
    );
    let mut denied = emission(json!({"pr": 12}), "new-version", Some(SECOND_THREAD));
    denied["arguments"]["version"] = json!(2);
    let result = writer.request("tools/call", denied).await?;
    assert_eq!(
        result["structuredContent"]["error"]["code"], "SIGNAL_NOT_GRANTED",
        "registering version 2 must not expand a bare-name version-1 grant"
    );
    Ok(writer)
}

async fn verify_delivery(
    paths: &TestPaths,
    repository: &Path,
    mut writer: McpClient,
) -> Result<()> {
    let empty = writer.call("signals.list", json!({})).await?;
    assert_eq!(empty["signals"], json!([]));
    let cursor = empty["nextCursor"]
        .as_str()
        .context("empty cursor missing")?;
    fs::write(paths.data_dir.join("signal-cursor"), cursor)?;
    let input = emission(json!({"pr": 12}), "review-12", Some(SECOND_THREAD));
    let first = writer.request("tools/call", input.clone()).await?;
    ensure!(first["isError"] != true, "valid signal rejected: {first}");
    let repeated = writer.request("tools/call", input).await?;
    assert_eq!(first["structuredContent"], repeated["structuredContent"]);
    fs::write(
        paths.data_dir.join("signal-record.json"),
        serde_json::to_vec(&first["structuredContent"])?,
    )?;
    let page = writer
        .call("signals.list", json!({"after": cursor, "limit": 1}))
        .await?;
    assert_eq!(page["signals"], json!([first["structuredContent"]]));
    assert_eq!(
        page,
        writer
            .call("signals.list", json!({"after": cursor, "limit": 1}))
            .await?
    );
    let cli_page = cli_json(
        &run_cli(
            paths,
            repository,
            &["signal", "ls", "--after", cursor, "--json"],
        )
        .await?,
    )?;
    assert_eq!(cli_page["signals"], page["signals"]);
    scoping::verify(paths, repository, &page["signals"]).await?;
    follow::verify(paths, repository, &mut writer, &page["signals"]).await?;
    writer.stop().await?;
    Ok(())
}

pub(super) async fn verify_replay(paths: &TestPaths, repository: &Path) -> Result<()> {
    let cursor = fs::read_to_string(paths.data_dir.join("signal-cursor"))?;
    let original: Value =
        serde_json::from_slice(&fs::read(paths.data_dir.join("signal-record.json"))?)?;
    let mut client = client(paths, repository, true).await?;
    let page = client
        .call("signals.list", json!({"after": cursor}))
        .await?;
    assert_eq!(page["signals"].as_array().map(Vec::len), Some(2));
    assert_eq!(page["signals"][0], original);
    assert_eq!(page["signals"][1]["payload"], json!({"pr": 13}));
    let repeated = client
        .request(
            "tools/call",
            emission(json!({"pr": 12}), "review-12", Some(SECOND_THREAD)),
        )
        .await?;
    assert_eq!(repeated["structuredContent"], original);
    client.stop().await?;
    Ok(())
}
