use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

use super::{TestPaths, WORKSPACE_NAME, run_cli, workspace_status};
use std::path::Path;

pub(super) async fn configure_before_runtime_start(
    paths: &TestPaths,
    codex_binary: &Path,
    repository: &Path,
) -> Result<()> {
    let shown = limits_json(paths, codex_binary, repository, &["show", WORKSPACE_NAME]).await?;
    if shown.pointer("/controller/capabilities/backend") != Some(&json!("systemd_cgroup_v2")) {
        return Ok(());
    }

    let configured = limits_json(
        paths,
        codex_binary,
        repository,
        &[
            "set",
            WORKSPACE_NAME,
            "--memory-high",
            "384MiB",
            "--memory-max",
            "768MiB",
            "--cpu-max",
            "1.5",
            "--cpu-weight",
            "321",
            "--tasks-max",
            "128",
        ],
    )
    .await?;
    ensure!(
        configured.pointer("/policy/revision") == Some(&json!(1))
            && configured.pointer("/policy/policy/memoryHighBytes")
                == Some(&json!(402_653_184_u64))
            && configured.pointer("/policy/policy/cpuMaxMillicores") == Some(&json!(1_500))
            && configured.pointer("/controller/runtimeState") == Some(&json!("inactive"))
            && configured.pointer("/controller/appliedPolicy").is_none(),
        "prepared workspace did not retain the desired resource policy: {configured}"
    );
    Ok(())
}

pub(super) async fn update_running_policy(
    paths: &TestPaths,
    codex_binary: &Path,
    repository: &Path,
) -> Result<()> {
    let current = limits_json(paths, codex_binary, repository, &["show", WORKSPACE_NAME]).await?;
    if current.pointer("/policy/revision") == Some(&json!(0)) {
        return Ok(());
    }
    ensure_applied(&current, 1)?;

    let updated = limits_json(
        paths,
        codex_binary,
        repository,
        &[
            "set",
            WORKSPACE_NAME,
            "--memory-high",
            "256MiB",
            "--cpu-max",
            "0.75",
            "--cpu-weight",
            "456",
            "--tasks-max",
            "96",
        ],
    )
    .await?;
    ensure_applied(&updated, 2)?;
    ensure!(
        updated.pointer("/policy/policy/memoryMaxBytes") == Some(&json!(805_306_368_u64))
            && updated.pointer("/policy/policy/cpuMaxMillicores") == Some(&json!(750)),
        "partial live policy update lost an unchanged field: {updated}"
    );
    Ok(())
}

pub(super) async fn verify_restart_and_reset(
    paths: &TestPaths,
    codex_binary: &Path,
    repository: &Path,
) -> Result<()> {
    let current = limits_json(paths, codex_binary, repository, &["show", WORKSPACE_NAME]).await?;
    if current.pointer("/policy/revision") == Some(&json!(0)) {
        return Ok(());
    }
    ensure_applied(&current, 2)?;
    let process_before = workspace_status(paths, codex_binary, repository)
        .await?
        .pointer("/runtimeResources/processId")
        .cloned();

    let reset = limits_json(paths, codex_binary, repository, &["reset", WORKSPACE_NAME]).await?;
    ensure!(
        reset.pointer("/policy/revision") == Some(&json!(3))
            && reset
                .pointer("/policy/policy")
                .and_then(Value::as_object)
                .is_some_and(|policy| policy.len() == 1 && policy["schemaVersion"] == 1)
            && reset.pointer("/controller/runtimeState") == Some(&json!("running"))
            && reset.pointer("/controller/appliedPolicy/revision") == Some(&json!(2))
            && reset.pointer("/controller/appliedPolicy/policy/cpuMaxMillicores")
                == Some(&json!(750)),
        "active CPU-cap reset was not staged for the next runtime: {reset}"
    );
    let process_after = workspace_status(paths, codex_binary, repository)
        .await?
        .pointer("/runtimeResources/processId")
        .cloned();
    ensure!(
        process_before.is_some() && process_before == process_after,
        "reset implicitly replaced the running workspace executor"
    );
    Ok(())
}

async fn limits_json(
    paths: &TestPaths,
    codex_binary: &Path,
    repository: &Path,
    arguments: &[&str],
) -> Result<Value> {
    let mut command = vec!["limits"];
    command.extend_from_slice(arguments);
    command.push("--json");
    let output = run_cli(paths, codex_binary, repository, &command).await?;
    serde_json::from_slice(&output.stdout).context("coco limits did not return JSON")
}

fn ensure_applied(result: &Value, revision: u64) -> Result<()> {
    ensure!(
        result.pointer("/policy/revision") == Some(&json!(revision))
            && result.pointer("/controller/runtimeState") == Some(&json!("running"))
            && result.pointer("/controller/appliedPolicy/revision") == Some(&json!(revision)),
        "workspace resource policy revision {revision} was not applied: {result}"
    );
    Ok(())
}
