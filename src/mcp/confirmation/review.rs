use super::*;
use crate::protocol::{
    TurnInterruptMode, WorkspaceBaseRequest, WorkspaceSetupContext, WorkspaceSetupRequest,
    WorkspaceWorktreeRequest,
};

pub(crate) fn create_review(repository_path: &Path, request: &WorkspaceSetupRequest) -> String {
    let (worktree, base) = match &request.worktree {
        WorkspaceWorktreeRequest::NewBranch { branch, base } => (
            format!("new branch {}", branch.as_deref().unwrap_or(&request.name)),
            base_label(base),
        ),
        WorkspaceWorktreeRequest::Detached { base } => ("detached".into(), base_label(base)),
        WorkspaceWorktreeRequest::ExistingBranch { branch } => {
            (format!("existing branch {branch}"), branch.clone())
        }
    };
    let context = match &request.context {
        WorkspaceSetupContext::Fresh => "fresh".into(),
        WorkspaceSetupContext::Fork { workspace, compact } => format!(
            "workspace {workspace}{}",
            if *compact {
                "; compact child on first use"
            } else {
                ""
            }
        ),
    };
    let mut review = format!("Create workspace\n\n{}", repository(repository_path));
    for (label, value) in [
        ("Workspace", request.name.as_str()),
        ("Worktree", &worktree),
        ("Code base", &base),
        ("Context", &context),
        ("Git changes", changes_label(request.changes)),
        ("Profile", &request.profile),
        (
            "Model",
            request.model.as_deref().unwrap_or("configured model"),
        ),
        ("After create", "prepare only"),
    ] {
        review.push_str(&field(label, value));
    }
    if let Some(binding) = &request.worker_mcp {
        review.push_str(&field("Worker tools", &binding.path.to_string_lossy()));
        review.push_str(&field("Source hash", &binding.sha256));
    }
    review
}

pub(crate) fn send_review(repository_path: &Path, input: &WorkspaceSendInput) -> String {
    format!(
        "Send instructions\n\n{}{}{}\nStarts agent work with this workspace's permissions and account usage.",
        repository(repository_path),
        field("Workspace", &input.workspace),
        field("Message", &input.message),
    )
}

pub(crate) fn interrupt_review(
    path: &Path,
    workspace: &str,
    operation: &str,
    mode: TurnInterruptMode,
) -> String {
    format!(
        "Interrupt turn\n\n{}{}{}{}{}",
        repository(path),
        field("Workspace", workspace),
        field("Operation", operation),
        field(
            "Mode",
            match mode {
                TurnInterruptMode::Strict => "strict",
                TurnInterruptMode::BestEffort => "bestEffort",
            }
        ),
        if mode == TurnInterruptMode::BestEffort {
            "\nBest effort: a concurrently started newer turn could be interrupted instead. This request does not confirm a stopped turn or undo changes."
        } else {
            ""
        }
    )
}

fn base_label(base: &WorkspaceBaseRequest) -> String {
    match base {
        WorkspaceBaseRequest::Revision { revision } => revision.clone(),
        WorkspaceBaseRequest::Workspace { workspace } => {
            format!("workspace {workspace} (committed)")
        }
    }
}

fn changes_label(changes: crate::protocol::WorkspaceChangesRequest) -> &'static str {
    use crate::protocol::WorkspaceChangesRequest::*;
    match changes {
        Reject => "require clean source",
        Ignore => "do not copy changes",
        CarryTracked => "copy tracked changes",
        CarryTrackedAndUntracked => "copy tracked + untracked changes",
    }
}
