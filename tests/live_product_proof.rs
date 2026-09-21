#![cfg(unix)]

use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

#[path = "support/mcp_client.rs"]
mod mcp_client;
#[path = "live_product_proof/support.rs"]
mod support;

use support::*;

const OPT_IN_ENV: &str = "COCO_RUN_LIVE_PRODUCT_PROOF";
const AUTH_HOME_ENV: &str = "COCO_LIVE_CODEX_HOME";
const MESSAGE_ENV: &str = "COCO_LIVE_PRODUCT_PROOF_MESSAGE";
const CODEX_BINARY_ENV: &str = "COCO_REAL_CODEX_BINARY";
const FIRST_WORKSPACE: &str = "proof/cli";
const SECOND_WORKSPACE: &str = "proof/mcp";
const FIRST_OPERATION: &str = "live-proof-cli-first";
const SECOND_OPERATION: &str = "live-proof-cli-second";
const MCP_OPERATION: &str = "live-proof-mcp-continuation";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires explicit opt-in, local Codex authentication, and live model access"]
#[expect(
    clippy::too_many_lines,
    reason = "one supervised acceptance scenario keeps its cross-client and restart assertions ordered"
)]
async fn authenticated_cli_mcp_and_restart_keep_two_workspaces_exact() -> Result<()> {
    ensure!(
        env::var(OPT_IN_ENV).as_deref() == Ok("1"),
        "set {OPT_IN_ENV}=1 in addition to passing --ignored"
    );
    let source_codex_home = env::var_os(AUTH_HOME_ENV)
        .map(PathBuf::from)
        .context("set COCO_LIVE_CODEX_HOME to a Codex home containing auth.json")?;
    let message = env::var(MESSAGE_ENV)
        .context("set COCO_LIVE_PRODUCT_PROOF_MESSAGE to a bounded test instruction")?;
    ensure!(
        !message.trim().is_empty(),
        "the live proof message is empty"
    );
    let codex_binary = env::var_os(CODEX_BINARY_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("codex"));

    let temporary = tempfile::tempdir()?;
    let paths = TestPaths::new(temporary.path());
    prepare_private_home(&paths, &source_codex_home)?;
    verify_codex_version(&paths, &codex_binary).await?;

    let first_repository = temporary.path().join("first-repository");
    let second_repository = temporary.path().join("second-repository");
    prepare_repository(&first_repository)?;
    prepare_repository(&second_repository)?;

    let first_log = paths.data_dir.join("cocod-first.log");
    let mut daemon = spawn_daemon(&paths, &codex_binary, &first_log)?;
    wait_for_daemon(&paths, &mut daemon, &first_log).await?;

    for repository in [&first_repository, &second_repository] {
        run_cli(&paths, &codex_binary, repository, &["repo", "add", "."]).await?;
    }
    run_cli(
        &paths,
        &codex_binary,
        &first_repository,
        &["create", FIRST_WORKSPACE],
    )
    .await?;
    run_cli(
        &paths,
        &codex_binary,
        &second_repository,
        &["create", SECOND_WORKSPACE],
    )
    .await?;

    run_cli(
        &paths,
        &codex_binary,
        &first_repository,
        &[
            "send",
            FIRST_WORKSPACE,
            &message,
            "--operation-id",
            FIRST_OPERATION,
        ],
    )
    .await?;
    run_cli(
        &paths,
        &codex_binary,
        &second_repository,
        &[
            "send",
            SECOND_WORKSPACE,
            &message,
            "--operation-id",
            SECOND_OPERATION,
        ],
    )
    .await?;

    let first_before =
        wait_for_idle(&paths, &codex_binary, &first_repository, FIRST_WORKSPACE).await?;
    let second_before =
        wait_for_idle(&paths, &codex_binary, &second_repository, SECOND_WORKSPACE).await?;
    assert_bound_workspace(&first_before, FIRST_WORKSPACE)?;
    assert_bound_workspace(&second_before, SECOND_WORKSPACE)?;

    let mut writer = spawn_mcp_writer(&paths, &codex_binary, &second_repository).await?;
    let listed = writer.call("workspaces.list", json!({})).await?;
    ensure!(
        listed.as_array().is_some_and(|workspaces| {
            workspaces.len() == 1 && workspaces[0]["name"] == SECOND_WORKSPACE
        }),
        "repository-scoped MCP did not expose exactly its own workspace"
    );
    let input = json!({
        "workspace": SECOND_WORKSPACE,
        "message": &message,
        "operationId": MCP_OPERATION,
    });
    let accepted = writer.call("workspaces.send", input.clone()).await?;
    let repeated = writer.call("workspaces.send", input).await?;
    assert_same_turn(&accepted, &repeated)?;
    writer.stop().await?;
    wait_for_idle(&paths, &codex_binary, &second_repository, SECOND_WORKSPACE).await?;

    stop_daemon(&mut daemon, &first_log).await?;
    let second_log = paths.data_dir.join("cocod-second.log");
    let mut daemon = spawn_daemon(&paths, &codex_binary, &second_log)?;
    wait_for_daemon(&paths, &mut daemon, &second_log).await?;

    let first_after =
        workspace_status(&paths, &codex_binary, &first_repository, FIRST_WORKSPACE).await?;
    let second_after =
        workspace_status(&paths, &codex_binary, &second_repository, SECOND_WORKSPACE).await?;
    assert_same_binding(&first_before, &first_after)?;
    assert_same_binding(&second_before, &second_after)?;

    let mut writer = spawn_mcp_writer(&paths, &codex_binary, &second_repository).await?;
    let replayed = writer
        .call(
            "workspaces.send",
            json!({
                "workspace": SECOND_WORKSPACE,
                "message": &message,
                "operationId": MCP_OPERATION,
            }),
        )
        .await?;
    assert_same_turn(&accepted, &replayed)?;
    writer.stop().await?;

    let all = run_cli(
        &paths,
        &codex_binary,
        temporary.path(),
        &["list", "--all-repos", "--json"],
    )
    .await?;
    let all: Value = serde_json::from_slice(&all.stdout)?;
    ensure!(
        all["workspaces"].as_array().map(Vec::len) == Some(2),
        "restart or replay changed the two-workspace inventory"
    );

    stop_daemon(&mut daemon, &second_log).await?;
    Ok(())
}
