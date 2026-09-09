use std::ffi::OsString;
use std::fs;
use std::path::Path;

use super::repository::{canonicalize, validate_object_id};
use super::worktree::{secure_directory, validate_workspace_name, workspace_path};
use super::{
    Git, GitError, GitRepository, WorktreeBinding, WorktreeMode, WorktreeRetirementObservation,
};

impl Git {
    pub fn validate_managed_worktree_path(
        &self,
        repository: &GitRepository,
        worktrees_root: &Path,
        workspace_name: &str,
        stored_path: &Path,
    ) -> Result<(), GitError> {
        validate_workspace_name(workspace_name)?;
        let root = canonicalize(worktrees_root)?;
        let expected = root.join(&repository.id).join(workspace_name);
        if expected != stored_path {
            return Err(GitError::BindingMismatch(format!(
                "stored path {} does not equal managed path {}",
                stored_path.display(),
                expected.display()
            )));
        }
        // The leaf may be absent for a closed workspace. Existing parent
        // components must still be directories at their original locations.
        let relative = stored_path
            .strip_prefix(&root)
            .expect("checked managed prefix")
            .to_owned();
        let mut current = root;
        for component in &relative {
            current.push(component);
            match fs::symlink_metadata(&current) {
                Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
                Ok(_) => {
                    return Err(GitError::BindingMismatch(format!(
                        "managed worktree path contains a non-directory or symlink: {}",
                        current.display()
                    )));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                Err(source) => {
                    return Err(GitError::Io {
                        path: current,
                        source,
                    });
                }
            }
        }
        Ok(())
    }

    pub fn observe_worktree_retirement(
        &self,
        repository: &GitRepository,
        worktree_path: impl AsRef<Path>,
        expected_mode: WorktreeMode,
        expected_branch: Option<&str>,
    ) -> Result<WorktreeRetirementObservation, GitError> {
        let binding =
            self.verify_worktree(repository, worktree_path, expected_mode, expected_branch)?;
        let status = self.run(
            &binding.path,
            "retirement-status",
            ["status", "--porcelain=v1", "-z", "--untracked-files=all"],
        )?;
        let tracked_changes = status
            .stdout
            .bytes
            .split(|byte| *byte == 0)
            .any(|record| record.len() >= 3 && &record[..2] != b"??");
        let untracked_file_count = self
            .run(
                &binding.path,
                "retirement-untracked",
                ["ls-files", "--others", "--exclude-standard", "-z"],
            )?
            .stdout
            .bytes
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
            .count();
        let ignored_file_count = self
            .run(
                &binding.path,
                "retirement-ignored",
                [
                    "ls-files",
                    "--others",
                    "--ignored",
                    "--exclude-standard",
                    "-z",
                ],
            )?
            .stdout
            .bytes
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
            .count();
        let lock_reason = self.worktree_lock_reason(repository, &binding.path)?;
        Ok(WorktreeRetirementObservation {
            binding,
            tracked_changes,
            untracked_file_count,
            ignored_file_count,
            lock_reason,
        })
    }

    pub fn remove_worktree(
        &self,
        repository: &GitRepository,
        binding: &WorktreeBinding,
        discard_changes: bool,
    ) -> Result<(), GitError> {
        let current = self.verify_worktree(
            repository,
            &binding.path,
            binding.mode,
            binding.branch_name.as_deref(),
        )?;
        if current.head_sha != binding.head_sha {
            return Err(GitError::BindingMismatch(format!(
                "worktree HEAD moved from {} to {}",
                binding.head_sha, current.head_sha
            )));
        }
        if let Some(reason) = self.worktree_lock_reason(repository, &current.path)? {
            return Err(GitError::WorktreeLocked {
                reason: format!(": {reason}"),
            });
        }
        if !discard_changes {
            let observation = self.observe_worktree_retirement(
                repository,
                &current.path,
                current.mode,
                current.branch_name.as_deref(),
            )?;
            if observation.binding.head_sha != binding.head_sha {
                return Err(GitError::BindingMismatch(format!(
                    "worktree HEAD moved from {} to {}",
                    binding.head_sha, observation.binding.head_sha
                )));
            }
            if let Some(reason) = observation.lock_reason {
                return Err(GitError::WorktreeLocked {
                    reason: format!(": {reason}"),
                });
            }
            if observation.has_local_changes() {
                return Err(GitError::DirtyWorktree(current.path));
            }
        }
        let mut arguments = vec![OsString::from("worktree"), OsString::from("remove")];
        if discard_changes {
            arguments.push(OsString::from("--force"));
        }
        arguments.push(OsString::from("--"));
        arguments.push(current.path.as_os_str().to_owned());
        self.run(&repository.root_path, "worktree-remove", arguments)?;
        if fs::symlink_metadata(&current.path).is_ok()
            || self.worktree_record(repository, &current.path)?.is_some()
        {
            return Err(GitError::BindingMismatch(
                "Git reported success but the worktree still exists".to_owned(),
            ));
        }
        Ok(())
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "reopening revalidates every immutable stored worktree binding field"
    )]
    pub fn restore_worktree(
        &self,
        repository: &GitRepository,
        worktrees_root: &Path,
        workspace_name: &str,
        stored_path: &Path,
        mode: WorktreeMode,
        branch_name: Option<&str>,
        base_sha: &str,
        closed_head_sha: &str,
    ) -> Result<WorktreeBinding, GitError> {
        self.validate_managed_worktree_path(
            repository,
            worktrees_root,
            workspace_name,
            stored_path,
        )?;
        validate_workspace_name(workspace_name)?;
        validate_object_id(base_sha)?;
        validate_object_id(closed_head_sha)?;
        let root = secure_directory(worktrees_root)?;
        let repository_root = secure_directory(&root.join(&repository.id))?;
        let expected_path = workspace_path(&repository_root, workspace_name)?;
        if expected_path != stored_path {
            return Err(GitError::BindingMismatch(format!(
                "stored path {} does not equal managed path {}",
                stored_path.display(),
                expected_path.display()
            )));
        }
        if fs::symlink_metadata(stored_path).is_ok() {
            return Err(GitError::DestinationExists(stored_path.to_owned()));
        }

        let mut arguments = vec![
            OsString::from("worktree"),
            OsString::from("add"),
            OsString::from("--no-guess-remote"),
        ];
        match (mode, branch_name) {
            (WorktreeMode::NewBranch | WorktreeMode::ExistingBranch, Some(branch)) => {
                let actual = self.resolve_local_branch(repository, branch)?;
                if actual != closed_head_sha {
                    return Err(GitError::BranchMoved {
                        branch: branch.to_owned(),
                        expected: closed_head_sha.to_owned(),
                        actual,
                    });
                }
                if let Some(path) = self.branch_checkout_path(repository, branch)? {
                    return Err(GitError::BranchAlreadyCheckedOut {
                        branch: branch.to_owned(),
                        path,
                    });
                }
            }
            (WorktreeMode::Detached, None) => arguments.push(OsString::from("--detach")),
            _ => {
                return Err(GitError::BindingMismatch(
                    "worktree mode and branch binding disagree".to_owned(),
                ));
            }
        }
        arguments.push(OsString::from("--"));
        arguments.push(stored_path.as_os_str().to_owned());
        arguments.push(OsString::from(branch_name.unwrap_or(closed_head_sha)));
        self.run(&repository.root_path, "worktree-restore", arguments)?;

        let binding = self.verify_worktree(repository, stored_path, mode, branch_name)?;
        if binding.head_sha != closed_head_sha {
            return Err(GitError::BindingMismatch(format!(
                "restored HEAD {} does not equal closed HEAD {closed_head_sha}",
                binding.head_sha
            )));
        }
        Ok(binding)
    }

