use super::*;
use crate::{
    CODEX_BINARY_ENV, daemon_request, prepare_repository, require_explicit_opt_in, run_cli,
    spawn_daemon, stop_daemon, verify_codex_version, wait_for_file,
};
use serde_json::{Value, json};
use std::time::Duration;

#[path = "ai/provider.rs"]
mod provider;

#[path = "ai/live.rs"]
mod live;

#[path = "ai/progress.rs"]
mod progress;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires COCO_RUN_REAL_CODEX_COMPAT=1, Codex 0.166.0 and tmux; scripted loopback, no real model"]
async fn native_ai_creation_requires_human_approval_and_auto_handles_two_repositories() -> Result<()>
{
    require_explicit_opt_in()?;
    ensure_tmux_available().await?;
    let binary = std::env::var_os(CODEX_BINARY_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| "codex".into());
    let temporary = tempfile::tempdir()?;
    let paths = TestPaths::new(temporary.path());
    for directory in [&paths.home, &paths.codex_home, &paths.data_dir] {
        fs::create_dir_all(directory)?;
    }
    verify_codex_version(&binary, &paths).await?;
    let first = temporary.path().join("frontend");
    let second = temporary.path().join("backend");
    prepare_repository(&first)?;
    prepare_repository(&second)?;
    let provider = provider::Provider::start(&paths).await?;
    install_dummy_auth(&paths)?;
    let forbidden_http = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    install_untrusted_project(&paths, &first, forbidden_http.local_addr()?)?;
    trust_projects(&paths, &[&second])?;
    let log = paths.data_dir.join("ai-daemon.log");
    let mut daemon = spawn_daemon(&paths, &binary, &log)?;
    wait_for_file(&paths.socket, &mut daemon, &log).await?;
    run_cli(&paths, &binary, &second, &["repo", "add"]).await?;
    let original_config = fs::read(paths.codex_home.join("config.toml"))?;
    let outcome = verify_sessions(&paths, &binary, &first, &second).await;
    let stopped = stop_daemon(&mut daemon, &log).await;
    if outcome.is_err() && provider.is_finished() {
        let provider_error = provider.finish().await.err();
        return outcome.with_context(|| format!("scripted provider: {provider_error:?}"));
    }
    outcome?;
    stopped?;
    provider.finish().await?;
    let current_config = fs::read(paths.codex_home.join("config.toml"))?;
    verify_configuration(&original_config, &current_config, &first)?;
    ensure!(
        !paths.data_dir.join("inherited-mcp-started").exists(),
        "inherited MCP executable ran"
    );
    ensure!(
        !paths.data_dir.join("project-mcp-started").exists(),
        "project MCP executable ran after folder trust"
    );
    ensure!(
        timeout(Duration::from_millis(20), forbidden_http.accept())
            .await
            .is_err(),
        "project HTTP MCP connected after folder trust"
    );
    crate::verify_runtime_cleanup(&paths)
}

fn install_untrusted_project(
    paths: &TestPaths,
    repository: &Path,
    http_address: std::net::SocketAddr,
) -> Result<()> {
    let directory = repository.join(".codex");
    fs::create_dir_all(&directory)?;
    let exclude = repository.join(".git/info/exclude");
    fs::write(
        &exclude,
        format!("{}\n.codex/\n", fs::read_to_string(&exclude)?),
    )?;
    let marker = paths.data_dir.join("project-mcp-started");
    fs::write(
        directory.join("config.toml"),
        format!(
            "[mcp_servers.project_inherited]\ncommand='/bin/sh'\nrequired=true\nargs=['-c', {}]\n[mcp_servers.project_http]\nurl='http://{http_address}/mcp'\nrequired=true\n",
            toml::Value::String(format!("touch {}; exit 1", shell_word(&marker)))
        ),
    )?;
    Ok(())
}

