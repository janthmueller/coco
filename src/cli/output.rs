use anyhow::Result;
use serde_json::{Value, json};

use crate::domain::{Decision, DecisionKind, DecisionState, Repository, Workspace, WorkspacePhase};
use crate::protocol::{
    RepositorySummary, WorkspaceCloseResult, WorkspaceDeleteResult, WorkspaceDiffResult,
    WorkspaceListItem, WorkspaceReopenResult, WorkspaceResult, WorkspaceRetirementPlan,
    WorkspaceStatusResult, WorkspaceThreadDisposition,
};

use super::style::{Palette, Tone};

mod collections;

pub(super) use collections::{
    print_model_list, print_repository_list, print_workspace_list, render_workspace_list_for_stdout,
};

const PUBLIC_SCHEMA_VERSION: u64 = 7;

pub(super) fn phase_label(phase: &str) -> &'static str {
    match phase {
        "provisioning" => "Preparing worktree",
        "starting" => "Starting Codex",
        "prepared" => "Prepared",
        "active" => "Working",
        "waiting_for_approval" => "Waiting for approval",
        "waiting_for_input" => "Waiting for input",
        "idle" => "Ready",
        "not_loaded" => "Codex thread is unloaded",
        "system_error" => "Codex system error",
        "unavailable" => "Status unavailable",
        "completed" => "Completed",
        "failed" => "Failed",
        "closing" => "Closing",
        "closed" => "Closed",
        "reopening" => "Reopening",
        "deleting" => "Deleting",
        _ => "Unknown",
    }
}

pub(super) fn versioned(value: Value) -> Value {
    match value {
        Value::Object(mut object) => {
            object.insert("schemaVersion".into(), Value::from(PUBLIC_SCHEMA_VERSION));
            Value::Object(object)
        }
        value => json!({ "schemaVersion": PUBLIC_SCHEMA_VERSION, "result": value }),
    }
}

pub(super) fn versioned_array(key: &str, value: Value) -> Value {
    let mut object = serde_json::Map::new();
    object.insert(
        "schemaVersion".to_owned(),
        Value::from(PUBLIC_SCHEMA_VERSION),
    );
    object.insert(key.to_owned(), value);
    Value::Object(object)
}

pub(super) fn print_json(value: Value) -> Result<()> {
    println!("{}", serde_json::to_string(&value)?);
    Ok(())
}

pub(super) fn print_repository_registered(repository: &Repository) {
    print!(
        "{}",
        render_repository_registered(repository, Palette::stdout())
    );
}

pub(super) fn print_workspace_created(result: &WorkspaceResult) {
    print!(
        "{}",
        render_workspace_success("Created", result, true, Palette::stdout())
    );
}

pub(super) fn print_turn_started(result: &WorkspaceResult) {
    print!(
        "{}",
        render_workspace_success("Sent to", result, false, Palette::stdout())
    );
}

pub(super) fn print_workspace_closed(result: &WorkspaceCloseResult) {
    let palette = Palette::stdout();
    let prefix = format!(
        "{} Closed {}",
        palette.paint(Tone::GreenBold, "✓"),
        palette.paint(Tone::Bold, safe_line(&result.workspace.name)),
    );
    if result.plan.thread_id.is_some() {
        println!(
            "{} {} {}",
            prefix,
            palette.paint(Tone::Dim, "·"),
            palette.paint(
                Tone::Dim,
                match result.plan.thread_disposition {
                    WorkspaceThreadDisposition::Archive => "thread archived",
                    WorkspaceThreadDisposition::Retain => "thread retained",
                    WorkspaceThreadDisposition::Delete => "thread deleted",
                }
            ),
        );
    } else {
        println!("{prefix}");
    }
}

pub(super) fn print_workspace_reopened(result: &WorkspaceReopenResult) {
    let palette = Palette::stdout();
    println!(
        "{} Reopened {}",
        palette.paint(Tone::GreenBold, "✓"),
        palette.paint(Tone::Bold, safe_line(&result.workspace.name)),
    );
    if let Some(path) = &result.workspace.worktree_path {
        println!("  {}", palette.paint(Tone::Dim, path.display()));
    }
}

pub(super) fn print_workspace_deleted(result: &WorkspaceDeleteResult) {
    let palette = Palette::stdout();
    println!(
        "{} Deleted {}",
        palette.paint(Tone::GreenBold, "✓"),
        palette.paint(Tone::Bold, safe_line(&result.plan.workspace_name)),
    );
}

