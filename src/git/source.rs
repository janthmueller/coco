use std::path::PathBuf;

use crate::domain::git_source::{GitBranch, GitChangeSummary, RepositoryGitState};

use super::repository::validate_object_id;
use super::{Git, GitError, GitRepository};

impl Git {
    pub(crate) fn inspect_source(
        &self,
        repository: &GitRepository,
        include_branches: bool,
    ) -> Result<RepositoryGitState, GitError> {
        let output = self.run(
            &repository.root_path,
            "source-status",
            [
                "--no-optional-locks",
                "status",
                "--porcelain=v2",
                "--branch",
                "-z",
                "--untracked-files=all",
            ],
        )?;
        let (branch, head_sha, changes) = parse_status(&output.stdout.bytes)?;
        Ok(RepositoryGitState {
            repository_id: repository.id.clone(),
            root_path: repository.root_path.clone(),
            branch,
            head_sha,
            changes,
            branches: if include_branches {
                self.source_branches(repository)?
            } else {
                Vec::new()
            },
            base_sha: None,
        })
    }

    fn source_branches(&self, repository: &GitRepository) -> Result<Vec<GitBranch>, GitError> {
        let output = self.run(
            &repository.root_path,
            "source-branches",
            [
                "for-each-ref",
                "--format=%(refname)%00%(objectname)%00%(symref)%00%(worktreepath)%00",
                "refs/heads/",
                "refs/remotes/",
            ],
        )?;
        let text =
            std::str::from_utf8(&output.stdout.bytes).map_err(|_| GitError::NonUtf8Metadata {
                category: "source-branches",
            })?;
        parse_branches(text)
    }
}

fn malformed(category: &'static str) -> GitError {
    GitError::BindingMismatch(format!("invalid Git {category} metadata"))
}

fn parse_status(
    bytes: &[u8],
) -> Result<(Option<String>, Option<String>, GitChangeSummary), GitError> {
    let mut branch = None;
    let mut head = None;
    let mut seen_head = false;
    let mut seen_branch = false;
    let mut changes = GitChangeSummary::default();
    let mut entries = bytes
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty());
    while let Some(entry) = entries.next() {
        if let Some(value) = entry.strip_prefix(b"# branch.oid ") {
            let value = std::str::from_utf8(value).map_err(|_| malformed("source-status"))?;
            if value != "(initial)" {
                validate_object_id(value)?;
                head = Some(value.to_owned());
            }
            seen_head = true;
        } else if let Some(value) = entry.strip_prefix(b"# branch.head ") {
            let value = std::str::from_utf8(value).map_err(|_| malformed("source-status"))?;
            if value != "(detached)" {
                branch = Some(value.to_owned());
            }
            seen_branch = true;
        } else if entry.starts_with(b"# ") {
            continue;
        } else if entry.starts_with(b"? ") {
            changes.untracked_files += 1;
        } else if entry.starts_with(b"1 ") || entry.starts_with(b"u ") {
            changes.tracked_files += 1;
        } else if entry.starts_with(b"2 ") {
            changes.tracked_files += 1;
            entries.next().ok_or_else(|| malformed("rename"))?;
        } else {
            return Err(malformed("source-status"));
        }
    }
    if !seen_head || !seen_branch {
        return Err(malformed("source-status"));
    }
    Ok((branch, head, changes))
}

fn parse_branches(mut text: &str) -> Result<Vec<GitBranch>, GitError> {
    let mut branches = Vec::new();
    while !text.is_empty() {
        let mut field = || {
            let (value, rest) = text
                .split_once('\0')
                .ok_or_else(|| malformed("source-branches"))?;
            text = rest;
            Ok::<_, GitError>(value)
        };
        let reference = field()?;
        let commit = field()?;
        let symbolic = field()?;
        let path = field()?;
        text = text
            .strip_prefix('\n')
            .ok_or_else(|| malformed("source-branches"))?;
        if !symbolic.is_empty() {
            continue;
        }
        validate_object_id(commit)?;
        let (name, remote) = if let Some(name) = reference.strip_prefix("refs/heads/") {
            (name, false)
        } else if let Some(name) = reference.strip_prefix("refs/remotes/") {
            (name, true)
        } else {
            return Err(malformed("source-branches"));
        };
        branches.push(GitBranch {
            name: name.to_owned(),
            reference: reference.to_owned(),
            commit: commit.to_owned(),
            remote,
            checkout_path: (!path.is_empty()).then(|| PathBuf::from(path)),
        });
    }
    Ok(branches)
}

#[cfg(test)]
mod tests;