async fn verify_untrusted_inventory(paths: &TestPaths, repository: &Path) -> Result<()> {
    let inventory = daemon_request(
        paths,
        "controlSession.prepare",
        json!({"repositoryPath":repository}),
    )
    .await?;
    ensure!(
        inventory["mcpServers"].as_array().is_some_and(|servers| [
            "project_inherited",
            "project_http"
        ]
        .iter()
        .all(|name| servers.iter().any(|server| server["name"] == *name))),
        "control preparation omitted the disabled project MCP server"
    );
    Ok(())
}

fn verify_configuration(before: &[u8], after: &[u8], trusted_project: &Path) -> Result<()> {
    let mut before = toml::from_str::<toml::Table>(std::str::from_utf8(before)?)?;
    let mut after = toml::from_str::<toml::Table>(std::str::from_utf8(after)?)?;
    // Native Codex records screen-reader discovery and model introduction
    // counters, plus the one explicit folder-trust choice in this scenario.
    // No control/tool policy, provider or unrelated project setting may leak.
    before.remove("tui");
    if let Some(tui) = after.remove("tui") {
        ensure!(
            tui.as_table()
                .context("native UI preferences are not a table")?
                .keys()
                .all(|key| matches!(
                    key.as_str(),
                    "screen_reader_detection_done" | "model_availability_nux"
                )),
            "unexpected native UI setting changed"
        );
    }
    let trust = after
        .get_mut("projects")
        .and_then(toml::Value::as_table_mut)
        .context("native folder trust was not saved")?
        .remove(trusted_project.canonicalize()?.to_string_lossy().as_ref());
    ensure!(
        trust
            == Some(toml::Value::Table(toml::Table::from_iter([(
                "trust_level".into(),
                toml::Value::String("trusted".into())
            )]))),
        "native folder trust changed more than the explicit trust choice"
    );
    ensure!(
        before == after,
        "AI launch persisted a control configuration override"
    );
    Ok(())
}

async fn verify_sessions(
    paths: &TestPaths,
    binary: &Path,
    first: &Path,
    second: &Path,
) -> Result<()> {
    verify_untrusted_inventory(paths, first).await?;
    let server = TmuxServer::new(paths.data_dir.join("ai-human.tmux.sock"));
    let progress = progress::Progress::snapshot(paths, first)?;
    start_jump(
        &server,
        &command(paths, binary, first, "", "approval-check"),
    )
    .await?;
    trust_folder(&server).await?;
    wait_for(&server, "Create workspace").await?;
    let before = daemon_request(
        paths,
        "workspaceCreation.get",
        json!({
            "repositoryPath": first, "operationId":"ai-approval-check"
        }),
    )
    .await?;
    ensure!(
        before["state"] == "unknown",
        "unapproved creation ran: {before}"
    );
    // The native form renders CoCo's exact action review, including its
    // repository. No daemon mutation is dispatched before human acceptance.
    let screen = server.capture().await?;
    ensure!(
        screen.contains("human-approved") && screen.contains("frontend"),
        "missing concrete approval target: {screen}"
    );
    wait_for(&server, "enter to submit | esc to cancel").await?;
    let visible = server.run(["capture-pane", "-p", "-t", SESSION]).await?;
    let visible = String::from_utf8(visible.stdout)?;
    ensure!(
        visible.contains("Run this action?") && visible.contains("enter to submit | esc to cancel"),
        "native confirmation was not identifiable in the current pane"
    );
    server.send_keys(&["Up"]).await?;
    wait_for(&server, "› 1. True").await?;
    server.send_keys(&["Enter"]).await?;
    wait_for(&server, "AI approval fixture finished").await?;
    ensure!(
        progress.completed()?,
        "native turn completion was not persisted"
    );
    close_tui(&server, "AI approval").await?;
    let created = daemon_request(
        paths,
        "workspaceCreation.get",
        json!({
            "repositoryPath":first, "operationId":"ai-approval-check"
        }),
    )
    .await?;
    ensure!(
        created["state"] == "configured",
        "approved creation did not run: {created}"
    );
    let server = TmuxServer::new(paths.data_dir.join("ai-auto.tmux.sock"));
    let progress = progress::Progress::snapshot(paths, first)?;
    start_jump(
        &server,
        &command(paths, binary, first, "--approval auto", "two-repositories"),
    )
    .await?;
    wait_for(&server, "AI multi-repository fixture finished").await?;
    ensure!(
        progress.completed()?,
        "native multi-repo turn was not complete"
    );
    close_tui(&server, "AI automatic").await?;
    for (repo, operation) in [(first, "ai-auto-frontend"), (second, "ai-auto-backend")] {
        let receipt = daemon_request(
            paths,
            "workspaceCreation.get",
            json!({
                "repositoryPath":repo, "operationId":operation
            }),
        )
        .await?;
        ensure!(
            receipt["state"] == "configured",
            "wrong repository or missing creation: {receipt}"
        );
    }
    verify_readonly(paths, binary, first).await
}

