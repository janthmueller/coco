use std::env;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::sync::watch;
use tokio::task::JoinSet;
use tokio::time::{sleep, timeout};
use tracing::{error, info, warn};

use super::{GuardDefinition, HookDefinition, HookRegistry};
use crate::domain::hooks::{ClaimedHookDelivery, MAX_GUARD_OUTPUT_BYTES, MAX_GUARD_REASON_CHARS};
use crate::store::Store;

const MAX_CONCURRENT_HOOKS: usize = 4;
const IDLE_POLL: Duration = Duration::from_secs(1);

pub(crate) async fn run_dispatcher(
    store: Arc<Store>,
    registry: Arc<HookRegistry>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut tasks = JoinSet::new();
    loop {
        if *shutdown.borrow() {
            break;
        }
        while tasks.len() < MAX_CONCURRENT_HOOKS {
            match store.claim_hook_delivery(chrono::Utc::now().timestamp_millis()) {
                Ok(Some(delivery)) => {
                    let store = Arc::clone(&store);
                    let registry = Arc::clone(&registry);
                    tasks.spawn(async move { deliver(store, registry, delivery).await });
                }
                Ok(None) => break,
                Err(source) => {
                    error!(%source, "could not claim a pending hook delivery");
                    break;
                }
            }
        }

        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    break;
                }
            }
            joined = tasks.join_next(), if !tasks.is_empty() => {
                if let Some(Err(source)) = joined {
                    error!(%source, "hook delivery task panicked");
                }
            }
            () = registry.notified() => {}
            () = sleep(IDLE_POLL) => {}
        }
    }

    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
}

async fn deliver(store: Arc<Store>, registry: Arc<HookRegistry>, delivery: ClaimedHookDelivery) {
    let id = delivery.summary.id.clone();
    let hook_id = delivery.summary.hook_id.clone();
    let event_id = delivery.summary.event_id.clone();
    let Some((definition, working_directory)) =
        registry.definition(&hook_id, &delivery.definition_hash)
    else {
        if let Err(source) = store.cancel_hook_delivery(
            &id,
            "hook definition changed or was removed before delivery",
        ) {
            error!(%source, delivery_id = %id, "could not cancel stale hook delivery");
        }
        return;
    };

    let outcome = run_hook_command(&definition, &working_directory, &delivery.event_body).await;
    let error = outcome.as_ref().err().map(String::as_str);
    let recorded = store.complete_hook_delivery(&id, definition.max_attempts(), error);
    match (recorded, outcome) {
        (Ok(_), Ok(())) => {
            info!(hook_id, event_id, delivery_id = %id, "hook delivery succeeded");
        }
        (Ok(result), Err(reason))
            if result.state == crate::domain::hooks::HookDeliveryState::Pending =>
        {
            warn!(
                hook_id,
                event_id,
                delivery_id = %id,
                attempt = result.attempts,
                %reason,
                "hook delivery will retry"
            );
        }
        (Ok(_), Err(reason)) => {
            error!(
                hook_id,
                event_id,
                delivery_id = %id,
                %reason,
                "hook delivery exhausted its attempts"
            );
        }
        (Err(source), _) => {
            error!(%source, delivery_id = %id, "could not record hook delivery outcome");
        }
    }
}

async fn run_hook_command(
    definition: &HookDefinition,
    working_directory: &std::path::Path,
    event: &[u8],
) -> Result<(), String> {
    let (executable, arguments) = definition
        .command()
        .split_first()
        .expect("validated hook command is non-empty");
    let mut command = Command::new(executable);
    command
        .args(arguments)
        .current_dir(working_directory)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    if let Some(path) = env::var_os("PATH") {
        command.env("PATH", path);
    }
    let mut child = command
        .spawn()
        .map_err(|source| format!("could not start hook command: {source}"))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "hook command did not expose stdin".to_owned())?;
    let duration = Duration::from_secs(definition.timeout_seconds());
    let execution = async {
        stdin
            .write_all(event)
            .await
            .map_err(|source| format!("could not write hook event to stdin: {source}"))?;
        stdin
            .shutdown()
            .await
            .map_err(|source| format!("could not close hook stdin: {source}"))?;
        drop(stdin);
        match child.wait().await {
            Ok(status) if status.success() => Ok(()),
            Ok(status) => Err(format!("hook command exited with {status}")),
            Err(source) => Err(format!("could not wait for hook command: {source}")),
        }
    };
    match timeout(duration, execution).await {
        Ok(outcome) => outcome,
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            Err(format!(
                "hook command timed out after {} seconds",
                definition.timeout_seconds()
            ))
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum GuardDecision {
    Allow,
    Deny(String),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum GuardDecisionValue {
    Allow,
    Deny,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GuardOutput {
    decision: GuardDecisionValue,
    #[serde(default)]
    reason: Option<String>,
}

pub(super) async fn run_guard(
    definition: &GuardDefinition,
    working_directory: &std::path::Path,
    request: &[u8],
) -> Result<GuardDecision, String> {
    let (executable, arguments) = definition
        .command()
        .split_first()
        .expect("validated guard command is non-empty");
    let mut command = Command::new(executable);
    command
        .args(arguments)
        .current_dir(working_directory)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    if let Some(path) = env::var_os("PATH") {
        command.env("PATH", path);
    }
    let mut child = command
        .spawn()
        .map_err(|source| format!("could not start guard command: {source}"))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "guard command did not expose stdin".to_owned())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "guard command did not expose stdout".to_owned())?;
    let duration = Duration::from_secs(definition.timeout_seconds());
    let execution = async {
        let write = async move {
            stdin
                .write_all(request)
                .await
                .map_err(|source| format!("could not write guard request to stdin: {source}"))?;
            let result = stdin
                .shutdown()
                .await
                .map_err(|source| format!("could not close guard stdin: {source}"));
            drop(stdin);
            result
        };
        let read = async {
            let mut output = Vec::new();
            stdout
                .take((MAX_GUARD_OUTPUT_BYTES + 1) as u64)
                .read_to_end(&mut output)
                .await
                .map_err(|source| format!("could not read guard output: {source}"))?;
            Ok::<_, String>(output)
        };
        let (written, output) = tokio::join!(write, read);
        written?;
        let output = output?;
        if output.len() > MAX_GUARD_OUTPUT_BYTES {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err("guard output exceeded 8 KiB".to_owned());
        }
        match child.wait().await {
            Ok(status) if status.success() => parse_guard_output(&output),
            Ok(status) => Err(format!("guard command exited with {status}")),
            Err(source) => Err(format!("could not wait for guard command: {source}")),
        }
    };
    match timeout(duration, execution).await {
        Ok(outcome) => outcome,
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            Err(format!(
                "guard command timed out after {} seconds",
                definition.timeout_seconds()
            ))
        }
    }
}

