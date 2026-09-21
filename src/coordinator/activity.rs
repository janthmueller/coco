use std::collections::HashMap;

use chrono::Utc;
use serde_json::Value;

use super::Coordinator;
use crate::domain::activity::{WorkspaceActivity, WorkspaceActivitySource};
use crate::domain::{CodexThreadStatus, Workspace, WorkspacePhase};

const MAX_ACTIVITY_LABEL_CHARS: usize = 120;
const MAX_REASONING_BUFFER_CHARS: usize = 2_048;

impl Coordinator {
    pub(super) fn observe_workspace_activity(&self, method: &str, params: &Value) {
        self.activities
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .observe(method, params, &self.runtime_generation);
    }

    pub(super) fn clear_workspace_activities(&self) {
        self.activities
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }

    pub(super) fn workspace_activity(&self, workspace: &Workspace) -> Option<WorkspaceActivity> {
        let thread_id = workspace.codex_thread_id.as_deref()?;
        let mut activities = self
            .activities
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !matches!(
            workspace
                .thread_runtime
                .as_ref()
                .map(|runtime| &runtime.status),
            Some(CodexThreadStatus::Active { .. })
        ) {
            activities.remove(thread_id);
            return None;
        }
        if workspace.phase != WorkspacePhase::Active {
            return None;
        }
        let activity = activities.snapshot(thread_id)?;
        (activity.runtime_generation == self.runtime_generation).then_some(activity)
    }
}

#[derive(Default)]
pub(super) struct ActivityRegistry {
    threads: HashMap<String, ThreadActivity>,
}

struct ThreadActivity {
    turn_id: String,
    reasoning_item_id: Option<String>,
    reasoning_summary_index: Option<i64>,
    reasoning_buffer: String,
    reasoning: Option<WorkspaceActivity>,
    compaction: Option<WorkspaceActivity>,
}

impl ThreadActivity {
    fn new(turn_id: &str) -> Self {
        Self {
            turn_id: turn_id.to_owned(),
            reasoning_item_id: None,
            reasoning_summary_index: None,
            reasoning_buffer: String::new(),
            reasoning: None,
            compaction: None,
        }
    }

    fn visible(&self) -> Option<&WorkspaceActivity> {
        self.compaction.as_ref().or(self.reasoning.as_ref())
    }
}

impl ActivityRegistry {
    pub(super) fn observe(&mut self, method: &str, params: &Value, generation: &str) {
        match method {
            "thread/status/changed" => self.observe_thread_status(params),
            "turn/started" => self.observe_turn_started(params),
            "item/started" => self.observe_item_started(params, generation),
            "item/reasoning/summaryPartAdded" => self.observe_summary_part(params),
            "item/reasoning/summaryTextDelta" => {
                self.observe_summary_delta(params, generation);
            }
            "item/completed" => self.observe_item_completed(params),
            "turn/completed" => self.observe_turn_completed(params),
            "thread/archived" | "thread/deleted" => self.remove_thread(params),
            _ => {}
        }
    }

    pub(super) fn snapshot(&self, thread_id: &str) -> Option<WorkspaceActivity> {
        self.threads
            .get(thread_id)
            .and_then(ThreadActivity::visible)
            .cloned()
    }

    pub(super) fn clear(&mut self) {
        self.threads.clear();
    }

    fn remove(&mut self, thread_id: &str) {
        self.threads.remove(thread_id);
    }

    fn observe_thread_status(&mut self, params: &Value) {
        let Some(thread_id) = string(params, "/threadId") else {
            return;
        };
        if string(params, "/status/type") != Some("active") {
            self.threads.remove(thread_id);
        }
    }

    fn observe_turn_started(&mut self, params: &Value) {
        let (Some(thread_id), Some(turn_id)) =
            (string(params, "/threadId"), string(params, "/turn/id"))
        else {
            return;
        };
        self.threads
            .insert(thread_id.to_owned(), ThreadActivity::new(turn_id));
    }

    fn observe_item_started(&mut self, params: &Value, generation: &str) {
        let (Some(thread_id), Some(turn_id), Some(item_id), Some(item_type)) = (
            string(params, "/threadId"),
            string(params, "/turnId"),
            string(params, "/item/id"),
            string(params, "/item/type"),
        ) else {
            return;
        };
        let Some(activity) = self.activity_for_turn(thread_id, turn_id, true) else {
            return;
        };
        match item_type {
            "contextCompaction" => {
                activity.reasoning_item_id = None;
                activity.reasoning_summary_index = None;
                activity.reasoning_buffer.clear();
                activity.reasoning = None;
                activity.compaction = Some(snapshot(
                    "Compacting context".to_owned(),
                    WorkspaceActivitySource::ContextCompaction,
                    thread_id,
                    turn_id,
                    item_id,
                    false,
                    generation,
                ));
            }
            "reasoning" if activity.compaction.is_none() => {
                activity.reasoning_item_id = Some(item_id.to_owned());
                activity.reasoning_summary_index = None;
                activity.reasoning_buffer.clear();
            }
            _ => {}
        }
    }

    fn observe_summary_part(&mut self, params: &Value) {
        let (Some(thread_id), Some(turn_id), Some(item_id)) = (
            string(params, "/threadId"),
            string(params, "/turnId"),
            string(params, "/itemId"),
        ) else {
            return;
        };
        let Some(activity) = self.activity_for_turn(thread_id, turn_id, false) else {
            return;
        };
        if activity.reasoning_item_id.as_deref() != Some(item_id) {
            return;
        }
        activity.reasoning_summary_index = params.get("summaryIndex").and_then(Value::as_i64);
        activity.reasoning_buffer.clear();
    }

