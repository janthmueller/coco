use tokio::time::{Instant, timeout_at};

use crate::diagnostics::{PROBE_TIMEOUT, ProbeError};
use crate::domain::{WorkspaceAvailability, WorkspaceLifecycle, WorktreeMode};
use crate::protocol::{DiagnosticCheck, DiagnosticStatus};
use crate::store::{DiagnosticSnapshot, DiagnosticWorkspaceBinding, SUPPORTED_SCHEMA_VERSION};

use super::Coordinator;

pub(crate) struct DoctorChecks {
    pub(crate) checks: Vec<DiagnosticCheck>,
    pub(crate) complete: bool,
}

impl Coordinator {
    pub(crate) async fn doctor_checks(&self, deadline: Instant) -> DoctorChecks {
        let mut result = DoctorChecks {
            checks: Vec::new(),
            complete: true,
        };
        let native_available = self.doctor_native(deadline, &mut result).await;
        let snapshot = match timeout_at(probe_deadline(deadline), self.store.diagnostic_snapshot())
            .await
        {
            Ok(Ok(snapshot)) => snapshot,
            response => {
                let error = match response {
                    Ok(Err(error)) => error,
                    _ => ProbeError::TimedOut,
                };
                result.checks.push(probe_failure("state", "Saved state", error)
                    .hint("Try again when the coordinator is idle. If this persists, back up CoCo's data before investigating."));
                result.checks.push(DiagnosticCheck::new(
                    "bindings",
                    "Workspace bindings",
                    DiagnosticStatus::Skipped,
                    "Saved state could not be inspected",
                ));
                result.complete = false;
                return result;
            }
        };
        inspect_snapshot(&snapshot, &mut result);
        self.doctor_bindings(&snapshot, native_available, deadline, &mut result)
            .await;
        result.complete &= !result.checks.iter().any(|check| check.timed_out);
        result
    }

    async fn doctor_native(&self, deadline: Instant, result: &mut DoctorChecks) -> bool {
        match timeout_at(probe_deadline(deadline), self.worker.list_models()).await {
            Ok(Ok(_)) => {
                result.checks.push(DiagnosticCheck::new(
                    "codex.api",
                    "Codex connection",
                    DiagnosticStatus::Ok,
                    "App Server responds to model discovery",
                ));
                true
            }
            response => {
                let timed_out = response.is_err();
                let message = if timed_out {
                    "App Server did not respond within its deadline"
                } else {
                    "App Server model discovery failed"
                };
                result.complete = false;
                let mut check = DiagnosticCheck::new("codex.api", "Codex connection", DiagnosticStatus::Error, message)
                    .hint("Check the coordinator's log and Codex configuration. Stop active work before restarting cocod.");
                if timed_out {
                    check = check.timed_out();
                }
                result.checks.push(check);
                false
            }
        }
    }

    async fn doctor_bindings(
        &self,
        snapshot: &DiagnosticSnapshot,
        native_available: bool,
        deadline: Instant,
        result: &mut DoctorChecks,
    ) {
        for repository in &snapshot.repositories {
            if Instant::now() >= deadline {
                incomplete(result);
                return;
            }
            let response = timeout_at(
                probe_deadline(deadline),
                self.git.diagnostic_repository(&repository.root_path),
            )
            .await;
            let check = match response {
                Ok(Ok(binding)) if binding.common_dir == repository.git_common_dir => {
                    DiagnosticCheck::new(
                        "repository",
                        "Repository",
                        DiagnosticStatus::Ok,
                        "Git binding is valid",
                    )
                }
                Ok(Ok(_)) => DiagnosticCheck::new(
                    "repository",
                    "Repository",
                    DiagnosticStatus::Error,
                    "Path now belongs to a different Git repository",
                )
                .hint("Restore the original repository path before using its workspaces."),
                Ok(Err(error)) => probe_failure("repository", "Repository", error)
                    .hint("Check that the registered repository still exists and is accessible."),
                Err(_) => probe_failure("repository", "Repository", ProbeError::TimedOut),
            };
            result.checks.push(
                check
                    .subject(repository.root_path.display().to_string())
                    .subject_id(&repository.id),
            );
        }
        for workspace in &snapshot.workspaces {
            if Instant::now() >= deadline {
                incomplete(result);
                return;
            }
            let subject = format!(
                "{}: {}",
                workspace.repository_path.display(),
                workspace.name
            );
            let worktree = self.doctor_worktree(workspace, deadline).await;
            result
                .checks
                .push(worktree.subject(subject.clone()).subject_id(&workspace.id));
            if Instant::now() >= deadline {
                incomplete(result);
                return;
            }
            let thread = self
                .doctor_thread(workspace, native_available, deadline)
                .await;
            result
                .checks
                .push(thread.subject(subject).subject_id(&workspace.id));
        }
    }

