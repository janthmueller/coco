use anyhow::Result;
use serde_json::{Value, json};

use crate::domain::runtime::{
    WorkspaceResourceControllerStatus, WorkspaceResourcePolicy, WorkspaceResourcePolicySnapshot,
    WorkspaceRuntimeResources, WorkspaceRuntimeState,
};
use crate::domain::{Decision, DecisionKind, DecisionState, Repository, Workspace, WorkspacePhase};
use crate::protocol::{
    RepositorySummary, WorkspaceCloseResult, WorkspaceDeleteResult, WorkspaceDiffResult,
    WorkspaceLimitsResult, WorkspaceListItem, WorkspaceReopenResult, WorkspaceResult,
    WorkspaceRetirementPlan, WorkspaceStatusResult, WorkspaceThreadDisposition,
};

use super::style::{Palette, Tone};

mod collections;
mod usage;

pub(super) use collections::{
    print_model_list, print_repository_list, print_workspace_list, render_workspace_list_for_stdout,
};
pub(super) use usage::{
    print_workspace_usage, print_workspace_usage_list, render_workspace_usage_for_stdout,
    render_workspace_usage_list_for_stdout,
};

const PUBLIC_SCHEMA_VERSION: u64 = 10;

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

pub(super) fn print_workspace_limits(result: &WorkspaceLimitsResult) {
    print!("{}", render_workspace_limits(result, Palette::stdout()));
}

fn render_workspace_limits(result: &WorkspaceLimitsResult, palette: Palette) -> String {
    let policy = &result.policy.policy;
    let name = palette.paint(Tone::Bold, safe_line(&result.workspace.name));
    let state = limit_state(&result.policy, &result.controller);
    if state == LimitState::Unconfigured {
        return format!(
            "{name} {}\n",
            palette.paint(Tone::Dim, "· No limits configured")
        );
    }
    let label = if state == LimitState::Unsupported {
        format!(
            "Unsupported here: {}",
            result
                .controller
                .capabilities
                .unsupported_fields(policy)
                .join(", ")
        )
    } else {
        state.label().to_owned()
    };
    let mut output = format!(
        "{name} {}\n",
        palette.paint(Tone::Dim, format!("· {label}"))
    );
    if state == LimitState::PendingRestart
        && let Some(applied) = &result.controller.applied_policy
    {
        output.push_str("  Desired\n");
        output.push_str(&render_resource_policy(policy, "    "));
        output.push_str("  Current\n");
        output.push_str(&render_resource_policy(&applied.policy, "    "));
        return output;
    }
    output.push_str(&render_resource_policy(policy, "  "));
    output
}

fn render_resource_policy(policy: &WorkspaceResourcePolicy, indent: &str) -> String {
    if policy.is_empty() {
        return format!("{indent}None\n");
    }
    let mut output = String::new();
    let mut memory = Vec::with_capacity(2);
    if let Some(value) = policy.memory_high_bytes {
        memory.push(format!("high {}", format_bytes(value)));
    }
    if let Some(value) = policy.memory_max_bytes {
        memory.push(format!("max {}", format_bytes(value)));
    }
    if !memory.is_empty() {
        output.push_str(&format!("{indent}Memory  {}\n", memory.join(" · ")));
    }
    let mut cpu = Vec::with_capacity(2);
    if let Some(value) = policy.cpu_max_millicores {
        cpu.push(format!("max {}", format_cpu_cores(value)));
    }
    if let Some(value) = policy.cpu_weight {
        cpu.push(format!("weight {value}"));
    }
    if !cpu.is_empty() {
        output.push_str(&format!("{indent}CPU     {}\n", cpu.join(" · ")));
    }
    if let Some(value) = policy.tasks_max {
        output.push_str(&format!("{indent}Tasks   max {value}\n"));
    }
    output
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LimitState {
    Unconfigured,
    Unsupported,
    Applied,
    PendingRestart,
    Unknown,
    NextStart,
}

impl LimitState {
    const fn label(self) -> &'static str {
        match self {
            Self::Unconfigured => "No limits configured",
            Self::Unsupported => "Unsupported here",
            Self::Applied => "Applied now",
            Self::PendingRestart => "Applies after runtime restart",
            Self::Unknown => "Application state unknown",
            Self::NextStart => "Applies on next start",
        }
    }
}

fn limit_state(
    desired: &WorkspaceResourcePolicySnapshot,
    controller: &WorkspaceResourceControllerStatus,
) -> LimitState {
    if !controller
        .capabilities
        .unsupported_fields(&desired.policy)
        .is_empty()
    {
        return LimitState::Unsupported;
    }
    if controller.runtime_state != WorkspaceRuntimeState::Running {
        return if desired.policy.is_empty() {
            LimitState::Unconfigured
        } else {
            LimitState::NextStart
        };
    }
    match controller.applied_policy.as_ref() {
        Some(applied) if applied == desired => {
            if desired.policy.is_empty() {
                LimitState::Unconfigured
            } else {
                LimitState::Applied
            }
        }
        Some(_) => LimitState::PendingRestart,
        None => LimitState::Unknown,
    }
}

