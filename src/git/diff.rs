use std::ffi::OsString;
#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};

use crate::domain::{BaseRelation, GitObservation};

use super::command::{command_failed, ensure_success};
use super::repository::{canonicalize, validate_object_id};
use super::{DiffResult, Git, GitError, GitRepository};

impl Git {
    pub fn diff(
        &self,
        worktree_path: impl AsRef<Path>,
        base_sha: &str,
    ) -> Result<DiffResult, GitError> {
        validate_object_id(base_sha)?;
        let path = canonicalize(worktree_path.as_ref())?;
        let patch = self.execute(
            &path,
            "diff",
            [
                OsString::from("diff"),
                OsString::from("--no-ext-diff"),
                OsString::from("--binary"),
                OsString::from(base_sha),
                OsString::from("--"),
            ],
        )?;
        ensure_success("diff", &patch)?;
        let untracked_paths = self.untracked_paths(&path)?;
        Ok(DiffResult {
            tracked_patch: patch.stdout.bytes,
            tracked_patch_truncated: patch.stdout.truncated,
            untracked_paths,
        })
    }

    pub fn observe(
        &self,
        repository: &GitRepository,
        worktree_path: impl AsRef<Path>,
        expected_branch: &str,
        base_sha: &str,
    ) -> Result<GitObservation, GitError> {
        validate_object_id(base_sha)?;
        let binding = self.verify_worktree(repository, worktree_path, expected_branch)?;
        let status = self.run(
            &binding.path,
            "status",
            ["status", "--porcelain=v1", "-z", "--untracked-files=all"],
        )?;
        let untracked_paths = self.untracked_paths(&binding.path)?;
        let counts = self.run_text(
            &binding.path,
            "rev-list-count",
            [
                OsString::from("rev-list"),
                OsString::from("--left-right"),
                OsString::from("--count"),
                OsString::from(format!("{base_sha}...HEAD")),
            ],
        )?;
        let (behind_by, ahead_by) = parse_counts(&counts)?;
        let base_relation = if binding.head_sha == base_sha {
            BaseRelation::AtBase
        } else if self.is_ancestor(&binding.path, base_sha, "HEAD")? {
            BaseRelation::Descendant
        } else {
            BaseRelation::Diverged
        };

        Ok(GitObservation {
            observed: true,
            canonical_path: Some(binding.path),
            branch_name: Some(binding.branch_name),
            head_sha: Some(binding.head_sha),
            base_sha: base_sha.to_owned(),
            dirty: !status.stdout.bytes.is_empty(),
            ahead_by: Some(ahead_by),
            behind_by: Some(behind_by),
            base_relation,
            binding_valid: true,
            untracked_paths,
        })
    }

    fn untracked_paths(&self, worktree_path: &Path) -> Result<Vec<PathBuf>, GitError> {
        let output = self.run(
            worktree_path,
            "untracked-paths",
            ["ls-files", "--others", "--exclude-standard", "-z"],
        )?;
        Ok(output
            .stdout
            .bytes
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
            .map(bytes_to_path)
            .collect())
    }

    fn is_ancestor(&self, cwd: &Path, ancestor: &str, tip: &str) -> Result<bool, GitError> {
        let output = self.execute(
            cwd,
            "merge-base",
            [
                OsString::from("merge-base"),
                OsString::from("--is-ancestor"),
                OsString::from(ancestor),
                OsString::from(tip),
            ],
        )?;
        match output.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(command_failed("merge-base", &output)),
        }
    }
}

fn parse_counts(value: &str) -> Result<(u64, u64), GitError> {
    let mut counts = value.split_whitespace();
    let behind = counts.next().and_then(|part| part.parse().ok());
    let ahead = counts.next().and_then(|part| part.parse().ok());
    match (behind, ahead, counts.next()) {
        (Some(behind), Some(ahead), None) => Ok((behind, ahead)),
        _ => Err(GitError::BindingMismatch(format!(
            "invalid rev-list counts: {value:?}"
        ))),
    }
}

#[cfg(unix)]
fn bytes_to_path(bytes: &[u8]) -> PathBuf {
    PathBuf::from(OsString::from_vec(bytes.to_vec()))
}

#[cfg(not(unix))]
fn bytes_to_path(bytes: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
}
