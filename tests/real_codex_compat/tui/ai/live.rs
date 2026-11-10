//! Opted-in, real-model smoke proof. Ordinary compatibility tests stay model-free.

use super::*;
use crate::{RemoteAppServer, select_default_model, verify_runtime_cleanup};
use std::time::Duration;
use tokio::process::Child;

#[path = "live/privacy.rs"]
mod privacy;
#[path = "live/support.rs"]
mod support;
use privacy::{LiveFailure, cleanup};
use support::{Fixture, inventory, run_dialog};

const SEED: &str = "COCO_LIVE_CONTEXT_SEED";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires COCO_RUN_LIVE_AI_PROOF=1, COCO_LIVE_CODEX_HOME and Codex 0.166.0; uses real model tokens"]
async fn authenticated_ai_handles_consent_context_multiple_repositories_and_readonly() -> Result<()>
{
    privacy::proof(authenticated_proof()).await
}

async fn authenticated_proof() -> Result<()> {
    ensure!(
        std::env::var("COCO_RUN_LIVE_AI_PROOF").as_deref() == Ok("1"),
        "real model use requires COCO_RUN_LIVE_AI_PROOF=1"
    );
    ensure_tmux_available().await?;
    let fixture = Fixture::new().await?;
    let mut daemon = fixture.spawn_daemon()?;
    let result = timeout(Duration::from_secs(300), verify(&fixture, &mut daemon)).await;
    let stopped = cleanup(
        stop_daemon(&mut daemon, &fixture.log),
        LiveFailure::Coordinator,
    )
    .await;
    stopped?;
    verify_runtime_cleanup(&fixture.paths)?;
    result.context("live AI smoke exceeded its five-minute deadline; not retried")??;
    eprintln!("Live AI smoke: consent, captured context, both repositories and read-only verified");
    Ok(())
}

async fn verify(fixture: &Fixture, daemon: &mut Child) -> Result<()> {
    wait_for_file(&fixture.paths.socket, daemon, &fixture.log).await?;
    for repo in [&fixture.first, &fixture.second] {
        fixture.cli(repo, &["repo", "add"]).await?;
    }
    let models = fixture
        .cli(&fixture.first, &["model", "list", "--json"])
        .await?;
    let models: Value = serde_json::from_slice(&models.stdout)?;
    let model = select_default_model(&models)?.model;
    ensure!(
        models["models"]
            .as_array()
            .context("model catalog missing")?
            .iter()
            .find(|entry| entry["model"] == model)
            .and_then(|entry| entry["supportedReasoningEfforts"].as_array())
            .is_some_and(|efforts| efforts
                .iter()
                .any(|effort| effort["reasoningEffort"] == "low")),
        "the default model does not advertise the bounded smoke fixture's low reasoning setting"
    );
    eprintln!("Live AI smoke: using advertised model {model}; one seed plus four control prompts");
    let source = seed(fixture, &model).await?;
    eprintln!("Live AI smoke: source conversation prepared");
    let declined = "Erstelle im Repository frontend den Workspace review/declined von main, ohne Gesprächskontext und ohne Änderungen zu kopieren. Sende keine Arbeitsanweisung. Falls ich die Aktion ablehne, versuche sie nicht erneut.";
    run_dialog(fixture, &model, "decline", "", declined, Some(false)).await?;
    ensure!(
        inventory(fixture).await?.len() == 1,
        "declined creation changed the inventory"
    );
    eprintln!("Live AI smoke: declined creation had no effect");
    let context = "Erstelle im Repository frontend den Workspace review/context. Codebasis main, Gesprächskontext aus context/source, keine Komprimierung, keine lokalen Änderungen kopieren. Bereite nur vor und sende danach keine Arbeitsanweisung.";
    run_dialog(fixture, &model, "context", "", context, Some(true)).await?;
    verify_context(fixture, &source).await?;
    eprintln!("Live AI smoke: accepted creation captured the requested context");
    let multiple = "Erstelle in frontend und backend jeweils einen Workspace review/parallel von main, jeweils mit frischem Gesprächskontext. Kopiere keine lokalen Änderungen und sende keine Arbeitsanweisung.";
    run_dialog(fixture, &model, "auto", "--approval auto", multiple, None).await?;
    let before = inventory(fixture).await?;
    ensure!(
        before.len() == 4,
        "multi-repository creation did not produce exactly four total workspaces"
    );
    for repo in [&fixture.first, &fixture.second] {
        let status = fixture.status(repo, "review/parallel").await?;
        ensure!(
            status["workspace"]["name"] == "review/parallel",
            "parallel target is missing"
        );
        ensure!(
            status["workspace"]["codexThreadId"].is_null(),
            "automatic create unexpectedly started agent work"
        );
    }
    eprintln!("Live AI smoke: automatic creation reached both repositories");
    let readonly = "Liste die vorhandenen Workspaces in frontend und backend. Erstelle anschließend in frontend einen Workspace review/forbidden. Falls dir dafür keine Schreibwerkzeuge zur Verfügung stehen, melde das und führe keinen Ersatzbefehl aus.";
    run_dialog(fixture, &model, "readonly", "--read-only", readonly, None).await?;
    ensure!(
        inventory(fixture).await? == before,
        "read-only conversation changed the inventory"
    );
    Ok(())
}

async fn seed(fixture: &Fixture, model: &str) -> Result<Value> {
    fixture
        .cli(&fixture.first, &["create", "context/source", "-m", model])
        .await?;
    let message = format!(
        "Remember the synthetic test marker {SEED}. Reply only with that marker. Do not use tools or modify files."
    );
    let output = fixture
        .cli(
            &fixture.first,
            &[
                "send",
                "context/source",
                &message,
                "--operation-id",
                "live-ai-source-seed",
                "--wait",
            ],
        )
        .await?;
    ensure!(
        String::from_utf8_lossy(&output.stdout).contains(SEED),
        "source turn did not return the synthetic marker"
    );
    fixture.status(&fixture.first, "context/source").await
}

async fn verify_context(fixture: &Fixture, source: &Value) -> Result<()> {
    let child = fixture.status(&fixture.first, "review/context").await?;
    let source_id = source["workspace"]["codexThreadId"]
        .as_str()
        .context("source thread missing")?;
    let child_id = child["workspace"]["codexThreadId"]
        .as_str()
        .context("created context was not captured")?;
    ensure!(
        child_id != source_id,
        "context creation reused the source thread"
    );
    ensure!(
        child["workspace"]["parentThreadId"] == source_id,
        "wrong conversation source"
    );
    ensure!(
        child["workspace"]["branchName"] == "review/context",
        "wrong created branch"
    );
    let mut native = RemoteAppServer::connect_with_capabilities(&fixture.paths, true).await?;
    let history = native
        .request(
            "thread/read",
            json!({"threadId":child_id,"includeTurns":true}),
        )
        .await?;
    ensure!(
        history["thread"]["turns"].to_string().contains(SEED),
        "child did not inherit the synthetic conversation"
    );
    native.close(child_id).await?;
    ensure!(
        inventory(fixture).await?.len() == 2,
        "accepted creation produced extra workspaces"
    );
    Ok(())
}
