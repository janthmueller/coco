use super::*;
use crate::domain::Workspace;
use crate::protocol::{WorkspaceRetirementPlan, WorkspaceThreadDisposition};

pub(crate) fn retirement_review(
    action: &str,
    path: &Path,
    plan: &WorkspaceRetirementPlan,
    discard_changes: bool,
    discard_commits: bool,
) -> String {
    let mut review = format!("{action} workspace\n\n{}", repository(path));
    for (label, value) in [
        ("Workspace", plan.workspace_name.clone()),
        ("Workspace ID", plan.workspace_id.clone()),
        (
            "Worktree",
            plan.worktree_path.to_string_lossy().into_owned(),
        ),
        ("Remove files", plan.remove_worktree.to_string()),
        (
            "HEAD",
            plan.head_sha.clone().unwrap_or_else(|| "none".into()),
        ),
        (
            "Branch",
            plan.branch_name.clone().unwrap_or_else(|| "none".into()),
        ),
        ("Delete branch", plan.delete_branch.to_string()),
        (
            "Thread ID",
            plan.thread_id.clone().unwrap_or_else(|| "none".into()),
        ),
        (
            "Conversation",
            match plan.thread_disposition {
                WorkspaceThreadDisposition::Retain => "keep",
                WorkspaceThreadDisposition::Archive => "archive",
                WorkspaceThreadDisposition::Delete => "delete",
            }
            .into(),
        ),
        ("Tracked edits", plan.tracked_changes.to_string()),
        ("Untracked", plan.untracked_file_count.to_string()),
        ("Ignored", plan.ignored_file_count.to_string()),
        ("Commit loss", plan.unretained_commit_count.to_string()),
        ("Discard files", discard_changes.to_string()),
        ("Discard commits", discard_commits.to_string()),
    ] {
        review.push_str(&field(label, &value));
    }
    if action == "Delete" {
        review.push_str("\nPermanently removes CoCo turn-operation/outcome receipts, even when the conversation is kept. No rollback or interruption of active work.");
    }
    review
}

pub(crate) fn reopen_review(path: &Path, workspace: &Workspace) -> String {
    let mut review = format!("Reopen workspace\n\n{}", repository(path));
    for (label, value) in [
        ("Workspace", workspace.name.clone()),
        ("Workspace ID", workspace.id.clone()),
        (
            "Worktree",
            workspace
                .worktree_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
        ),
        (
            "Branch",
            workspace
                .branch_name
                .clone()
                .unwrap_or_else(|| "detached".into()),
        ),
        (
            "HEAD",
            workspace.closed_head_sha.clone().unwrap_or_default(),
        ),
    ] {
        review.push_str(&field(label, &value));
    }
    review.push_str(
        "\nRestores the saved worktree and conversation binding. Does not start an agent turn.",
    );
    review
}
