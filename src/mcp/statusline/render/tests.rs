use super::*;
use crate::domain::runtime::{
    WorkspaceResourceScope, WorkspaceRuntimeBackend, WorkspaceRuntimeResources,
};

fn snapshot() -> WorkspaceStatuslineSnapshot {
    WorkspaceStatuslineSnapshot {
        workspace_id: "workspace".into(),
        name: "feat/login".into(),
        resources: Some(WorkspaceRuntimeResources {
            backend: WorkspaceRuntimeBackend::ExecServer,
            state: WorkspaceRuntimeState::Running,
            scope: WorkspaceResourceScope::CgroupV2,
            process_id: None,
            process_count: None,
            task_count: None,
            resident_memory_bytes: Some(999 * 1024 * 1024),
            memory_current_bytes: Some(24 * 1024 * 1024),
            cpu_percent: Some(150.0),
            cpu_usage_usec: None,
            cgroup_unit: None,
            events: None,
            sampled_at_ms: Some(1),
        }),
    }
}

#[test]
fn native_model_context_quota_and_cgroup_memory_are_used_without_duplicate_git_labels() {
    let state = json!({"thread":{"model":{"display_name":"Custom model", "reasoning_level":"high"},
        "usage":{"context":{"used_percentage":35}}},
        "config":{"statusline_items":["five-hour-limit", "weekly-limit"]},
        "account":{"rate_limits":[{"limit_id":"codex", "primary_limit":{"used_percent":25,"window_minutes":300},
            "secondary_limit":{"used_percent":50,"window_minutes":10080}}]}});
    let tree = footer(&state, Some(&snapshot())).to_string();
    for value in [
        "feat/login",
        "Custom model high",
        "RAM 24.0 MiB",
        "CPU 150.0%",
        "Ctx 65% left",
        "5h 75%",
        "Week 50%",
    ] {
        assert!(tree.contains(value), "missing {value}: {tree}");
    }
    assert!(!tree.contains("999") && !tree.contains("branch"));
}

#[test]
fn unknown_inactive_and_invalid_measurements_are_omitted_instead_of_becoming_zero() {
    let mut snapshot = snapshot();
    snapshot.resources.as_mut().unwrap().cpu_percent = Some(f64::NAN);
    let tree = footer(&Value::Null, Some(&snapshot)).to_string();
    assert!(!tree.contains("CPU"));
    snapshot.resources.as_mut().unwrap().state = WorkspaceRuntimeState::Inactive;
    let tree = footer(&Value::Null, Some(&snapshot)).to_string();
    assert!(!tree.contains("RAM") && !tree.contains("CPU"));
    let state = json!({"config":{"statusline_items":["weekly-limit"]},
        "account":{"rate_limits":[{"secondary_limit":{"used_percent":20,"window_minutes":60}}]}});
    assert!(!footer(&state, None).to_string().contains("Week"));
}

#[test]
fn process_memory_fallback_is_labelled_rss_instead_of_cgroup_ram() {
    let mut snapshot = snapshot();
    let resources = snapshot.resources.as_mut().unwrap();
    resources.memory_current_bytes = None;
    resources.scope = WorkspaceResourceScope::ProcessTree;
    let tree = footer(&Value::Null, Some(&snapshot)).to_string();
    assert!(tree.contains("RSS 999.0 MiB") && !tree.contains("RAM"));
}

#[test]
fn quota_requires_the_native_codex_bucket_and_explicit_native_selection() {
    let mut state = json!({"config":{"statusline_items":["five-hour-limit"]},
        "account":{"rate_limits":[{"limit_id":"other", "primary_limit":{"used_percent":25,"window_minutes":300}}]}});
    assert!(!footer(&state, None).to_string().contains("5h"));
    state["account"]["rate_limits"][0]["limit_id"] = json!("codex");
    assert!(footer(&state, None).to_string().contains("5h 75%"));
    state["config"]["statusline_items"] = json!([]);
    assert!(!footer(&state, None).to_string().contains("5h"));
}

#[test]
fn untrusted_text_and_shapes_stay_small_and_cannot_inject_terminal_controls_or_ui_nodes() {
    let mut snapshot = snapshot();
    snapshot.name = format!("bad\n\u{1b}\u{202e}{}", "界".repeat(20000));
    let state = json!({"thread":{"model":{"display_name":"x".repeat(20000),"reasoning_level":"\u{2066}max"}},
        "injected_ui":{"type":"link", "props":{"href":"file:///private"}}, "credentials":"synthetic-secret"});
    let tree = footer(&state, Some(&snapshot)).to_string();
    assert!(tree.len() < 4096);
    for hidden in [
        "\\n",
        "\\u001b",
        "\u{202e}",
        "\u{2066}",
        "synthetic-secret",
        "file:///private",
    ] {
        assert!(!tree.contains(hidden), "unsafe content: {hidden}");
    }
}