pub(super) fn print_retirement_plan(action: &str, plan: &WorkspaceRetirementPlan) {
    let palette = Palette::stdout();
    println!(
        "{} {} {}",
        palette.paint(Tone::Bold, action),
        palette.paint(Tone::Bold, safe_line(&plan.workspace_name)),
        palette.paint(Tone::Dim, "plan"),
    );
    println!(
        "  Worktree  {}",
        palette.paint(Tone::Dim, plan.worktree_path.display())
    );
    if plan.has_local_changes() {
        let tracked = if plan.tracked_changes {
            "tracked changes, "
        } else {
            ""
        };
        println!(
            "  Changes   {}{} untracked, {} ignored",
            tracked, plan.untracked_file_count, plan.ignored_file_count
        );
    } else {
        println!("  Changes   none");
    }
    println!(
        "  Thread    {}",
        if plan.thread_id.is_none() {
            "none"
        } else {
            match plan.thread_disposition {
                WorkspaceThreadDisposition::Retain => "retain",
                WorkspaceThreadDisposition::Archive => "archive",
                WorkspaceThreadDisposition::Delete => "delete",
            }
        }
    );
    println!(
        "  Branch    {}",
        if plan.branch_name.is_none() {
            "none"
        } else if plan.delete_branch {
            "delete"
        } else {
            "retain"
        }
    );
    if !plan.blockers.is_empty() {
        println!("  {}", palette.paint(Tone::RedBold, "Blocked"));
        for blocker in &plan.blockers {
            println!(
                "    {} {}",
                palette.paint(Tone::Red, "✗"),
                safe_line(blocker)
            );
        }
    }
}

pub(super) fn print_decision_sent(workspace: &Workspace) {
    let palette = Palette::stdout();
    println!(
        "{} Response sent to {} {} {}",
        palette.paint(Tone::GreenBold, "✓"),
        palette.paint(Tone::Magenta, "Codex"),
        palette.paint(Tone::Dim, "·"),
        palette.paint(Tone::Bold, safe_line(&workspace.name)),
    );
}

pub(super) fn print_status(result: &WorkspaceStatusResult) {
    print!("{}", render_status_for_stdout(result));
}

pub(super) fn render_status_for_stdout(result: &WorkspaceStatusResult) -> String {
    render_status(result, None, Palette::stdout())
}

pub(super) fn render_follow_status(result: &WorkspaceStatusResult, spinner: &str) -> String {
    render_status(result, Some(spinner), Palette::stdout())
}

pub(super) fn render_workspace_update(
    item: &WorkspaceListItem,
    include_repository: bool,
) -> String {
    let repository = include_repository.then_some(&item.repository);
    format!(
        "{}\n",
        render_workspace_state_line(&item.workspace, repository, None, Palette::stdout())
    )
}

pub(super) fn print_diff(result: &WorkspaceDiffResult) {
    print!("{}", render_diff(result));
}

pub(super) fn render_diff(result: &WorkspaceDiffResult) -> String {
    let mut output = String::new();
    if !result.patch.is_empty() {
        output.push_str(&result.patch);
        if !result.patch.ends_with('\n') {
            output.push('\n');
        }
    }
    if result.patch_truncated {
        output.push_str("Warning: tracked patch output was truncated.\n");
    }
    if !result.untracked_paths.is_empty() {
        output.push_str("Untracked:\n");
        for path in &result.untracked_paths {
            output.push_str(&path.display().to_string());
            output.push('\n');
        }
    }
    if result.patch.is_empty() && result.untracked_paths.is_empty() && !result.patch_truncated {
        output.push_str("No changes.\n");
    }
    output
}

fn render_repository_registered(repository: &Repository, palette: Palette) -> String {
    format!(
        "{} Registered {}\n  {}\n",
        palette.paint(Tone::GreenBold, "✓"),
        palette.paint(Tone::Bold, safe_line(&repository.display_name)),
        palette.paint(
            Tone::Dim,
            safe_line(&repository.root_path.display().to_string())
        ),
    )
}

fn render_workspace_success(
    action: &str,
    result: &WorkspaceResult,
    include_location: bool,
    palette: Palette,
) -> String {
    let workspace = &result.workspace;
    let phase = phase_presentation(workspace.phase);
    let mut output = format!(
        "{} {action} {} {} {}\n",
        palette.paint(Tone::GreenBold, "✓"),
        palette.paint(Tone::Bold, safe_line(&workspace.name)),
        palette.paint(Tone::Dim, "·"),
        palette.paint(phase.tone, phase.label),
    );
    if include_location && let Some(detail) = workspace_location(workspace) {
        output.push_str("  ");
        output.push_str(&palette.paint(Tone::Dim, detail));
        output.push('\n');
    }
    output
}

fn render_status(
    result: &WorkspaceStatusResult,
    marker_override: Option<&str>,
    palette: Palette,
) -> String {
    let workspace = &result.workspace;
    let mut output = format!(
        "{}\n",
        render_workspace_state_line(workspace, None, marker_override, palette)
    );
    if let Some(detail) = workspace_location(workspace) {
        output.push_str("  ");
        output.push_str(&palette.paint(Tone::Dim, detail));
        output.push('\n');
    }
    if let Some(message) = &workspace.last_error_message {
        output.push_str(&format!(
            "  {} {}\n",
            palette.paint(Tone::RedBold, "✗"),
            palette.paint(Tone::Red, safe_line(message)),
        ));
    }
    output.push_str(&render_decision_hints(&result.open_decisions, palette));
    if result.open_decisions.is_empty()
        && matches!(
            workspace.phase,
            WorkspacePhase::WaitingForApproval | WorkspacePhase::WaitingForInput
        )
    {
        output.push_str(&format!(
            "  {} {}\n",
            palette.paint(Tone::RedBold, "✗"),
            palette.paint(
                Tone::Red,
                "This Codex request cannot be answered with coco decide."
            ),
        ));
    }
    output
}

