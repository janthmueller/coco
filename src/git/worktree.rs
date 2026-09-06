use std::ffi::OsString;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::command::command_failed;
use super::repository::{canonicalize, validate_object_id};
use super::{Git, GitError, GitRepository, WorktreeBinding, WorktreePlan};

impl Git {
    pub fn plan_worktree(
        &self,
        repository: &GitRepository,
        worktrees_root: impl AsRef<Path>,
        workspace_name: &str,
        base_sha: &str,
    ) -> Result<WorktreePlan, GitError> {
        validate_workspace_name(workspace_name)?;
        validate_object_id(base_sha)?;
        let branch_name = format!("coco/{workspace_name}");
        self.validate_branch_name(repository, &branch_name)?;

        let worktrees_root = secure_directory(worktrees_root.as_ref())?;
        let repository_root = secure_directory(&worktrees_root.join(&repository.id))?;
        let path = repository_root.join(workspace_name);
        if fs::symlink_metadata(&path).is_ok() {
            return Err(GitError::DestinationExists(path));
        }
        match self.branch_exists(repository, &branch_name) {
            Ok(true) => return Err(GitError::BranchExists(branch_name)),
            Ok(false) => {}
            Err(error) => return Err(error),
        }

        Ok(WorktreePlan {
            path,
            branch_name,
            base_sha: base_sha.to_owned(),
        })
    }

    pub fn create_worktree(
        &self,
        repository: &GitRepository,
        plan: &WorktreePlan,
    ) -> Result<WorktreeBinding, GitError> {
        // Repeat collision checks immediately before the side effect. The caller must
        // additionally hold its repository-scoped lock across plan and creation.
        if fs::symlink_metadata(&plan.path).is_ok() {
            return Err(GitError::DestinationExists(plan.path.clone()));
        }
        if self.branch_exists(repository, &plan.branch_name)? {
            return Err(GitError::BranchExists(plan.branch_name.clone()));
        }
        self.validate_branch_name(repository, &plan.branch_name)?;
        validate_object_id(&plan.base_sha)?;

        self.run(
            &repository.root_path,
            "worktree-add",
            [
                OsString::from("worktree"),
                OsString::from("add"),
                OsString::from("--no-guess-remote"),
                OsString::from("-b"),
                OsString::from(&plan.branch_name),
                OsString::from("--"),
                plan.path.as_os_str().to_owned(),
                OsString::from(&plan.base_sha),
            ],
        )?;

        let canonical = canonicalize(&plan.path)?;
        if canonical != plan.path {
            return Err(GitError::BindingMismatch(format!(
                "created path {} canonicalized to {}",
                plan.path.display(),
                canonical.display()
            )));
        }
        let binding = self.verify_worktree(repository, &canonical, &plan.branch_name)?;
        if binding.head_sha != plan.base_sha {
            return Err(GitError::BindingMismatch(format!(
                "HEAD {} does not equal requested base {}",
                binding.head_sha, plan.base_sha
            )));
        }
        Ok(binding)
    }

    pub fn verify_worktree(
        &self,
        repository: &GitRepository,
        worktree_path: impl AsRef<Path>,
        expected_branch: &str,
    ) -> Result<WorktreeBinding, GitError> {
        let worktree_path = canonicalize(worktree_path.as_ref())?;
        let discovered = self.discover(&worktree_path)?;
        if discovered.git_common_dir != repository.git_common_dir {
            return Err(GitError::BindingMismatch(
                "worktree belongs to another Git common directory".to_owned(),
            ));
        }
        let branch_ref = self.run_text(
            &worktree_path,
            "symbolic-ref",
            ["symbolic-ref", "--quiet", "HEAD"],
        )?;
        let expected_ref = format!("refs/heads/{expected_branch}");
        if branch_ref != expected_ref {
            return Err(GitError::BindingMismatch(format!(
                "expected branch {expected_ref}, observed {branch_ref}"
            )));
        }
        let head_sha = self.run_text(&worktree_path, "head", ["rev-parse", "--verify", "HEAD"])?;
        let listing = self.run_text(
            &repository.root_path,
            "worktree-list",
            ["worktree", "list", "--porcelain", "-z"],
        )?;
        let marker = format!("worktree {}\0", worktree_path.to_string_lossy());
        if !listing
            .split("\0\0")
            .any(|entry| entry.starts_with(&marker))
        {
            return Err(GitError::BindingMismatch(
                "worktree is absent from `git worktree list`".to_owned(),
            ));
        }

        Ok(WorktreeBinding {
            path: worktree_path,
            branch_name: expected_branch.to_owned(),
            head_sha,
        })
    }

    fn validate_branch_name(
        &self,
        repository: &GitRepository,
        branch_name: &str,
    ) -> Result<(), GitError> {
        self.run(
            &repository.root_path,
            "check-ref-format",
            [
                OsString::from("check-ref-format"),
                OsString::from(format!("refs/heads/{branch_name}")),
            ],
        )?;
        Ok(())
    }

    fn branch_exists(
        &self,
        repository: &GitRepository,
        branch_name: &str,
    ) -> Result<bool, GitError> {
        let reference = format!("refs/heads/{branch_name}");
        let output = self.execute(
            &repository.root_path,
            "branch-exists",
            [
                OsString::from("rev-parse"),
                OsString::from("--verify"),
                OsString::from("--quiet"),
                OsString::from("--end-of-options"),
                OsString::from(reference),
            ],
        )?;
        match output.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(command_failed("branch-exists", &output)),
        }
    }
}

pub(super) fn validate_workspace_name(name: &str) -> Result<(), GitError> {
    let bytes = name.as_bytes();
    let valid = (1..=63).contains(&bytes.len())
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
        && bytes
            .first()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && bytes
            .last()
            .is_some_and(|byte| byte.is_ascii_alphanumeric());
    if valid {
        Ok(())
    } else {
        Err(GitError::InvalidWorkspaceName(name.to_owned()))
    }
}

fn secure_directory(path: &Path) -> Result<PathBuf, GitError> {
    fs::create_dir_all(path).map_err(|source| GitError::Io {
        path: path.to_owned(),
        source,
    })?;
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|source| {
        GitError::Io {
            path: path.to_owned(),
            source,
        }
    })?;
    canonicalize(path)
}
