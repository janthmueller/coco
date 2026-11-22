use serde_json::{Value, json};

use crate::domain::runtime::WorkspaceRuntimeState;
use crate::protocol::WorkspaceStatuslineSnapshot;

#[cfg(test)]
mod tests;

/// Native state is presentation input, not authority for CoCo lookup/scope.
pub(super) fn footer(state: &Value, workspace: Option<&WorkspaceStatuslineSnapshot>) -> Value {
    let mut pieces = Vec::new();
    if let Some(workspace) = workspace {
        pieces.push(text(&workspace.name, "status-path"));
    } else {
        pieces.push(text("CoCo unavailable", "status-metadata"));
    }
    if let Some(model) = string(state, "/thread/model/display_name") {
        let effort = string(state, "/thread/model/reasoning_level");
        let label = effort.map_or_else(|| model.clone(), |effort| format!("{model} {effort}"));
        pieces.push(text(&label, "status-model"));
    }
    if let Some(resources) = workspace.and_then(|workspace| workspace.resources.as_ref())
        && resources.state == WorkspaceRuntimeState::Running
    {
        if let Some((label, bytes)) = resources
            .memory_current_bytes
            .map(|bytes| ("RAM", bytes))
            .or_else(|| resources.resident_memory_bytes.map(|bytes| ("RSS", bytes)))
        {
            pieces.push(text(
                &format!("{label} {:.1} MiB", bytes as f64 / (1024.0 * 1024.0)),
                "status-usage",
            ));
        }
        if let Some(cpu) = resources
            .cpu_percent
            .filter(|cpu| cpu.is_finite() && *cpu >= 0.0)
        {
            pieces.push(text(&format!("CPU {cpu:.1}%"), "status-usage"));
        }
    }
    if let Some(used) = number(state, "/thread/usage/context/used_percentage") {
        pieces.push(text(
            &format!("Ctx {:.0}% left", (100.0 - used).clamp(0.0, 100.0)),
            "status-usage",
        ));
    }
    quota(state, &mut pieces);
    let children = pieces
        .into_iter()
        .enumerate()
        .flat_map(|(index, item)| {
            if index == 0 {
                vec![item]
            } else {
                vec![text("·", "status-metadata"), item]
            }
        })
        .collect::<Vec<_>>();
    json!({"type":"row", "props":{"gap":1, "wrap":"wrap"}, "children":children})
}

fn quota(state: &Value, pieces: &mut Vec<Value>) {
    let selected = state
        .pointer("/config/statusline_items")
        .and_then(Value::as_array);
    let enabled =
        |name| selected.is_some_and(|items| items.iter().take(64).any(|item| item == name));
    let Some(buckets) = state
        .pointer("/account/rate_limits")
        .and_then(Value::as_array)
    else {
        return;
    };
    let bucket = buckets
        .iter()
        .take(8)
        .find(|bucket| bucket["limit_id"] == "codex");
    let Some(bucket) = bucket else {
        return;
    };
    for (field, setting, label) in [
        ("primary_limit", "five-hour-limit", "5h"),
        ("secondary_limit", "weekly-limit", "Week"),
    ] {
        if !enabled(setting) {
            continue;
        }
        let window = &bucket[field];
        let expected = if field == "primary_limit" { 300 } else { 10080 };
        if window["window_minutes"].as_u64() != Some(expected) {
            continue;
        }
        if let Some(used) = number(window, "/used_percent") {
            pieces.push(text(
                &format!("{label} {:.0}%", (100.0 - used).clamp(0.0, 100.0)),
                "status-limit",
            ));
        }
    }
}

fn number(value: &Value, path: &str) -> Option<f64> {
    value
        .pointer(path)
        .and_then(Value::as_f64)
        .filter(|number| number.is_finite() && *number >= 0.0)
}

fn string(value: &Value, path: &str) -> Option<String> {
    value
        .pointer(path)
        .and_then(Value::as_str)
        .map(clean)
        .filter(|text| !text.is_empty())
}

fn clean(value: &str) -> String {
    value
        .chars()
        .filter(|ch| {
            !ch.is_control() && !matches!(*ch, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
        .take(96)
        .collect()
}

fn text(value: &str, color: &str) -> Value {
    json!({"type":"text", "props":{"color":color}, "children":[clean(value)]})
}