fn parse_guard_output(output: &[u8]) -> Result<GuardDecision, String> {
    let parsed = serde_json::from_slice::<GuardOutput>(output)
        .map_err(|source| format!("guard returned invalid JSON: {source}"))?;
    match parsed.decision {
        GuardDecisionValue::Allow if parsed.reason.is_none() => Ok(GuardDecision::Allow),
        GuardDecisionValue::Allow => {
            Err("guard allow decision must not include a reason".to_owned())
        }
        GuardDecisionValue::Deny => {
            let reason = parsed
                .reason
                .as_deref()
                .map(sanitize_guard_reason)
                .filter(|reason| !reason.is_empty())
                .ok_or_else(|| "guard deny decision requires a non-empty reason".to_owned())?;
            Ok(GuardDecision::Deny(reason))
        }
    }
}

fn sanitize_guard_reason(reason: &str) -> String {
    let mut sanitized = reason
        .trim()
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .take(MAX_GUARD_REASON_CHARS + 1)
        .collect::<String>();
    if sanitized.chars().count() > MAX_GUARD_REASON_CHARS {
        sanitized = sanitized
            .chars()
            .take(MAX_GUARD_REASON_CHARS.saturating_sub(1))
            .collect();
        sanitized.push('…');
    }
    sanitized.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use super::*;

    fn load_definition(
        directory: &std::path::Path,
        command: Vec<String>,
        timeout_seconds: u64,
    ) -> HookRegistry {
        let path = directory.join("hooks.json");
        fs::write(
            &path,
            serde_json::to_vec(&json!({
                "version": 1,
                "hooks": [{
                    "id": "capture",
                    "event": "workspace.created",
                    "command": command,
                    "timeoutSeconds": timeout_seconds,
                    "maxAttempts": 1
                }]
            }))
            .unwrap(),
        )
        .unwrap();
        HookRegistry::load(&path).unwrap()
    }

    fn first_hook(registry: &HookRegistry) -> (HookDefinition, std::path::PathBuf) {
        let snapshot = registry
            .snapshot
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        (
            snapshot.definitions.values().next().unwrap().clone(),
            snapshot.working_directory.clone(),
        )
    }

    fn load_guard(
        directory: &std::path::Path,
        command: Vec<String>,
        timeout_seconds: u64,
    ) -> HookRegistry {
        let path = directory.join("hooks.json");
        fs::write(
            &path,
            serde_json::to_vec(&json!({
                "version": 1,
                "guards": [{
                    "id": "protect-delete",
                    "action": "workspace.delete",
                    "command": command,
                    "timeoutSeconds": timeout_seconds,
                    "onError": "deny"
                }]
            }))
            .unwrap(),
        )
        .unwrap();
        HookRegistry::load(&path).unwrap()
    }

    fn first_guard(registry: &HookRegistry) -> (GuardDefinition, std::path::PathBuf) {
        let snapshot = registry
            .snapshot
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        (
            snapshot.guards.values().next().unwrap().clone(),
            snapshot.working_directory.clone(),
        )
    }

    #[tokio::test]
    async fn command_receives_exact_event_without_inheriting_home() {
        let temporary = tempfile::tempdir().unwrap();
        let capture = temporary.path().join("capture.json");
        let registry = load_definition(
            temporary.path(),
            vec![
                "/bin/sh".to_owned(),
                "-c".to_owned(),
                "IFS= read -r payload || true; printf '%s' \"$payload\" > \"$1\"; [ -z \"${HOME+x}\" ]"
                    .to_owned(),
                "coco-hook".to_owned(),
                capture.display().to_string(),
            ],
            5,
        );
        let (definition, working_directory) = first_hook(&registry);
        let event = br#"{"schemaVersion":1,"id":"event-1"}"#;

        run_hook_command(&definition, &working_directory, event)
            .await
            .unwrap();

        assert_eq!(fs::read(capture).unwrap(), event);
    }

    #[tokio::test]
    async fn command_timeout_is_bounded() {
        let temporary = tempfile::tempdir().unwrap();
        let registry = load_definition(
            temporary.path(),
            vec!["/bin/sh".to_owned(), "-c".to_owned(), "sleep 2".to_owned()],
            1,
        );
        let (definition, working_directory) = first_hook(&registry);

        let event = vec![b'x'; 1024 * 1024];
        let error = run_hook_command(&definition, &working_directory, &event)
            .await
            .unwrap_err();

        assert_eq!(error, "hook command timed out after 1 seconds");
    }

    #[tokio::test]
    async fn guard_receives_exact_request_and_parses_a_decision() {
        let temporary = tempfile::tempdir().unwrap();
        let capture = temporary.path().join("guard.json");
        let registry = load_guard(
            temporary.path(),
            vec![
                "/bin/sh".to_owned(),
                "-c".to_owned(),
                "IFS= read -r payload || true; printf '%s' \"$payload\" > \"$1\"; [ -z \"${HOME+x}\" ]; printf '%s' '{\"decision\":\"deny\",\"reason\":\"not merged\"}'"
                    .to_owned(),
                "coco-guard".to_owned(),
                capture.display().to_string(),
            ],
            5,
        );
        let (definition, working_directory) = first_guard(&registry);
        let request = br#"{"schemaVersion":1,"id":"guard-1"}"#;

        let decision = run_guard(&definition, &working_directory, request)
            .await
            .unwrap();

        assert_eq!(decision, GuardDecision::Deny("not merged".to_owned()));
        assert_eq!(fs::read(capture).unwrap(), request);
    }

    #[tokio::test]
    async fn guard_rejects_invalid_or_oversized_output() {
        let temporary = tempfile::tempdir().unwrap();
        let invalid = load_guard(
            temporary.path(),
            vec![
                "/bin/sh".to_owned(),
                "-c".to_owned(),
                "printf not-json".to_owned(),
            ],
            5,
        );
        let (definition, working_directory) = first_guard(&invalid);
        assert!(
            run_guard(&definition, &working_directory, b"{}")
                .await
                .unwrap_err()
                .contains("invalid JSON")
        );

        let oversized = load_guard(
            temporary.path(),
            vec![
                "/bin/sh".to_owned(),
                "-c".to_owned(),
                format!("head -c {} /dev/zero", MAX_GUARD_OUTPUT_BYTES + 1),
            ],
            5,
        );
        let (definition, working_directory) = first_guard(&oversized);
        assert_eq!(
            run_guard(&definition, &working_directory, b"{}")
                .await
                .unwrap_err(),
            "guard output exceeded 8 KiB"
        );
    }

    #[tokio::test]
    async fn guard_timeout_is_bounded() {
        let temporary = tempfile::tempdir().unwrap();
        let registry = load_guard(
            temporary.path(),
            vec!["/bin/sh".to_owned(), "-c".to_owned(), "sleep 2".to_owned()],
            1,
        );
        let (definition, working_directory) = first_guard(&registry);

        assert_eq!(
            run_guard(&definition, &working_directory, b"{}")
                .await
                .unwrap_err(),
            "guard command timed out after 1 seconds"
        );
    }

    #[test]
    fn guard_output_is_strict_and_bounds_user_facing_reasons() {
        assert_eq!(
            parse_guard_output(br#"{"decision":"allow"}"#).unwrap(),
            GuardDecision::Allow
        );
        assert_eq!(
            parse_guard_output(br#"{"decision":"allow","reason":"unused"}"#).unwrap_err(),
            "guard allow decision must not include a reason"
        );
        assert!(parse_guard_output(br#"{"decision":"deny"}"#).is_err());
        assert!(parse_guard_output(br#"{"decision":"allow","extra":true}"#).is_err());

        let reason = format!("start\u{1b}{}", "x".repeat(MAX_GUARD_REASON_CHARS + 20));
        let output = serde_json::to_vec(&serde_json::json!({
            "decision": "deny",
            "reason": reason,
        }))
        .unwrap();
        let GuardDecision::Deny(reason) = parse_guard_output(&output).unwrap() else {
            panic!("expected a deny decision");
        };
        assert!(!reason.contains('\u{1b}'));
        assert_eq!(reason.chars().count(), MAX_GUARD_REASON_CHARS);
        assert!(reason.ends_with('…'));
    }
}