    fn observe_summary_delta(&mut self, params: &Value, generation: &str) {
        let (Some(thread_id), Some(turn_id), Some(item_id), Some(delta)) = (
            string(params, "/threadId"),
            string(params, "/turnId"),
            string(params, "/itemId"),
            string(params, "/delta"),
        ) else {
            return;
        };
        let summary_index = params.get("summaryIndex").and_then(Value::as_i64);
        let activity = self.threads.entry(thread_id.to_owned()).or_insert_with(|| {
            let mut activity = ThreadActivity::new(turn_id);
            activity.reasoning_item_id = Some(item_id.to_owned());
            activity
        });
        if activity.turn_id != turn_id
            || activity.compaction.is_some()
            || activity.reasoning_item_id.as_deref() != Some(item_id)
        {
            return;
        }
        if activity.reasoning_summary_index != summary_index {
            activity.reasoning_summary_index = summary_index;
            activity.reasoning_buffer.clear();
        }
        push_bounded(&mut activity.reasoning_buffer, delta);
        let Some(label) = latest_summary_line(&activity.reasoning_buffer) else {
            return;
        };
        let (label, truncated) = truncate(&label, MAX_ACTIVITY_LABEL_CHARS);
        activity.reasoning = Some(snapshot(
            label,
            WorkspaceActivitySource::ReasoningSummary,
            thread_id,
            turn_id,
            item_id,
            truncated,
            generation,
        ));
    }

    fn observe_item_completed(&mut self, params: &Value) {
        let (Some(thread_id), Some(turn_id), Some(item_id), Some(item_type)) = (
            string(params, "/threadId"),
            string(params, "/turnId"),
            string(params, "/item/id"),
            string(params, "/item/type"),
        ) else {
            return;
        };
        let Some(activity) = self.activity_for_turn(thread_id, turn_id, false) else {
            return;
        };
        match item_type {
            "contextCompaction"
                if activity
                    .compaction
                    .as_ref()
                    .map(|value| value.item_id.as_str())
                    == Some(item_id) =>
            {
                activity.compaction = None;
            }
            "reasoning" if activity.reasoning_item_id.as_deref() == Some(item_id) => {
                activity.reasoning_item_id = None;
                activity.reasoning_summary_index = None;
                activity.reasoning_buffer.clear();
            }
            _ => {}
        }
    }

    fn observe_turn_completed(&mut self, params: &Value) {
        let (Some(thread_id), Some(turn_id)) =
            (string(params, "/threadId"), string(params, "/turn/id"))
        else {
            return;
        };
        if self
            .threads
            .get(thread_id)
            .is_some_and(|activity| activity.turn_id == turn_id)
        {
            self.threads.remove(thread_id);
        }
    }

    fn remove_thread(&mut self, params: &Value) {
        if let Some(thread_id) = string(params, "/threadId") {
            self.threads.remove(thread_id);
        }
    }

    fn activity_for_turn(
        &mut self,
        thread_id: &str,
        turn_id: &str,
        create: bool,
    ) -> Option<&mut ThreadActivity> {
        if create {
            self.threads
                .entry(thread_id.to_owned())
                .or_insert_with(|| ThreadActivity::new(turn_id));
        }
        self.threads
            .get_mut(thread_id)
            .filter(|activity| activity.turn_id == turn_id)
    }
}

fn snapshot(
    label: String,
    source: WorkspaceActivitySource,
    thread_id: &str,
    turn_id: &str,
    item_id: &str,
    truncated: bool,
    generation: &str,
) -> WorkspaceActivity {
    WorkspaceActivity {
        label,
        source,
        thread_id: thread_id.to_owned(),
        turn_id: turn_id.to_owned(),
        item_id: item_id.to_owned(),
        truncated,
        runtime_generation: generation.to_owned(),
        observed_at_ms: Utc::now().timestamp_millis(),
    }
}

fn string<'a>(params: &'a Value, pointer: &str) -> Option<&'a str> {
    params.pointer(pointer).and_then(Value::as_str)
}

fn push_bounded(buffer: &mut String, delta: &str) {
    buffer.push_str(delta);
    let excess = buffer
        .chars()
        .count()
        .saturating_sub(MAX_REASONING_BUFFER_CHARS);
    if excess == 0 {
        return;
    }
    let retained_at = buffer
        .char_indices()
        .nth(excess)
        .map_or(buffer.len(), |(index, _)| index);
    buffer.drain(..retained_at);
    if let Some(newline) = buffer.find('\n') {
        buffer.drain(..=newline);
    } else {
        buffer.clear();
    }
}

fn latest_summary_line(buffer: &str) -> Option<String> {
    buffer.lines().rev().find_map(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with("<!--") {
            return None;
        }
        let line = line.trim_start_matches('#').trim();
        let line = if let Some(stripped) = line.strip_prefix("**") {
            let (bold, trailing) = stripped.split_once("**")?;
            format!("{bold}{trailing}")
        } else {
            line.to_owned()
        };
        let normalized = normalize(&line);
        (!normalized.is_empty()).then_some(normalized)
    })
}

fn normalize(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut separator = false;
    for character in value.chars() {
        if character.is_whitespace() || character.is_control() {
            separator |= !normalized.is_empty();
            continue;
        }
        if separator {
            normalized.push(' ');
            separator = false;
        }
        normalized.push(character);
    }
    normalized
}

fn truncate(value: &str, max_chars: usize) -> (String, bool) {
    if value.chars().count() <= max_chars {
        return (value.to_owned(), false);
    }
    let mut value = value
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    value.push('…');
    (value, true)
}

#[cfg(test)]
mod tests;