async fn verify_readonly(paths: &TestPaths, binary: &Path, first: &Path) -> Result<()> {
    let server = TmuxServer::new(paths.data_dir.join("ai-readonly.tmux.sock"));
    let progress = progress::Progress::snapshot(paths, first)?;
    start_jump(
        &server,
        &command(paths, binary, first, "--read-only", "inspect-only"),
    )
    .await?;
    wait_for(&server, "AI readonly fixture finished").await?;
    ensure!(
        progress.completed()?,
        "native read-only turn was not complete"
    );
    close_tui(&server, "AI read-only").await
}

async fn trust_folder(server: &TmuxServer) -> Result<()> {
    wait_for(server, "Trust and continue").await?;
    // Native startup deliberately drains pending input after rendering this
    // screen. Acknowledge fresh, idempotent navigation before confirming once.
    for (key, selected) in [("Down", "› 2. Quit"), ("Up", "› 1. Trust and continue")] {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            server.send_keys(&[key]).await?;
            sleep(POLL_INTERVAL).await;
            let pane = server.run(["capture-pane", "-p", "-t", SESSION]).await?;
            if String::from_utf8(pane.stdout)?.contains(selected) {
                break;
            }
            ensure!(
                Instant::now() < deadline,
                "folder trust input was not acknowledged"
            );
        }
    }
    server.send_keys(&["Enter"]).await
}

async fn wait_for(server: &TmuxServer, marker: &str) -> Result<()> {
    let deadline = Instant::now() + COMPATIBILITY_TIMEOUT;
    loop {
        let screen = server.capture().await?;
        if screen.contains(marker) {
            return Ok(());
        }
        ensure!(
            !(marker == "Create workspace" && screen.contains("AI approval fixture finished")),
            "creation finished without its approval: {screen}"
        );
        ensure!(
            !screen.contains("Connection failed:"),
            "isolated AI provider failed: {screen}"
        );
        ensure!(
            exit_status(&screen).is_none(),
            "AI TUI exited before {marker}: {screen}"
        );
        ensure!(
            Instant::now() < deadline,
            "AI TUI did not show {marker}: {screen}"
        );
        sleep(POLL_INTERVAL).await;
    }
}

fn command(
    paths: &TestPaths,
    binary: &Path,
    repository: &Path,
    options: &str,
    prompt: &str,
) -> String {
    let assignments = [
        ("HOME", paths.home.as_path()),
        ("CODEX_HOME", paths.codex_home.as_path()),
        ("COCO_CODEX_BINARY", binary),
        ("COCO_DATA_DIR", paths.data_dir.as_path()),
        ("COCO_SOCKET_PATH", paths.socket.as_path()),
    ]
    .into_iter()
    .map(|(name, value)| format!("{name}={}", shell_word(value)))
    .collect::<Vec<_>>()
    .join(" ");
    format!(
        "cd {} && env TERM=xterm-256color RUST_LOG=warn {assignments} {} ai {options} {}; status=$?; printf '\\n{EXIT_MARKER}%s\\n' \"$status\"",
        shell_word(repository),
        shell_word(Path::new(env!("CARGO_BIN_EXE_coco"))),
        shell_word(Path::new(prompt))
    )
}
