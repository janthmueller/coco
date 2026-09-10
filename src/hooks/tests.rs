use std::fs;

use serde_json::json;

use super::*;
use crate::domain::{
    ContextMode, ProfileSnapshot, WorkspaceAvailability, WorkspaceLifecycle, WorkspacePhase,
    WorktreeMode,
};

fn repository(root: &Path) -> Repository {
    Repository {
        id: "repo-id".into(),
        root_path: root.into(),
        git_common_dir: root.join(".git"),
        display_name: "example".into(),
        is_linked_worktree: false,
        created_at_ms: 1,
        updated_at_ms: 1,
    }
}

fn workspace(root: &Path) -> Workspace {
    Workspace {
        id: "workspace-id".into(),
        create_operation_id: None,
        repository_id: "repo-id".into(),
        name: "review/widget".into(),
        context_mode: ContextMode::Fresh,
        context: json!({}),
        profile: ProfileSnapshot {
            name: "default".into(),
            source_path: None,
            source_hash: "sha256:test".into(),
            model_override: None,
            effective_settings: json!({}),
        },
        lifecycle: WorkspaceLifecycle::Ready,
        availability: WorkspaceAvailability::Open,
        phase: WorkspacePhase::Idle,
        wait_reasons: Vec::new(),
        worktree_mode: WorktreeMode::NewBranch,
        branch_name: Some("coco/review/widget".into()),
        base_sha: Some("abc".into()),
        worktree_path: Some(root.join("worktree")),
        codex_thread_id: Some("thread-id".into()),
        parent_thread_id: None,
        active_turn_id: None,
        thread_runtime: None,
        thread_archived: false,
        closed_head_sha: None,
        last_error_code: None,
        last_error_message: None,
        created_at_ms: 1,
        updated_at_ms: 1,
        closed_at_ms: None,
        completed_at_ms: None,
    }
}

fn write_config(root: &Path, body: Value) -> PathBuf {
    let path = root.join("hooks.json");
    fs::write(&path, serde_json::to_vec(&body).unwrap()).unwrap();
    path
}

#[test]
fn missing_configuration_is_an_empty_registry() {
    let temporary = tempfile::tempdir().unwrap();
    let registry = HookRegistry::load(&temporary.path().join("missing.json")).unwrap();
    assert_eq!(
        registry.summary(),
        HookRegistrySummary {
            hooks: Vec::new(),
            guards: Vec::new(),
        }
    );
}

#[test]
fn loads_and_matches_exact_signal_hooks_without_exposing_commands() {
    let temporary = tempfile::tempdir().unwrap();
    let path = write_config(
        temporary.path(),
        json!({
            "version": 1,
            "hooks": [{
                "id": "review-notify",
                "event": "signal.emitted",
                "signal": "review.requested@2",
                "command": ["/bin/sh", "-c", "exit 0"],
                "timeoutSeconds": 12,
                "maxAttempts": 2
            }]
        }),
    );
    let registry = HookRegistry::load(&path).unwrap();
    assert_eq!(
        registry.summary().hooks,
        vec![HookSummary {
            id: "review-notify".into(),
            event: HookEventKind::SignalEmitted,
            signal: Some("review.requested@2".into()),
            timeout_seconds: 12,
            max_attempts: 2,
        }]
    );
    let repo = repository(temporary.path());
    let workspace = workspace(temporary.path());
    assert!(
        registry
            .event(
                HookEventKind::SignalEmitted,
                &repo,
                &workspace,
                json!({"name": "review.requested", "version": 1}),
            )
            .is_none()
    );
    let dispatch = registry
        .event(
            HookEventKind::SignalEmitted,
            &repo,
            &workspace,
            json!({"name": "review.requested", "version": 2}),
        )
        .unwrap();
    assert_eq!(dispatch.targets.len(), 1);
    assert_eq!(dispatch.event.workspace.id, "workspace-id");
    assert_eq!(dispatch.event.repository.path, temporary.path());
}

#[test]
fn rejects_unsafe_or_ambiguous_definitions() {
    let temporary = tempfile::tempdir().unwrap();
    for (name, hook) in [
        (
            "relative",
            json!({"id": "relative", "event": "workspace.created", "command": ["sh"]}),
        ),
        (
            "wrong-filter",
            json!({"id": "wrong-filter", "event": "workspace.closed", "signal": "review.requested@1", "command": ["/bin/sh"]}),
        ),
        (
            "uncanonical",
            json!({"id": "uncanonical", "event": "signal.emitted", "signal": "review.requested@01", "command": ["/bin/sh"]}),
        ),
    ] {
        let path = write_config(temporary.path(), json!({"version": 1, "hooks": [hook]}));
        let error = HookRegistry::load(&path).unwrap_err();
        assert!(
            error.to_string().contains(name) || error.to_string().contains("signal"),
            "unexpected error: {error}"
        );
    }
}