    pub fn delete_created_branch(
        &self,
        repository: &GitRepository,
        branch_name: &str,
        expected_head_sha: &str,
    ) -> Result<(), GitError> {
        self.validate_created_branch_deletion(repository, branch_name, expected_head_sha)?;
        self.run(
            &repository.root_path,
            "branch-delete",
            ["branch", "-D", "--", branch_name],
        )?;
        match self.resolve_local_branch(repository, branch_name) {
            Err(GitError::BranchNotFound(_)) => Ok(()),
            Ok(actual) => Err(GitError::BindingMismatch(format!(
                "Git reported success but branch {branch_name} still exists at {actual}"
            ))),
            Err(source) => Err(source),
        }
    }

    pub fn validate_created_branch_deletion(
        &self,
        repository: &GitRepository,
        branch_name: &str,
        expected_head_sha: &str,
    ) -> Result<(), GitError> {
        let actual = self.resolve_local_branch(repository, branch_name)?;
        if actual != expected_head_sha {
            return Err(GitError::BranchMoved {
                branch: branch_name.to_owned(),
                expected: expected_head_sha.to_owned(),
                actual,
            });
        }
        if let Some(path) = self.branch_checkout_path(repository, branch_name)? {
            return Err(GitError::BranchAlreadyCheckedOut {
                branch: branch_name.to_owned(),
                path,
            });
        }
        Ok(())
    }

    pub fn delete_created_branch_if_present(
        &self,
        repository: &GitRepository,
        branch_name: &str,
        expected_head_sha: &str,
    ) -> Result<bool, GitError> {
        match self.validate_created_branch_deletion(repository, branch_name, expected_head_sha) {
            Ok(()) => {
                self.delete_created_branch(repository, branch_name, expected_head_sha)?;
                Ok(true)
            }
            Err(GitError::BranchNotFound(_)) => Ok(false),
            Err(source) => Err(source),
        }
    }

    pub fn worktree_is_registered(
        &self,
        repository: &GitRepository,
        path: &Path,
    ) -> Result<bool, GitError> {
        Ok(self.worktree_record(repository, path)?.is_some())
    }

    fn worktree_lock_reason(
        &self,
        repository: &GitRepository,
        path: &Path,
    ) -> Result<Option<String>, GitError> {
        Ok(self.worktree_record(repository, path)?.and_then(|record| {
            record.lines().find_map(|field| {
                field.strip_prefix("locked").map(|reason| {
                    let reason = reason.trim();
                    if reason.is_empty() {
                        "no reason supplied".to_owned()
                    } else {
                        reason.to_owned()
                    }
                })
            })
        }))
    }

    fn worktree_record(
        &self,
        repository: &GitRepository,
        path: &Path,
    ) -> Result<Option<String>, GitError> {
        let listing = self.run_text(
            &repository.root_path,
            "worktree-list",
            ["worktree", "list", "--porcelain", "-z"],
        )?;
        let marker = format!("worktree {}\0", path.to_string_lossy());
        Ok(listing
            .split("\0\0")
            .find(|record| record.starts_with(&marker))
            .map(|record| record.replace('\0', "\n")))
    }
}