fn render_decision_hints(decisions: &[Decision], palette: Palette) -> String {
    let mut output = String::new();
    for decision in decisions {
        match decision.state {
            DecisionState::Pending => {
                output.push_str(&format!(
                    "  {} {} {} {}\n",
                    palette.paint(Tone::CyanBold, "→"),
                    decision_kind_label(decision.kind),
                    palette.paint(Tone::Dim, "·"),
                    palette.paint(Tone::Cyan, format!("coco decide {}", decision.id)),
                ));
            }
            DecisionState::Submitted => {
                output.push_str(&format!(
                    "  {} Response sent {} waiting for {}\n",
                    palette.paint(Tone::CyanBold, "→"),
                    palette.paint(Tone::Dim, "·"),
                    palette.paint(Tone::Magenta, "Codex"),
                ));
            }
            DecisionState::Resolved | DecisionState::Orphaned => {}
        }
    }
    output
}

pub(super) fn render_workspace_state_line(
    workspace: &Workspace,
    repository: Option<&RepositorySummary>,
    marker_override: Option<&str>,
    palette: Palette,
) -> String {
    let phase = phase_presentation(workspace.phase);
    let marker = marker_override.unwrap_or(phase.marker);
    let marker_tone = if marker_override.is_some() {
        Tone::CyanBold
    } else {
        phase.tone
    };
    let mut output = format!("{} ", palette.paint(marker_tone, marker));
    if let Some(repository) = repository {
        output.push_str(&palette.paint(
            Tone::Dim,
            format!(
                "{} · ",
                safe_line(&repository.root_path.display().to_string())
            ),
        ));
    }
    output.push_str(&palette.paint(Tone::Bold, safe_line(&workspace.name)));
    output.push_str("  ");
    output.push_str(&palette.paint(phase.tone, phase.label));
    output
}

fn decision_kind_label(kind: DecisionKind) -> &'static str {
    match kind {
        DecisionKind::CommandApproval => "Command approval",
        DecisionKind::FileChangeApproval => "File-change approval",
        DecisionKind::UserInput => "Question from Codex",
    }
}

fn workspace_location(workspace: &Workspace) -> Option<String> {
    let mut parts = Vec::with_capacity(2);
    if workspace.worktree_path.is_some() {
        parts.push(
            workspace
                .branch_name
                .clone()
                .unwrap_or_else(|| "detached".to_owned()),
        );
    }
    if let Some(path) = &workspace.worktree_path {
        parts.push(path.display().to_string());
    }
    (!parts.is_empty()).then(|| safe_line(&parts.join(" · ")))
}

#[derive(Debug, Clone, Copy)]
struct PhasePresentation {
    marker: &'static str,
    label: &'static str,
    tone: Tone,
}

fn phase_presentation(phase: WorkspacePhase) -> PhasePresentation {
    let (marker, tone) = match phase {
        WorkspacePhase::Prepared | WorkspacePhase::NotLoaded => ("○", Tone::Dim),
        WorkspacePhase::Idle | WorkspacePhase::Completed => ("●", Tone::Green),
        WorkspacePhase::SystemError | WorkspacePhase::Unavailable | WorkspacePhase::Failed => {
            ("✗", Tone::Red)
        }
        WorkspacePhase::Active
        | WorkspacePhase::WaitingForApproval
        | WorkspacePhase::WaitingForInput
        | WorkspacePhase::Provisioning
        | WorkspacePhase::Starting
        | WorkspacePhase::Closing
        | WorkspacePhase::Reopening
        | WorkspacePhase::Deleting => ("●", Tone::Cyan),
        WorkspacePhase::Closed => ("○", Tone::Dim),
    };
    PhasePresentation {
        marker,
        label: phase_label(phase.as_str()),
        tone,
    }
}

pub(super) fn safe_line(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn diff_output_remains_exact_unstyled_content() {
        let result = WorkspaceDiffResult {
            patch: "diff --git a/file b/file\n".to_owned(),
            patch_truncated: true,
            untracked_paths: vec![PathBuf::from("new.txt")],
        };
        assert_eq!(
            render_diff(&result),
            "diff --git a/file b/file\nWarning: tracked patch output was truncated.\nUntracked:\nnew.txt\n"
        );
    }

    #[test]
    fn empty_diff_has_a_short_plain_result() {
        let result = WorkspaceDiffResult {
            patch: String::new(),
            patch_truncated: false,
            untracked_paths: Vec::new(),
        };
        assert_eq!(render_diff(&result), "No changes.\n");
    }

    #[test]
    fn terminal_fields_cannot_inject_control_sequences() {
        assert_eq!(safe_line("repo\n\u{1b}[31m"), "repo  [31m");
    }
}