#[test]
fn loads_guards_and_reloads_atomically() {
    let temporary = tempfile::tempdir().unwrap();
    let path = write_config(
        temporary.path(),
        json!({
            "version": 1,
            "hooks": [{
                "id": "created",
                "event": "workspace.created",
                "command": ["/bin/sh"]
            }],
            "guards": [{
                "id": "protect-delete",
                "action": "workspace.delete",
                "command": ["/bin/sh", "-c", "printf '{\"decision\":\"allow\"}'"],
                "timeoutSeconds": 4,
                "onError": "deny"
            }]
        }),
    );
    let registry = HookRegistry::load(&path).unwrap();
    assert_eq!(registry.summary().hooks.len(), 1);
    assert_eq!(
        registry.summary().guards,
        vec![GuardSummary {
            id: "protect-delete".into(),
            action: GuardAction::WorkspaceDelete,
            timeout_seconds: 4,
            on_error: GuardErrorPolicy::Deny,
        }]
    );

    fs::write(&path, br#"{"version":1,"hooks":"invalid"}"#).unwrap();
    assert!(registry.reload().is_err());
    assert_eq!(registry.summary().hooks.len(), 1);
    assert_eq!(registry.summary().guards.len(), 1);

    fs::write(&path, br#"{"version":1,"hooks":[],"guards":[]}"#).unwrap();
    assert_eq!(
        registry.reload().unwrap(),
        HookRegistrySummary {
            hooks: Vec::new(),
            guards: Vec::new(),
        }
    );
}

#[test]
fn rejects_duplicate_ids_across_hooks_and_guards() {
    let temporary = tempfile::tempdir().unwrap();
    let path = write_config(
        temporary.path(),
        json!({
            "version": 1,
            "hooks": [{
                "id": "shared",
                "event": "workspace.created",
                "command": ["/bin/sh"]
            }],
            "guards": [{
                "id": "shared",
                "action": "workspace.close",
                "command": ["/bin/sh"],
                "onError": "deny"
            }]
        }),
    );

    assert!(
        HookRegistry::load(&path)
            .unwrap_err()
            .to_string()
            .contains("duplicate hook or guard id")
    );
}

#[tokio::test]
async fn guards_deny_or_apply_their_explicit_error_policy() {
    let temporary = tempfile::tempdir().unwrap();
    let path = write_config(
        temporary.path(),
        json!({
            "version": 1,
            "guards": [
                {
                    "id": "broken-open",
                    "action": "workspace.delete",
                    "command": ["/bin/sh", "-c", "exit 3"],
                    "onError": "allow"
                },
                {
                    "id": "policy",
                    "action": "workspace.delete",
                    "command": ["/bin/sh", "-c", "printf '{\"decision\":\"deny\",\"reason\":\"not merged\"}'"],
                    "onError": "deny"
                }
            ]
        }),
    );
    let registry = HookRegistry::load(&path).unwrap();
    let error = registry
        .check_guards(
            GuardAction::WorkspaceDelete,
            &repository(temporary.path()),
            &workspace(temporary.path()),
            json!({"plan": {}}),
        )
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        GuardRejection::Denied { guard_id, reason, .. }
            if guard_id == "policy" && reason == "not merged"
    ));
}

#[tokio::test]
async fn guards_run_in_id_order_and_a_denial_short_circuits() {
    let temporary = tempfile::tempdir().unwrap();
    let later_capture = temporary.path().join("later-ran");
    let path = write_config(
        temporary.path(),
        json!({
            "version": 1,
            "guards": [
                {
                    "id": "z-later",
                    "action": "workspace.close",
                    "command": [
                        "/bin/sh",
                        "-c",
                        "printf ran > \"$1\"; printf '{\"decision\":\"allow\"}'",
                        "coco-guard",
                        later_capture
                    ],
                    "onError": "deny"
                },
                {
                    "id": "a-first",
                    "action": "workspace.close",
                    "command": [
                        "/bin/sh",
                        "-c",
                        "printf '{\"decision\":\"deny\",\"reason\":\"first denied\"}'"
                    ],
                    "onError": "deny"
                }
            ]
        }),
    );
    let registry = HookRegistry::load(&path).unwrap();

    let error = registry
        .check_guards(
            GuardAction::WorkspaceClose,
            &repository(temporary.path()),
            &workspace(temporary.path()),
            json!({"plan": {}}),
        )
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        GuardRejection::Denied { guard_id, reason, .. }
            if guard_id == "a-first" && reason == "first denied"
    ));
    assert!(!later_capture.exists());
}

#[test]
fn guard_error_policy_must_be_explicit() {
    let temporary = tempfile::tempdir().unwrap();
    let path = write_config(
        temporary.path(),
        json!({
            "version": 1,
            "guards": [{
                "id": "ambiguous-failure",
                "action": "workspace.close",
                "command": ["/bin/sh"]
            }]
        }),
    );

    assert!(
        HookRegistry::load(&path)
            .unwrap_err()
            .to_string()
            .contains("onError")
    );
}

#[test]
fn rejects_an_oversized_configuration_before_parsing_it() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("hooks.json");
    fs::write(&path, vec![b' '; (MAX_HOOK_CONFIG_BYTES + 1) as usize]).unwrap();

    let error = HookRegistry::load(&path).unwrap_err();

    assert!(matches!(error, HookConfigError::TooLarge(found) if found == path));
}

#[cfg(unix)]
#[test]
fn rejects_symlinked_or_group_writable_configuration() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let temporary = tempfile::tempdir().unwrap();
    let target = write_config(temporary.path(), json!({"version": 1, "hooks": []}));
    fs::set_permissions(&target, fs::Permissions::from_mode(0o620)).unwrap();
    let error = HookRegistry::load(&target).unwrap_err();
    assert!(matches!(error, HookConfigError::UnsafePermissions(found) if found == target));

    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
    let link = temporary.path().join("linked-hooks.json");
    symlink(&target, &link).unwrap();
    let error = HookRegistry::load(&link).unwrap_err();
    assert!(matches!(error, HookConfigError::UnsafePath(found) if found == link));
}
