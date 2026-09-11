use std::ffi::OsString;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::command::{command_failed, one_line_metadata};
use super::repository::{canonicalize, validate_object_id};
use super::{Git, GitError, GitRepository, WorktreeBinding, WorktreePlan, WorktreeTarget};
use crate::domain::WorktreeMode;

impl Git {
    pub fn plan_worktree(
        &self,
        repository: &GitRepository,
        worktrees_root: impl AsRef<Path>,
        workspace_name: &str,
        target: WorktreeTarget,
        base_sha: &str,
    ) -> Result<WorktreePlan, GitError> {
        validate_workspace_name(workspace_name)?;
        validate_object_id(base_sha)?;
        match &target {
            WorktreeTarget::NewBranch { branch_name } => {
                self.validate_branch_name(repository, branch_name)?;
                if let Some(existing) = self.branch_namespace_collision(repository, branch_name)? {
                    return Err(GitError::BranchCollision {
                        requested: branch_name.clone(),
                        existing,
                    });
                }
            }
            WorktreeTarget::ExistingBranch { branch_name } => {
                let branch_sha = self.resolve_local_branch(repository, branch_name)?;
                if branch_sha != base_sha {
                    return Err(GitError::BindingMismatch(format!(
                        "existing branch {branch_name} points at {branch_sha}, not requested base {base_sha}"
                    )));
                }
                if let Some(path) = self.branch_checkout_path(repository, branch_name)? {
                    return Err(GitError::BranchAlreadyCheckedOut {
                        branch: branch_name.clone(),
                        path,
                    });
                }
            }
            WorktreeTarget::Detached => {}
        }

        let worktrees_root = secure_directory(worktrees_root.as_ref())?;
        let repository_root = secure_directory(&worktrees_root.join(&repository.id))?;
        let path = workspace_path(&repository_root, workspace_name)?;
        if fs::symlink_metadata(&path).is_ok() {
            return Err(GitError::DestinationExists(path));
        }

        Ok(WorktreePlan {
            path,
            mode: target.mode(),
            branch_name: target.branch_name().map(ToOwned::to_owned),
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
        match (plan.mode, plan.branch_name.as_deref()) {
            (WorktreeMode::NewBranch, Some(branch_name)) => {
                if let Some(existing) = self.branch_namespace_collision(repository, branch_name)? {
                    return Err(GitError::BranchCollision {
                        requested: branch_name.to_owned(),
                        existing,
                    });
                }
                self.validate_branch_name(repository, branch_name)?;
            }
            (WorktreeMode::ExistingBranch, Some(branch_name)) => {
                let branch_sha = self.resolve_local_branch(repository, branch_name)?;
                if branch_sha != plan.base_sha {
                    return Err(GitError::BindingMismatch(format!(
                        "existing branch {branch_name} moved from {} to {branch_sha}",
                        plan.base_sha
                    )));
                }
                if let Some(path) = self.branch_checkout_path(repository, branch_name)? {
                    return Err(GitError::BranchAlreadyCheckedOut {
                        branch: branch_name.to_owned(),
                        path,
                    });
                }
            }
            (WorktreeMode::Detached, None) => {}
            _ => {
                return Err(GitError::BindingMismatch(
                    "worktree mode and branch binding disagree".to_owned(),
                ));
            }
        }
        validate_object_id(&plan.base_sha)?;

        let mut arguments = vec![
            OsString::from("worktree"),
            OsString::from("add"),
            OsString::from("--no-guess-remote"),
        ];
        match (plan.mode, plan.branch_name.as_deref()) {
            (WorktreeMode::NewBranch, Some(branch_name)) => {
                arguments.push(OsString::from("-b"));
                arguments.push(OsString::from(branch_name));
            }
            (WorktreeMode::ExistingBranch, Some(_)) => {}
            (WorktreeMode::Detached, None) => arguments.push(OsString::from("--detach")),
            _ => unreachable!("worktree plan invariants were checked above"),
        }
        arguments.push(OsString::from("--"));
        arguments.push(plan.path.as_os_str().to_owned());
        match (plan.mode, plan.branch_name.as_deref()) {
            (WorktreeMode::ExistingBranch, Some(branch_name)) => {
                arguments.push(OsString::from(branch_name));
            }
            _ => arguments.push(OsString::from(&plan.base_sha)),
        }
        self.run(&repository.root_path, "worktree-add", arguments)?;

        let canonical = canonicalize(&plan.path)?;
        if canonical != plan.path {
            return Err(GitError::BindingMismatch(format!(
                "created path {} canonicalized to {}",
                plan.path.display(),
                canonical.display()
            )));
        }
        let binding = self.verify_worktree(
            repository,
            &canonical,
            plan.mode,
            plan.branch_name.as_deref(),
        )?;
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
        expected_mode: WorktreeMode,
        expected_branch: Option<&str>,
    ) -> Result<WorktreeBinding, GitError> {
        let supplied_path = worktree_path.as_ref();
        let worktree_path = canonicalize(supplied_path)?;
        if worktree_path != supplied_path {
            return Err(GitError::BindingMismatch(format!(
                "worktree path {} redirects to {}",
                supplied_path.display(),
                worktree_path.display()
            )));
        }
        let discovered = self.discover(&worktree_path)?;
        if discovered.git_common_dir != repository.git_common_dir {
            return Err(GitError::BindingMismatch(
                "worktree belongs to another Git common directory".to_owned(),
            ));
        }
        let branch = self.execute(
            &worktree_path,
            "symbolic-ref",
            ["symbolic-ref", "--quiet", "HEAD"],
        )?;
        let observed_branch = match branch.status.code() {
            Some(0) => {
                let branch_ref = one_line_metadata(branch, "symbolic-ref")?;
                Some(
                    branch_ref
                        .strip_prefix("refs/heads/")
                        .ok_or_else(|| {
                            GitError::BindingMismatch(format!(
                                "HEAD points at unexpected ref {branch_ref}"
                            ))
                        })?
                        .to_owned(),
                )
            }
            Some(1) => None,
            _ => return Err(command_failed("symbolic-ref", &branch)),
        };
        match (expected_mode, expected_branch, observed_branch.as_deref()) {
            (
                WorktreeMode::NewBranch | WorktreeMode::ExistingBranch,
                Some(expected),
                Some(actual),
            ) if expected == actual => {}
            (WorktreeMode::Detached, None, None) => {}
            _ => {
                return Err(GitError::BindingMismatch(format!(
                    "expected {expected_mode:?} branch {expected_branch:?}, observed {observed_branch:?}"
                )));
            }
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
            mode: expected_mode,
            branch_name: observed_branch,
            head_sha,
        })
    }

    pub fn resolve_local_branch(
        &self,
        repository: &GitRepository,
        branch_name: &str,
    ) -> Result<String, GitError> {
        self.validate_branch_name(repository, branch_name)?;
        let reference = format!("refs/heads/{branch_name}^{{commit}}");
        let output = self.execute(
            &repository.root_path,
            "existing-branch",
            [
                "rev-parse",
                "--verify",
                "--quiet",
                "--end-of-options",
                &reference,
            ],
        )?;
        match output.status.code() {
            Some(0) => one_line_metadata(output, "existing-branch"),
            Some(1) => Err(GitError::BranchNotFound(branch_name.to_owned())),
            _ => Err(command_failed("existing-branch", &output)),
        }
    }

    pub(super) fn branch_checkout_path(
        &self,
        repository: &GitRepository,
        branch_name: &str,
    ) -> Result<Option<PathBuf>, GitError> {
        let listing = self.run_text(
            &repository.root_path,
            "worktree-list",
            ["worktree", "list", "--porcelain", "-z"],
        )?;
        let expected = format!("branch refs/heads/{branch_name}");
        for record in listing.split("\0\0") {
            let mut path = None;
            let mut matches = false;
            for field in record.split('\0') {
                if let Some(value) = field.strip_prefix("worktree ") {
                    path = Some(PathBuf::from(value));
                } else if field == expected {
                    matches = true;
                }
            }
            if matches {
                return Ok(path);
            }
        }
        Ok(None)
    }

    pub(super) fn validate_branch_name(
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

    fn branch_namespace_collision(
        &self,
        repository: &GitRepository,
        branch_name: &str,
    ) -> Result<Option<String>, GitError> {
        let requested = format!("refs/heads/{branch_name}");
        let refs = self.run_text(
            &repository.root_path,
            "branch-list",
            [
                OsString::from("for-each-ref"),
                OsString::from("--format=%(refname)"),
                OsString::from("refs/heads"),
            ],
        )?;
        Ok(refs.lines().find_map(|existing| {
            let collides = existing == requested
                || existing
                    .strip_prefix(&requested)
                    .is_some_and(|suffix| suffix.starts_with('/'))
                || requested
                    .strip_prefix(existing)
                    .is_some_and(|suffix| suffix.starts_with('/'));
            collides.then(|| {
                existing
                    .strip_prefix("refs/heads/")
                    .unwrap_or(existing)
                    .to_owned()
            })
        }))
    }
}

pub(super) fn validate_workspace_name(name: &str) -> Result<(), GitError> {
    let bytes = name.as_bytes();
    let valid =
        (1..=63).contains(&bytes.len()) && name.split('/').all(valid_workspace_name_component);
    if valid {
        Ok(())
    } else {
        Err(GitError::InvalidWorkspaceName(name.to_owned()))
    }
}

fn valid_workspace_name_component(component: &str) -> bool {
    let bytes = component.as_bytes();
    !bytes.is_empty()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
        && bytes
            .first()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && bytes
            .last()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
}

pub(super) fn workspace_path(
    repository_root: &Path,
    workspace_name: &str,
) -> Result<PathBuf, GitError> {
    let mut components = workspace_name.split('/').peekable();
    let mut parent = repository_root.to_path_buf();
    while let Some(component) = components.next() {
        if components.peek().is_none() {
            return Ok(parent.join(component));
        }
        parent = secure_child_directory(&parent, component)?;
    }
    Err(GitError::InvalidWorkspaceName(workspace_name.to_owned()))
}

fn secure_child_directory(parent: &Path, component: &str) -> Result<PathBuf, GitError> {
    let path = parent.join(component);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => return Err(GitError::DestinationExists(path)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&path).map_err(|source| GitError::Io {
                path: path.clone(),
                source,
            })?;
        }
        Err(source) => return Err(GitError::Io { path, source }),
    }
    #[cfg(unix)]
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).map_err(|source| {
        GitError::Io {
            path: path.clone(),
            source,
        }
    })?;
    let canonical = canonicalize(&path)?;
    if canonical != path {
        return Err(GitError::DestinationExists(path));
    }
    Ok(canonical)
}

pub(super) fn secure_directory(path: &Path) -> Result<PathBuf, GitError> {
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
