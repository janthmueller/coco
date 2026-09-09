use super::*;
use tokio::io::{AsyncBufReadExt, BufReader};

pub(super) async fn verify(
    paths: &TestPaths,
    repository: &Path,
    writer: &mut McpClient,
    initial_signals: &Value,
) -> Result<()> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_coco"));
    paths.apply(&mut command);
    command
        .args(["signal", "list", "-f", "--json"])
        .current_dir(repository)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().context("could not start signal follower")?;
    let mut lines = BufReader::new(child.stdout.take().context("missing stdout")?).lines();
    let initial: Value = serde_json::from_str(
        &timeout(PROCESS_TIMEOUT, lines.next_line())
            .await??
            .context("no initial page")?,
    )?;
    assert_eq!(initial["signals"], *initial_signals);

    // Wait past a polling interval: no unchanged pages and no early exit.
    ensure!(
        timeout(Duration::from_millis(1100), lines.next_line())
            .await
            .is_err(),
        "idle follower repeated a page or closed stdout"
    );
    ensure!(child.try_wait()?.is_none(), "idle follower stopped");
    let emitted = writer
        .request(
            "tools/call",
            emission(json!({"pr": 13}), "review-13", Some(SECOND_THREAD)),
        )
        .await?;
    ensure!(
        emitted["isError"] != true,
        "follow emission failed: {emitted}"
    );
    let update: Value = serde_json::from_str(
        &timeout(PROCESS_TIMEOUT, lines.next_line())
            .await??
            .context("no subsequent page")?,
    )?;
    assert_eq!(update["signals"], json!([emitted["structuredContent"]]));
    assert_ne!(update["nextCursor"], initial["nextCursor"]);
    ensure!(
        timeout(Duration::from_millis(1100), lines.next_line())
            .await
            .is_err(),
        "follower repeated an update or stopped after it"
    );
    ensure!(child.try_wait()?.is_none(), "follower stopped after update");
    interrupt(&child).await?;
    let output = timeout(PROCESS_TIMEOUT, child.wait_with_output()).await??;
    ensure!(
        output.status.success(),
        "signal follow failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(timeout(PROCESS_TIMEOUT, lines.next_line()).await??, None);
    Ok(())
}
