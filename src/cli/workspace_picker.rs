use std::cmp::Ordering;

use crate::domain::WorktreeMode;
use crate::protocol::WorkspaceListItem;

use super::args::WorkspaceSort;
use super::output::phase_label;
use super::prompt::Choice;
use super::status::natural_cmp;

pub(super) fn choices(
    rows: &mut [WorkspaceListItem],
    sort: WorkspaceSort,
    global: bool,
    now_ms: i64,
) -> Vec<Choice> {
    rows.sort_by(|left, right| {
        let order = match sort {
            WorkspaceSort::Recent => recency(right).cmp(&recency(left)),
            WorkspaceSort::Name => Ordering::Equal,
        };
        order
            .then_with(|| natural_cmp(&left.workspace.name, &right.workspace.name))
            .then_with(|| left.repository.root_path.cmp(&right.repository.root_path))
            .then_with(|| left.workspace.id.cmp(&right.workspace.id))
    });
    rows.iter().map(|row| choice(row, global, now_ms)).collect()
}

fn recency(row: &WorkspaceListItem) -> Option<i64> {
    row.workspace
        .thread_runtime
        .as_ref()?
        .recency_at_ms
        .filter(|millis| *millis >= 0)
        .filter(|millis| chrono::DateTime::from_timestamp_millis(*millis).is_some())
}

fn choice(row: &WorkspaceListItem, global: bool, now_ms: i64) -> Choice {
    let mut detail = vec![phase_label(row.workspace.phase.as_str()).to_owned()];
    match row.workspace.branch_name.as_deref() {
        Some(branch) if branch != row.workspace.name => detail.push(branch.to_owned()),
        None if row.workspace.worktree_mode == WorktreeMode::Detached => {
            detail.push("detached".to_owned());
        }
        _ => {}
    }
    if global {
        detail.push(row.repository.root_path.display().to_string());
    }
    Choice::new(row.workspace.name.clone(), Some(detail.join(" · ")))
        .with_trailing_detail(recency(row).map(|time| relative_time(time, now_ms)))
}

fn relative_time(time_ms: i64, now_ms: i64) -> String {
    let minutes = now_ms.saturating_sub(time_ms).max(0) / 60_000;
    match minutes {
        0 => "just now".to_owned(),
        1..60 => format!("{minutes}m ago"),
        60..1_440 => format!("{}h ago", minutes / 60),
        1_440..10_080 => format!("{}d ago", minutes / 1_440),
        10_080..40_320 => format!("{}w ago", minutes / 10_080),
        40_320..525_600 => format!("{}mo ago", (minutes / 43_200).max(1)),
        _ => format!("{}y ago", minutes / 525_600),
    }
}

#[cfg(test)]
mod tests;
