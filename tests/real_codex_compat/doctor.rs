use super::*;

pub(super) async fn verify(
    paths: &TestPaths,
    codex_binary: &Path,
    repository: &Path,
    before: &Value,
) -> Result<()> {
    let output = run_cli(paths, codex_binary, repository, &["doctor", "--json"]).await?;
    let report: Value = serde_json::from_slice(&output.stdout)?;
    ensure!(
        report["complete"] == true,
        "native doctor report was incomplete: {report}"
    );
    ensure!(
        report["daemon"]["codexVersion"].as_str()
            == codex_compat::SELECTED_CODEX_VERSION.strip_prefix("codex-cli "),
        "native initialize did not supply the selected running version: {report}"
    );
    ensure!(report["daemon"]["executionMode"] == "exec-server");
    let checks = report["checks"]
        .as_array()
        .context("native doctor returned no checks")?;
    for id in ["codex.api", "state", "repository", "worktree", "thread"] {
        ensure!(
            checks
                .iter()
                .any(|check| check["id"] == id && check["status"] == "ok"),
            "native doctor failed {id}: {report}"
        );
    }
    let token = fs::read_to_string(&paths.token)?;
    ensure!(
        !String::from_utf8_lossy(&output.stdout).contains(token.trim()),
        "native doctor leaked credentials"
    );
    ensure!(output.stderr.is_empty());
    let after = workspace_status(paths, codex_binary, repository).await?;
    for field in [
        "codexThreadId",
        "phase",
        "availability",
        "lifecycle",
        "worktreePath",
    ] {
        ensure!(
            before["workspace"][field] == after["workspace"][field],
            "doctor changed {field}"
        );
    }
    ensure!(
        before["runtimeResources"]["pid"] == after["runtimeResources"]["pid"],
        "doctor replaced or started an executor"
    );
    Ok(())
}