fn format_cpu_cores(millicores: u32) -> String {
    if millicores.is_multiple_of(1_000) {
        let cores = millicores / 1_000;
        format!("{cores} {}", if cores == 1 { "core" } else { "cores" })
    } else {
        let cores = f64::from(millicores) / 1_000.0;
        let formatted = format!("{cores:.3}");
        format!("{} cores", formatted.trim_end_matches('0'))
    }
}

pub(super) fn print_source_changes_omitted_warning(path: &str) {
    eprint!(
        "{}",
        render_source_changes_omitted_warning(path, Palette::stderr())
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
    if result.plan.thread_disposition == WorkspaceThreadDisposition::Retain
        && let Some(thread) = &result.plan.thread_id
    {
        println!("  Kept thread  {}", safe_line(thread));
    }
    if !result.plan.delete_branch
        && let Some(branch) = &result.plan.branch_name
    {
        println!("  Kept branch  {}", safe_line(branch));
    }
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
        "  Worktree  {} {}",
        if plan.remove_worktree {
            "remove"
        } else {
            "absent"
        },
        palette.paint(Tone::Dim, safe_line(&plan.worktree_path.to_string_lossy()))
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
    if plan.unretained_commit_count > 0 {
        println!(
            "  Commits   {} not retained by another branch or tag",
            plan.unretained_commit_count
        );
    }
    println!(
        "  Thread    {} {}",
        if plan.thread_id.is_none() {
            "none"
        } else {
            match plan.thread_disposition {
                WorkspaceThreadDisposition::Retain => "retain",
                WorkspaceThreadDisposition::Archive => "archive",
                WorkspaceThreadDisposition::Delete => "delete",
            }
        },
        safe_line(plan.thread_id.as_deref().unwrap_or("")),
    );
    println!(
        "  Branch    {} {}",
        if plan.branch_name.is_none() {
            "none"
        } else if plan.delete_branch {
            "delete"
        } else {
            "retain"
        },
        safe_line(plan.branch_name.as_deref().unwrap_or("")),
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

pub(super) fn print_status(result: &WorkspaceStatusResult, include_resources: bool) {
    print!("{}", render_status_for_stdout(result, include_resources));
}

pub(super) fn render_status_for_stdout(
    result: &WorkspaceStatusResult,
    include_resources: bool,
) -> String {
    render_status(result, None, include_resources, Palette::stdout())
}

pub(super) fn render_follow_status(
    result: &WorkspaceStatusResult,
    spinner: &str,
    include_resources: bool,
) -> String {
    render_status(result, Some(spinner), include_resources, Palette::stdout())
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

fn render_source_changes_omitted_warning(path: &str, palette: Palette) -> String {
    format!(
        "{} Local changes remain in {} and were not copied.\n",
        palette.paint(Tone::YellowBold, "!"),
        palette.paint(Tone::Bold, safe_line(path)),
    )
}

fn render_status(
    result: &WorkspaceStatusResult,
    marker_override: Option<&str>,
    include_resources: bool,
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
    if include_resources && let Some(resources) = &result.runtime_resources {
        output.push_str(&render_runtime_resources(resources, palette));
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

fn render_runtime_resources(resources: &WorkspaceRuntimeResources, palette: Palette) -> String {
    match resources.state {
        WorkspaceRuntimeState::Inactive | WorkspaceRuntimeState::Exited => {
            format!("  {}\n", palette.paint(Tone::Dim, "Resources —"))
        }
        WorkspaceRuntimeState::Running => {
            let mut parts = Vec::with_capacity(3);
            let sampling_supported = resources.process_count.is_some()
                || resources.memory_current_bytes.is_some()
                || resources.resident_memory_bytes.is_some()
                || resources.cpu_percent.is_some();
            if let Some(bytes) = resources.memory_current_bytes {
                parts.push(format!("{} memory", format_bytes(bytes)));
            } else if let Some(bytes) = resources.resident_memory_bytes {
                parts.push(format!("{} RSS", format_bytes(bytes)));
            }
            if let Some(count) = resources.process_count {
                parts.push(format!(
                    "{count} {}",
                    if count == 1 { "process" } else { "processes" }
                ));
            }
            if sampling_supported {
                parts.push(resources.cpu_percent.map_or_else(
                    || "CPU sampling".to_owned(),
                    |percent| format!("{percent:.1}% CPU"),
                ));
            }
            if parts.is_empty() {
                format!("  {}\n", palette.paint(Tone::Dim, "Resources —"))
            } else {
                format!("  {}\n", palette.paint(Tone::Dim, parts.join(" \u{b7} ")))
            }
        }
    }
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let bytes = bytes as f64;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes / KIB)
    } else {
        format!("{bytes:.0} B")
    }
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
    if let Some(path) = &workspace.worktree_path {
        parts.push(path.display().to_string());
        parts.push(
            workspace
                .branch_name
                .clone()
                .unwrap_or_else(|| "detached".to_owned()),
        );
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

    #[test]
    fn omitted_source_changes_warning_is_concise_and_safe() {
        assert_eq!(
            render_source_changes_omitted_warning("/repo\n\u{1b}[31m", Palette::plain()),
            "! Local changes remain in /repo  [31m and were not copied.\n"
        );
    }

    #[test]
    fn running_workspace_resources_are_compact_and_truthful() {
        let resources = WorkspaceRuntimeResources {
            backend: crate::domain::runtime::WorkspaceRuntimeBackend::ExecServer,
            state: WorkspaceRuntimeState::Running,
            scope: crate::domain::runtime::WorkspaceResourceScope::ProcessTree,
            process_id: Some(42),
            process_count: Some(3),
            task_count: None,
            resident_memory_bytes: Some(25 * 1024 * 1024),
            memory_current_bytes: None,
            cpu_percent: Some(12.34),
            cpu_usage_usec: None,
            cgroup_unit: None,
            events: None,
            sampled_at_ms: Some(1),
        };
        assert_eq!(
            render_runtime_resources(&resources, Palette::plain()),
            "  25.0 MiB RSS \u{b7} 3 processes \u{b7} 12.3% CPU\n"
        );
    }

    #[test]
    fn unsupported_resource_measurements_do_not_claim_to_be_sampling() {
        let resources = WorkspaceRuntimeResources {
            backend: crate::domain::runtime::WorkspaceRuntimeBackend::ExecServer,
            state: WorkspaceRuntimeState::Running,
            scope: crate::domain::runtime::WorkspaceResourceScope::RootProcess,
            process_id: Some(42),
            process_count: None,
            task_count: None,
            resident_memory_bytes: None,
            memory_current_bytes: None,
            cpu_percent: None,
            cpu_usage_usec: None,
            cgroup_unit: None,
            events: None,
            sampled_at_ms: Some(1),
        };
        assert_eq!(
            render_runtime_resources(&resources, Palette::plain()),
            "  Resources —\n"
        );
    }

    #[test]
    fn cgroup_memory_is_not_presented_as_rss() {
        let resources = WorkspaceRuntimeResources {
            backend: crate::domain::runtime::WorkspaceRuntimeBackend::ExecServer,
            state: WorkspaceRuntimeState::Running,
            scope: crate::domain::runtime::WorkspaceResourceScope::CgroupV2,
            process_id: Some(42),
            process_count: Some(3),
            task_count: Some(7),
            resident_memory_bytes: None,
            memory_current_bytes: Some(25 * 1024 * 1024),
            cpu_percent: Some(12.34),
            cpu_usage_usec: Some(500_000),
            cgroup_unit: Some("opaque.scope".to_owned()),
            events: None,
            sampled_at_ms: Some(1),
        };
        assert_eq!(
            render_runtime_resources(&resources, Palette::plain()),
            "  25.0 MiB memory \u{b7} 3 processes \u{b7} 12.3% CPU\n"
        );
    }

    #[test]
    fn limit_state_keeps_desired_and_applied_policy_distinct() {
        let capabilities = crate::domain::runtime::WorkspaceResourceCapabilities {
            backend: crate::domain::runtime::WorkspaceResourceControllerBackend::SystemdCgroupV2,
            dynamic_updates: true,
            memory_high: true,
            memory_max: true,
            cpu_max: true,
            cpu_weight: true,
            tasks_max: true,
        };
        let applied = WorkspaceResourcePolicySnapshot {
            revision: 1,
            policy: WorkspaceResourcePolicy {
                cpu_max_millicores: Some(750),
                ..WorkspaceResourcePolicy::default()
            },
        };
        let desired = WorkspaceResourcePolicySnapshot {
            revision: 2,
            policy: WorkspaceResourcePolicy::default(),
        };
        let controller = WorkspaceResourceControllerStatus {
            capabilities,
            runtime_state: WorkspaceRuntimeState::Running,
            applied_policy: Some(applied),
        };

        assert_eq!(
            limit_state(&desired, &controller),
            LimitState::PendingRestart
        );
        assert_eq!(format_cpu_cores(750), "0.75 cores");
    }
}