    async fn doctor_worktree(
        &self,
        workspace: &DiagnosticWorkspaceBinding,
        deadline: Instant,
    ) -> DiagnosticCheck {
        if workspace.availability == WorkspaceAvailability::Closed {
            return DiagnosticCheck::new(
                "worktree",
                "Worktree",
                DiagnosticStatus::Skipped,
                "Closed; no checkout expected",
            );
        }
        if workspace.availability != WorkspaceAvailability::Open
            || workspace.lifecycle != WorkspaceLifecycle::Ready
        {
            return DiagnosticCheck::new(
                "worktree",
                "Worktree",
                DiagnosticStatus::Warning,
                "Workspace is in a lifecycle transition or failed preparation",
            )
            .hint("Inspect it with coco status before changing or removing anything.");
        }
        let Some(path) = &workspace.worktree_path else {
            return DiagnosticCheck::new(
                "worktree",
                "Worktree",
                DiagnosticStatus::Error,
                "Open workspace has no recorded checkout",
            );
        };
        let inspected = timeout_at(probe_deadline(deadline), async {
            let expected = tokio::fs::canonicalize(path)
                .await
                .map_err(|_| ProbeError::Unavailable)?;
            self.git
                .diagnostic_binding(path)
                .await
                .map(|binding| (expected, binding))
        })
        .await;
        match inspected {
            Ok(Ok((expected, binding))) => {
                let branch_matches = match workspace.worktree_mode {
                    WorktreeMode::Detached => binding.branch_name.is_none(),
                    _ => binding.branch_name == workspace.branch_name,
                };
                if binding.root_path == expected
                    && binding.common_dir == workspace.git_common_dir
                    && branch_matches
                {
                    DiagnosticCheck::new(
                        "worktree",
                        "Worktree",
                        DiagnosticStatus::Ok,
                        "Checkout and branch match",
                    )
                } else {
                    DiagnosticCheck::new("worktree", "Worktree", DiagnosticStatus::Error, "Checkout no longer matches its saved Git binding")
                        .hint("Check the worktree path and branch. Do not remove files to repair the binding blindly.")
                }
            }
            Ok(Err(error)) => probe_failure("worktree", "Worktree", error)
                .hint("Check that the workspace checkout still exists and is accessible."),
            Err(_) => probe_failure("worktree", "Worktree", ProbeError::TimedOut),
        }
    }

