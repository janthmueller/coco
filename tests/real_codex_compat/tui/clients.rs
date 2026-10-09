use super::*;
use serde_json::Value;

pub(super) async fn verify(
    paths: &TestPaths,
    binary: &Path,
    repository: &Path,
    expected: usize,
) -> Result<Vec<Value>> {
    let output = super::super::run_cli(
        paths,
        binary,
        repository,
        &["status", FORK_WORKSPACE_NAME, "--json"],
    )
    .await?;
    let status: Value = serde_json::from_slice(&output.stdout)?;
    let clients = status["clients"]
        .as_array()
        .context("native TUI status omitted clients")?;
    ensure!(
        clients.len() == expected,
        "expected {expected} native TUI clients, got: {clients:?}"
    );
    for client in clients {
        ensure!(
            client["metadata"]["kind"] == "native_tui",
            "native TUI client kind missing"
        );
        let integration = &client["metadata"]["integration"];
        ensure!(
            integration["kind"] == "tmux",
            "native tmux discovery did not reach status: {client}"
        );
        ensure!(
            integration["label"] == format!("{SESSION}:0.0"),
            "native tmux label was wrong: {integration}"
        );
        ensure!(
            integration["locator"]
                .as_str()
                .is_some_and(|value| value.starts_with('%')),
            "native pane locator missing"
        );
        ensure!(
            integration["scope"]
                .as_str()
                .is_some_and(|value| value.len() == 64),
            "opaque tmux server scope missing"
        );
        ensure!(
            client.get("leaseId").is_none(),
            "native status exposed a lease capability"
        );
    }
    Ok(clients.clone())
}
