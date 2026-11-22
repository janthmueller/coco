use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tokio::process::Command;

use crate::diagnostics::{ProbeError, capture_command};
use crate::domain::WorktreeMode;
use crate::domain::git_tracking::{GitTracking, GitTrackingState};

use super::Git;
use super::repository::{repository_id, validate_object_id};

mod binding;
mod parse;

#[derive(Debug)]
pub(crate) struct TrackingSnapshot {
    branches: HashMap<String, TrackingBranch>,
    worktrees: HashMap<PathBuf, TrackingWorktree>,
    observed_at_ms: i64,
}

#[derive(Debug)]
struct TrackingBranch {
    commit: String,
    tracking: GitTracking,
}

#[derive(Debug)]
struct TrackingWorktree {
    commit: String,
    branch: Option<String>,
    main: bool,
}

impl Git {
    /// Supplementary metadata only: no index scan, hooks, fetch or lazy fetch.
    /// Callers impose one budget on the whole operation, not per workspace.
    pub(crate) async fn tracking_snapshot(
        &self,
        path: &Path,
        expected_repository: &str,
    ) -> Result<TrackingSnapshot, ProbeError> {
        let repository = self.diagnostic_repository(path).await?;
        if repository_id(&repository.common_dir) != expected_repository {
            return Err(ProbeError::InvalidOutput);
        }
        let mut command = self.tracking_command(&repository.root_path);
        command.args([
            "for-each-ref",
            "--format=%(refname)%00%(objectname)%00%(symref)%00%(upstream)%00%(upstream:remotename)%00%(upstream:track,nobracket)%00",
            "refs/heads/",
        ]);
        let branches = parse::branches(&capture_command(command).await?)?;
        let mut command = self.tracking_command(&repository.root_path);
        command.args(["worktree", "list", "--porcelain", "-z"]);
        let worktrees = parse::worktrees(&capture_command(command).await?)?;
        let worktrees = binding::verified(worktrees, &repository.common_dir).await;
        // A separate-git-dir root can expose valid refs without a matching
        // native main-worktree entry. This fallback never invents such an entry.
        if !worktrees.contains_key(&repository.root_path)
            && !binding::registered(&repository.root_path, &repository.common_dir, true).await
        {
            return Err(ProbeError::InvalidOutput);
        }
        // Counts from an incomplete history cannot be presented as exact.
        // Reading this metadata file cannot cause a partial-clone lazy fetch.
        let shallow = repository.common_dir.join("shallow");
        let incomplete = match tokio::fs::metadata(shallow).await {
            Ok(metadata) => metadata.len() > 0,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(_) => return Err(ProbeError::Unavailable),
        };
        let mut snapshot = TrackingSnapshot {
            branches,
            worktrees,
            observed_at_ms: chrono::Utc::now().timestamp_millis(),
        };
        if incomplete {
            for branch in snapshot.branches.values_mut() {
                if branch.tracking.ahead_by.is_some() {
                    branch.tracking.state = GitTrackingState::Unknown;
                    branch.tracking.ahead_by = None;
                    branch.tracking.behind_by = None;
                }
            }
        }
        Ok(snapshot)
    }

    fn tracking_command(&self, path: &Path) -> Command {
        let mut command = Command::new(&self.executable);
        command
            .current_dir(path)
            .args(["--no-optional-locks", "--no-replace-objects"])
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_NO_LAZY_FETCH", "1")
            .env("LC_ALL", "C");
        for name in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_CONFIG_COUNT",
            "GIT_CONFIG_GLOBAL",
            "GIT_CONFIG_SYSTEM",
            "GIT_COMMON_DIR",
            "GIT_NAMESPACE",
            "GIT_CONFIG_PARAMETERS",
        ] {
            command.env_remove(name);
        }
        command
    }
}

impl TrackingSnapshot {
    pub(crate) fn branch(&self, reference: &str, commit: &str) -> GitTracking {
        self.branches
            .get(reference)
            .filter(|branch| branch.commit == commit)
            .map(|branch| self.observed(branch.tracking.clone()))
            .unwrap_or_else(GitTracking::unknown)
    }

    pub(crate) fn worktree(
        &self,
        path: &Path,
        mode: WorktreeMode,
        branch: Option<&str>,
        expected_commit: Option<&str>,
    ) -> GitTracking {
        let Some(worktree) = self.worktrees.get(path) else {
            return GitTracking::unknown();
        };
        if expected_commit.is_some_and(|expected| expected != worktree.commit) {
            return GitTracking::unknown();
        }
        match (mode, branch, worktree.branch.as_deref()) {
            (WorktreeMode::Detached, None, None) => {
                self.observed(GitTracking::without_counts(GitTrackingState::Detached))
            }
            (
                WorktreeMode::NewBranch | WorktreeMode::ExistingBranch,
                Some(expected),
                Some(actual),
            ) if actual == format!("refs/heads/{expected}") => {
                self.branch(actual, &worktree.commit)
            }
            _ => GitTracking::unknown(),
        }
    }

    fn observed(&self, mut tracking: GitTracking) -> GitTracking {
        tracking.observed_at_ms = Some(self.observed_at_ms);
        tracking
    }
}

#[cfg(test)]
mod tests;