    async fn doctor_thread(
        &self,
        workspace: &DiagnosticWorkspaceBinding,
        native_available: bool,
        deadline: Instant,
    ) -> DiagnosticCheck {
        let Some(thread_id) = &workspace.thread_id else {
            return DiagnosticCheck::new(
                "thread",
                "Conversation",
                DiagnosticStatus::Skipped,
                "Prepared; no saved conversation yet",
            );
        };
        if !native_available {
            return DiagnosticCheck::new(
                "thread",
                "Conversation",
                DiagnosticStatus::Skipped,
                "Codex connection is unavailable",
            );
        }
        let located = timeout_at(
            probe_deadline(deadline),
            self.worker.locate_thread(thread_id),
        )
        .await;
        match located {
            Ok(Ok(Some(located))) => {
                if located.thread.id != *thread_id || workspace.worktree_path.as_ref().is_some_and(|path| path != &located.thread.cwd) {
                    return DiagnosticCheck::new("thread", "Conversation", DiagnosticStatus::Error, "Saved conversation points to a different workspace")
                        .hint("Verify the Codex thread ID and its directory before continuing.");
                }
                if located.archived != workspace.thread_archived {
                    return DiagnosticCheck::new("thread", "Conversation", DiagnosticStatus::Warning, "Codex archive state changed outside CoCo")
                        .hint("Inspect the workspace with coco status before reopening or deleting it.");
                }
                DiagnosticCheck::new("thread", "Conversation", DiagnosticStatus::Ok, "Saved conversation is readable")
            }
            Ok(Ok(None)) => DiagnosticCheck::new("thread", "Conversation", DiagnosticStatus::Error, "Saved conversation was not found in the coordinator's Codex home")
                .hint("Check CODEX_HOME and whether the Codex conversation was moved or deleted."),
            Ok(Err(_)) => DiagnosticCheck::new("thread", "Conversation", DiagnosticStatus::Error, "Codex could not inspect the saved conversation")
                .hint("Check the coordinator's log and the Codex conversation without deleting its history."),
            Err(_) => probe_failure("thread", "Conversation", ProbeError::TimedOut)
                .hint("The conversation may be large or Codex may be busy. Try again and inspect the coordinator's log."),
        }
    }
}

fn inspect_snapshot(snapshot: &DiagnosticSnapshot, result: &mut DoctorChecks) {
    let healthy = snapshot.schema_version == SUPPORTED_SCHEMA_VERSION && snapshot.integrity_ok;
    let mut state = DiagnosticCheck::new(
        "state",
        "Saved state",
        if healthy {
            DiagnosticStatus::Ok
        } else {
            DiagnosticStatus::Error
        },
        if healthy {
            "Database checks passed"
        } else {
            "Database schema or integrity checks failed"
        },
    );
    if !healthy {
        state = state
            .hint("Back up CoCo's data before investigating. Doctor does not repair saved state.");
    }
    result.checks.push(state);
    if snapshot.repository_count > snapshot.repositories.len()
        || snapshot.workspace_count > snapshot.workspaces.len()
    {
        result.complete = false;
        result.checks.push(
            DiagnosticCheck::new(
                "coverage",
                "Coverage",
                DiagnosticStatus::Warning,
                format!(
                    "Checking {} of {} repositories and {} of {} workspaces",
                    snapshot.repositories.len(),
                    snapshot.repository_count,
                    snapshot.workspaces.len(),
                    snapshot.workspace_count
                ),
            )
            .hint("Use coco status -a to inspect the remaining workspaces."),
        );
    }
}

fn probe_deadline(deadline: Instant) -> Instant {
    deadline.min(Instant::now() + PROBE_TIMEOUT)
}

fn incomplete(result: &mut DoctorChecks) {
    result.complete = false;
    result.checks.push(
        DiagnosticCheck::new(
            "coverage.timeout",
            "Coverage",
            DiagnosticStatus::Warning,
            "Some bindings were not checked within the 20s report budget",
        )
        .hint("Try again when Codex responds, or inspect individual workspaces with coco status."),
    );
}

fn probe_failure(id: &str, label: &str, error: ProbeError) -> DiagnosticCheck {
    let message = match error {
        ProbeError::TimedOut => "Check did not finish within its deadline",
        ProbeError::Unavailable => "Required file, command, or state is unavailable",
        ProbeError::Failed => "Check failed",
        ProbeError::InvalidOutput => "Command returned invalid metadata",
    };
    let check = DiagnosticCheck::new(id, label, DiagnosticStatus::Error, message);
    if error == ProbeError::TimedOut {
        check.timed_out()
    } else {
        check
    }
}
