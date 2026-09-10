use std::collections::BTreeMap;

use super::support::*;
use super::*;

pub(super) fn prepare(paths: &TestPaths) -> Result<()> {
    fs::write(
        &paths.hook_handler,
        r#"#!/bin/sh
set -eu
IFS= read -r payload || true
printf '%s\n' "$payload" >> "$1"
"#,
    )?;
    fs::set_permissions(&paths.hook_handler, fs::Permissions::from_mode(0o700))?;
    fs::write(
        &paths.guard_handler,
        r#"#!/bin/sh
set -eu
IFS= read -r payload || true
printf '%s\n' "$payload" >> "$1"
printf '%s' '{"decision":"allow"}'
"#,
    )?;
    fs::set_permissions(&paths.guard_handler, fs::Permissions::from_mode(0o700))?;
    let command = |id: &str, event: &str, signal: Option<&str>| {
        let mut hook = json!({
            "id": id,
            "event": event,
            "command": [paths.hook_handler, paths.hook_capture],
            "timeoutSeconds": 5,
            "maxAttempts": 2,
        });
        if let Some(signal) = signal {
            hook["signal"] = json!(signal);
        }
        hook
    };
    fs::write(
        &paths.hooks,
        serde_json::to_vec(&json!({
            "version": 1,
            "hooks": [
                command("workspace-created", "workspace.created", None),
                command("workspace-closed", "workspace.closed", None),
                command("workspace-reopened", "workspace.reopened", None),
                command("workspace-deleted", "workspace.deleted", None),
                command(
                    "review-requested",
                    "signal.emitted",
                    Some("review.requested@1"),
                ),
            ],
            "guards": [
                {
                    "id": "protect-close",
                    "action": "workspace.close",
                    "command": [paths.guard_handler, paths.guard_capture],
                    "timeoutSeconds": 5,
                    "onError": "deny"
                },
                {
                    "id": "protect-delete",
                    "action": "workspace.delete",
                    "command": [paths.guard_handler, paths.guard_capture],
                    "timeoutSeconds": 5,
                    "onError": "deny"
                }
            ]
        }))?,
    )?;
    fs::set_permissions(&paths.hooks, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

pub(super) async fn verify_loaded(paths: &TestPaths, repository: &Path) -> Result<()> {
    let listed = cli_json(&run_cli(paths, repository, &["hook", "ls", "--json"]).await?)?;
    assert_eq!(listed["schemaVersion"], 7);
    assert_eq!(listed["hooks"].as_array().map(Vec::len), Some(5));
    assert_eq!(listed["guards"].as_array().map(Vec::len), Some(2));
    let human = run_cli(paths, repository, &["hook", "list"]).await?;
    let human = String::from_utf8_lossy(&human.stdout);
    ensure!(
        human.contains("workspace-created")
            && human.contains("protect-delete")
            && human.contains("workspace.delete")
            && human.contains("review.requested@1")
            && !human.contains(paths.hook_handler.to_string_lossy().as_ref())
            && !human.contains('\u{1b}'),
        "hook listing was incomplete or exposed command details: {human}"
    );
    let validated = cli_json(&run_cli(paths, repository, &["hook", "validate", "--json"]).await?)?;
    assert_eq!(validated["hooks"].as_array().map(Vec::len), Some(5));
    assert_eq!(validated["guards"].as_array().map(Vec::len), Some(2));

    let valid = fs::read(&paths.hooks)?;
    fs::write(&paths.hooks, br#"{"version":1,"hooks":"invalid"}"#)?;
    let rejected = capture_cli(paths, repository, &["hook", "reload"], None).await?;
    ensure!(
        !rejected.status.success()
            && String::from_utf8_lossy(&rejected.stderr).contains("HOOK_CONFIG_INVALID"),
        "invalid hook reload was not rejected clearly: {}",
        String::from_utf8_lossy(&rejected.stderr)
    );
    let retained = cli_json(&run_cli(paths, repository, &["hook", "list", "--json"]).await?)?;
    assert_eq!(retained["hooks"].as_array().map(Vec::len), Some(5));
    assert_eq!(retained["guards"].as_array().map(Vec::len), Some(2));

    fs::write(&paths.hooks, valid)?;
    let reloaded = run_cli(paths, repository, &["hook", "reload"]).await?;
    ensure!(
        String::from_utf8_lossy(&reloaded.stdout).contains("Reloaded"),
        "valid hook reload did not confirm the active registry"
    );
    Ok(())
}

pub(super) async fn verify_offline_validation(paths: &TestPaths, repository: &Path) -> Result<()> {
    ensure!(
        !paths.socket.exists(),
        "offline hook validation started with a daemon socket present"
    );
    let validated = cli_json(&run_cli(paths, repository, &["hook", "validate", "--json"]).await?)?;
    assert_eq!(validated["hooks"].as_array().map(Vec::len), Some(5));
    assert_eq!(validated["guards"].as_array().map(Vec::len), Some(2));
    Ok(())
}

pub(super) async fn wait_for_kinds(
    paths: &TestPaths,
    expected: &[(&str, usize)],
) -> Result<Vec<Value>> {
    let deadline = Instant::now() + PROCESS_TIMEOUT;
    loop {
        let events = captured_events(paths)?;
        let counts = events.iter().fold(BTreeMap::new(), |mut counts, event| {
            if let Some(kind) = event.get("kind").and_then(Value::as_str) {
                *counts.entry(kind.to_owned()).or_insert(0_usize) += 1;
            }
            counts
        });
        if expected
            .iter()
            .all(|(kind, count)| counts.get(*kind).copied().unwrap_or(0) >= *count)
        {
            return Ok(events);
        }
        if Instant::now() >= deadline {
            bail!("hooks did not deliver expected events {expected:?}; observed {counts:?}");
        }
        sleep(POLL_INTERVAL).await;
    }
}

pub(super) async fn verify_history(
    paths: &TestPaths,
    repository: &Path,
    minimum: usize,
) -> Result<()> {
    let deadline = Instant::now() + PROCESS_TIMEOUT;
    loop {
        let history = cli_json(
            &run_cli(
                paths,
                repository,
                &["hook", "history", "--limit", "100", "--json"],
            )
            .await?,
        )?;
        let deliveries = history["deliveries"]
            .as_array()
            .context("hook history had no deliveries")?;
        if deliveries.len() >= minimum
            && deliveries
                .iter()
                .take(minimum)
                .all(|delivery| delivery["state"] == "succeeded")
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!("hook history did not settle: {history}");
        }
        sleep(POLL_INTERVAL).await;
    }
}

fn captured_events(paths: &TestPaths) -> Result<Vec<Value>> {
    let body = match fs::read_to_string(&paths.hook_capture) {
        Ok(body) => body,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(source.into()),
    };
    body.lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).context("hook captured invalid JSON"))
        .collect()
}

pub(super) fn captured_guards(paths: &TestPaths) -> Result<Vec<Value>> {
    let body = match fs::read_to_string(&paths.guard_capture) {
        Ok(body) => body,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(source.into()),
    };
    body.lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).context("guard captured invalid JSON"))
        .collect()
}
