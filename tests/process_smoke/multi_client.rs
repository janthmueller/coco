use super::app_server::{fake_thread_read, idle_thread_response, send_json, send_result};
use super::support::*;
use super::*;
use crate::mcp_client::McpClient;

pub(super) const SECOND_THREAD: &str = "thread-second-repository";
const FIRST_SEND: &str = "second-repo-first-send";
const MCP_SEND: &str = "second-repo-mcp-send";

pub(super) async fn exercise(paths: &TestPaths, root: &Path) -> Result<PathBuf> {
    let repository = root.join("second-repository");
    prepare_repository(&repository)?;
    run_cli(paths, &repository, &["repo", "add", "."]).await?;
    run_cli(paths, &repository, &["create", WORKSPACE_NAME]).await?;
    run_cli(
        paths,
        &repository,
        &[
            "send",
            WORKSPACE_NAME,
            "First message",
            "--operation-id",
            FIRST_SEND,
        ],
    )
    .await?;
    let workspace = wait_for_workspace_phase(paths, &repository, "idle").await?;
    assert_eq!(workspace["workspace"]["codexThreadId"], SECOND_THREAD);
    let worktree = PathBuf::from(
        workspace["workspace"]["worktreePath"]
            .as_str()
            .context("second worktree missing")?,
    );

    let list = cli_json(&run_cli(paths, root, &["list", "-a", "--json"]).await?)?;
    assert_eq!(list["workspaces"].as_array().map(Vec::len), Some(2));
    let ambiguous = capture_cli(paths, root, &["status", WORKSPACE_NAME, "-g"], None).await?;
    ensure!(
        !ambiguous.status.success(),
        "global duplicate name unexpectedly resolved"
    );
    ensure!(
        String::from_utf8_lossy(&ambiguous.stderr).contains("AMBIGUOUS"),
        "duplicate name did not explain ambiguity"
    );

    let mut command = Command::new(env!("CARGO_BIN_EXE_coco-mcp"));
    paths.apply(&mut command);
    command.args([
        "--repository",
        repository
            .to_str()
            .context("repository path was not UTF-8")?,
    ]);
    let mut read_only = McpClient::spawn(&mut command).await?;
    let tools = read_only.request("tools/list", json!({})).await?;
    ensure!(
        !tools["tools"]
            .as_array()
            .context("MCP tools missing")?
            .iter()
            .any(|tool| tool["name"] == "workspaces.send")
    );
    let listed = read_only.call("workspaces.list", json!({})).await?;
    assert_eq!(listed.as_array().map(Vec::len), Some(1));
    assert_eq!(listed[0]["id"], workspace["workspace"]["id"]);
    let denied = read_only
        .request(
            "tools/call",
            json!({
                "name": "workspaces.send",
                "arguments": {"workspace": WORKSPACE_NAME, "message": "Must not be sent"}
            }),
        )
        .await;
    ensure!(
        denied
            .as_ref()
            .map_or(true, |value| value["isError"] == true),
        "read-only MCP accepted a mutating call"
    );
    read_only.stop().await?;

    command.arg("--allow-send");
    let mut writer = McpClient::spawn(&mut command).await?;
    let input = json!({"workspace": WORKSPACE_NAME, "message": "Continue through MCP", "operationId": MCP_SEND});
    let accepted = writer.call("workspaces.send", input.clone()).await?;
    let repeated = writer.call("workspaces.send", input).await?;
    assert_eq!(accepted["turnId"], repeated["turnId"]);
    assert_eq!(accepted["codexTurnId"], repeated["codexTurnId"]);
    assert_eq!(accepted["workspace"]["id"], repeated["workspace"]["id"]);
    writer.stop().await?;
    wait_for_workspace_phase(paths, &repository, "idle").await?;
    super::signals::exercise(paths, &repository).await?;
    Ok(worktree)
}

pub(super) async fn verify_replay(paths: &TestPaths, root: &Path) -> Result<()> {
    let repository = root.join("second-repository");
    let before = workspace_status(paths, &repository).await?;
    assert_eq!(before["workspace"]["codexThreadId"], SECOND_THREAD);
    assert_eq!(before["workspace"]["phase"], "not_loaded");
    let mut command = Command::new(env!("CARGO_BIN_EXE_coco-mcp"));
    paths.apply(&mut command);
    command
        .arg("--repository")
        .arg(&repository)
        .arg("--allow-send");
    let mut client = McpClient::spawn(&mut command).await?;
    let replayed = client.call("workspaces.send", json!({"workspace": WORKSPACE_NAME, "message": "Continue through MCP", "operationId": MCP_SEND})).await?;
    assert_eq!(replayed["codexTurnId"], "second-turn-2");
    client.stop().await?;
    let after = workspace_status(paths, &repository).await?;
    assert_eq!(
        after["workspace"]["phase"], "not_loaded",
        "retry loaded a thread unnecessarily"
    );
    let list = cli_json(&run_cli(paths, root, &["list", "-a", "--json"]).await?)?;
    assert_eq!(list["workspaces"].as_array().map(Vec::len), Some(2));
    super::signals::verify_replay(paths, &repository).await?;
    Ok(())
}

#[derive(Default)]
pub(super) struct SecondThread {
    cwd: Option<Value>,
    turns: usize,
}

impl SecondThread {
    pub(super) async fn handle(
        &mut self,
        socket: &mut WebSocketStream<TcpStream>,
        frame: &Value,
    ) -> Result<bool> {
        match frame["method"].as_str() {
            Some("thread/start") => {
                ensure!(
                    self.cwd.is_none(),
                    "second workspace created another thread"
                );
                self.cwd = frame.pointer("/params/cwd").cloned();
                send_result(
                    socket,
                    frame,
                    idle_thread_response(frame, SECOND_THREAD, DEFAULT_MODEL),
                )
                .await?;
            }
            Some("thread/read")
                if frame.pointer("/params/threadId") == Some(&json!(SECOND_THREAD)) =>
            {
                send_result(
                    socket,
                    frame,
                    fake_thread_read(
                        frame,
                        SECOND_THREAD,
                        WORKSPACE_NAME,
                        self.cwd.as_ref().context("second thread missing")?,
                        json!({"type": "idle"}),
                        &[],
                        None,
                    ),
                )
                .await?;
            }
            Some("turn/start")
                if frame.pointer("/params/threadId") == Some(&json!(SECOND_THREAD)) =>
            {
                self.turns += 1;
                ensure!(
                    self.turns <= 2,
                    "an idempotent retry started a duplicate native turn"
                );
                let turn_id = format!("second-turn-{}", self.turns);
                send_result(socket, frame, json!({"turn": {"id": turn_id}})).await?;
                send_json(socket, json!({"method": "turn/completed", "params": {"threadId": SECOND_THREAD, "turn": {"id": turn_id, "status": "completed"}}})).await?;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}

pub(super) fn recovery_read(frame: &Value, cwd: &Path) -> Option<Value> {
    (frame["method"] == "thread/read"
        && frame.pointer("/params/threadId") == Some(&json!(SECOND_THREAD)))
    .then(|| {
        fake_thread_read(
            frame,
            SECOND_THREAD,
            WORKSPACE_NAME,
            &json!(cwd),
            json!({"type": "notLoaded"}),
            &[],
            None,
        )
    })
}
