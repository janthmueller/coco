use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;

#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::domain::{BaseRelation, GitObservation};

const DEFAULT_CAPTURE_LIMIT: usize = 16 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum GitError {
    #[error("filesystem operation failed for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("git command `{category}` failed with status {status:?}: {stderr}")]
    CommandFailed {
        category: &'static str,
        status: Option<i32>,
        stderr: String,
    },
    #[error("git command `{category}` produced more than {limit} bytes")]
    OutputTooLarge {
        category: &'static str,
        limit: usize,
    },
    #[error("git command `{category}` returned non-UTF-8 metadata")]
    NonUtf8Metadata { category: &'static str },
    #[error("not a usable Git worktree: {0}")]
    NotAWorktree(PathBuf),
    #[error("repository checkout is dirty: {0}")]
    DirtyRepository(PathBuf),
    #[error("invalid task name `{0}`; use 1-63 lowercase ASCII letters, digits, or hyphens")]
    InvalidTaskName(String),
    #[error("branch already exists: {0}")]
    BranchExists(String),
    #[error("worktree destination already exists (including symlinks): {0}")]
    DestinationExists(PathBuf),
    #[error("base commit is not a complete Git object ID: {0}")]
    InvalidBaseSha(String),
    #[error("worktree binding mismatch: {0}")]
    BindingMismatch(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRepository {
    pub id: String,
    pub root_path: PathBuf,
    pub git_common_dir: PathBuf,
    pub display_name: String,
    pub is_linked_worktree: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreePlan {
    pub path: PathBuf,
    pub branch_name: String,
    pub base_sha: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeBinding {
    pub path: PathBuf,
    pub branch_name: String,
    pub head_sha: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffResult {
    pub tracked_patch: Vec<u8>,
    pub tracked_patch_truncated: bool,
    pub untracked_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct Git {
    executable: PathBuf,
    capture_limit: usize,
}

impl Default for Git {
    fn default() -> Self {
        Self::new("git")
    }
}

impl Git {
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            capture_limit: DEFAULT_CAPTURE_LIMIT,
        }
    }

    pub fn with_capture_limit(mut self, capture_limit: usize) -> Self {
        self.capture_limit = capture_limit.max(1);
        self
    }

    pub fn discover(&self, requested_path: impl AsRef<Path>) -> Result<GitRepository, GitError> {
        let requested_path = canonicalize(requested_path.as_ref())?;
        let root = self.canonical_git_path(
            &requested_path,
            "discover-root",
            ["rev-parse", "--show-toplevel"],
        )?;
        let common_dir = self.canonical_git_path(
            &root,
            "discover-common-dir",
            ["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )?;
        let git_dir = self.canonical_git_path(
            &root,
            "discover-git-dir",
            ["rev-parse", "--path-format=absolute", "--git-dir"],
        )?;

        let digest = hex::encode(Sha256::digest(common_dir.as_os_str().as_encoded_bytes()));
        let id = format!("repo-{}", &digest[..24]);
        let display_name = root
            .file_name()
            .unwrap_or_else(|| OsStr::new("repository"))
            .to_string_lossy()
            .into_owned();

        Ok(GitRepository {
            id,
            root_path: root,
            git_common_dir: common_dir.clone(),
            display_name,
            is_linked_worktree: git_dir != common_dir,
        })
    }

    pub fn assert_clean(&self, repository: &GitRepository) -> Result<(), GitError> {
        let output = self.run(
            &repository.root_path,
            "status",
            ["status", "--porcelain=v1", "-z", "--untracked-files=all"],
        )?;
        if output.stdout.bytes.is_empty() {
            Ok(())
        } else {
            Err(GitError::DirtyRepository(repository.root_path.clone()))
        }
    }

    pub fn resolve_commit(
        &self,
        repository: &GitRepository,
        reference: &str,
    ) -> Result<String, GitError> {
        let peeled = format!("{reference}^{{commit}}");
        let output = self.run(
            &repository.root_path,
            "resolve-commit",
            [
                OsString::from("rev-parse"),
                OsString::from("--verify"),
                OsString::from("--end-of-options"),
                OsString::from(peeled),
            ],
        )?;
        let sha = one_line_metadata(output, "resolve-commit")?;
        validate_object_id(&sha)?;
        Ok(sha)
    }

    pub fn plan_worktree(
        &self,
        repository: &GitRepository,
        worktrees_root: impl AsRef<Path>,
        task_name: &str,
        base_sha: &str,
    ) -> Result<WorktreePlan, GitError> {
        validate_task_name(task_name)?;
        validate_object_id(base_sha)?;
        let branch_name = format!("coco/{task_name}");
        self.validate_branch_name(repository, &branch_name)?;

        let worktrees_root = secure_directory(worktrees_root.as_ref())?;
        let repository_root = secure_directory(&worktrees_root.join(&repository.id))?;
        let path = repository_root.join(task_name);
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

    fn canonical_git_path<I, S>(
        &self,
        cwd: &Path,
        category: &'static str,
        args: I,
    ) -> Result<PathBuf, GitError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let value = self.run_text(cwd, category, args)?;
        if value.is_empty() {
            return Err(GitError::NotAWorktree(cwd.to_owned()));
        }
        canonicalize(Path::new(&value))
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

    fn run<I, S>(
        &self,
        cwd: &Path,
        category: &'static str,
        args: I,
    ) -> Result<CommandOutput, GitError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let output = self.execute(cwd, category, args)?;
        ensure_success(category, &output)?;
        if output.stdout.truncated || output.stderr.truncated {
            return Err(GitError::OutputTooLarge {
                category,
                limit: self.capture_limit,
            });
        }
        Ok(output)
    }

    fn run_text<I, S>(
        &self,
        cwd: &Path,
        category: &'static str,
        args: I,
    ) -> Result<String, GitError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        one_line_metadata(self.run(cwd, category, args)?, category)
    }

    fn execute<I, S>(
        &self,
        cwd: &Path,
        _category: &'static str,
        args: I,
    ) -> Result<CommandOutput, GitError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = Command::new(&self.executable);
        command
            .current_dir(cwd)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("LC_ALL", "C")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_OBJECT_DIRECTORY")
            .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
            .env_remove("GIT_CONFIG_COUNT");
        command
            .env_remove("GIT_CONFIG_GLOBAL")
            .env_remove("GIT_CONFIG_SYSTEM")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_NAMESPACE");
        let mut child = command.spawn().map_err(|source| GitError::Io {
            path: self.executable.clone(),
            source,
        })?;
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let limit = self.capture_limit;
        let stdout_reader = thread::spawn(move || read_bounded(stdout, limit));
        let stderr_reader = thread::spawn(move || read_bounded(stderr, limit));
        let status = child.wait().map_err(|source| GitError::Io {
            path: cwd.to_owned(),
            source,
        })?;
        let stdout = stdout_reader
            .join()
            .expect("Git stdout reader panicked")
            .map_err(|source| GitError::Io {
                path: cwd.to_owned(),
                source,
            })?;
        let stderr = stderr_reader
            .join()
            .expect("Git stderr reader panicked")
            .map_err(|source| GitError::Io {
                path: cwd.to_owned(),
                source,
            })?;
        Ok(CommandOutput {
            status,
            stdout,
            stderr,
        })
    }
}

pub fn validate_task_name(name: &str) -> Result<(), GitError> {
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
        Err(GitError::InvalidTaskName(name.to_owned()))
    }
}

fn validate_object_id(value: &str) -> Result<(), GitError> {
    if matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(GitError::InvalidBaseSha(value.to_owned()))
    }
}

#[derive(Debug)]
struct CommandOutput {
    status: ExitStatus,
    stdout: BoundedBytes,
    stderr: BoundedBytes,
}

#[derive(Debug)]
struct BoundedBytes {
    bytes: Vec<u8>,
    truncated: bool,
}

fn read_bounded(mut reader: impl Read, limit: usize) -> io::Result<BoundedBytes> {
    let mut retained = Vec::with_capacity(limit.min(8 * 1024));
    let mut buffer = [0_u8; 8 * 1024];
    let mut truncated = false;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_sub(retained.len());
        retained.extend_from_slice(&buffer[..count.min(remaining)]);
        truncated |= count > remaining;
    }
    Ok(BoundedBytes {
        bytes: retained,
        truncated,
    })
}

fn ensure_success(category: &'static str, output: &CommandOutput) -> Result<(), GitError> {
    if output.status.success() {
        Ok(())
    } else {
        Err(command_failed(category, output))
    }
}

fn command_failed(category: &'static str, output: &CommandOutput) -> GitError {
    let mut stderr = String::from_utf8_lossy(&output.stderr.bytes)
        .trim()
        .to_owned();
    if output.stderr.truncated {
        stderr.push_str(" [truncated]");
    }
    GitError::CommandFailed {
        category,
        status: output.status.code(),
        stderr,
    }
}

fn one_line_metadata(output: CommandOutput, category: &'static str) -> Result<String, GitError> {
    if output.stdout.truncated {
        return Err(GitError::OutputTooLarge {
            category,
            limit: output.stdout.bytes.len(),
        });
    }
    String::from_utf8(output.stdout.bytes)
        .map(|value| value.trim().to_owned())
        .map_err(|_| GitError::NonUtf8Metadata { category })
}

fn canonicalize(path: &Path) -> Result<PathBuf, GitError> {
    fs::canonicalize(path).map_err(|source| GitError::Io {
        path: path.to_owned(),
        source,
    })
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;

    use tempfile::TempDir;

    use super::*;

    struct Fixture {
        _temp: TempDir,
        source: PathBuf,
        worktrees: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let temp = tempfile::tempdir().unwrap();
            let source = temp.path().join("source");
            run(
                temp.path(),
                ["init", "--initial-branch=main", source.to_str().unwrap()],
            );
            run(&source, ["config", "user.name", "CoCo Tests"]);
            run(&source, ["config", "user.email", "coco@example.invalid"]);
            fs::write(source.join("README.md"), "fixture\n").unwrap();
            run(&source, ["add", "README.md"]);
            run(&source, ["commit", "-m", "fixture"]);
            let worktrees = temp.path().join("worktrees");
            Self {
                _temp: temp,
                source,
                worktrees,
            }
        }
    }

    #[test]
    fn repository_identity_is_stable_across_worktrees() {
        let fixture = Fixture::new();
        let git = Git::default();
        let repository = git.discover(&fixture.source).unwrap();
        let base = git.resolve_commit(&repository, "HEAD").unwrap();
        let plan = git
            .plan_worktree(&repository, &fixture.worktrees, "stable", &base)
            .unwrap();
        let binding = git.create_worktree(&repository, &plan).unwrap();
        let linked = git.discover(&binding.path).unwrap();

        assert_eq!(linked.id, repository.id);
        assert_eq!(linked.git_common_dir, repository.git_common_dir);
        assert_ne!(linked.root_path, repository.root_path);
        assert!(linked.is_linked_worktree);
        assert_eq!(binding.branch_name, "coco/stable");
        assert_eq!(binding.head_sha, base);
    }

    #[test]
    fn dirty_source_and_collisions_are_rejected() {
        let fixture = Fixture::new();
        let git = Git::default();
        let repository = git.discover(&fixture.source).unwrap();
        let base = git.resolve_commit(&repository, "HEAD").unwrap();
        let plan = git
            .plan_worktree(&repository, &fixture.worktrees, "collision", &base)
            .unwrap();
        git.create_worktree(&repository, &plan).unwrap();

        assert!(matches!(
            git.plan_worktree(&repository, &fixture.worktrees, "collision", &base),
            Err(GitError::DestinationExists(_))
        ));
        fs::write(fixture.source.join("untracked.txt"), "dirty\n").unwrap();
        assert!(matches!(
            git.assert_clean(&repository),
            Err(GitError::DirtyRepository(_))
        ));
    }

    #[test]
    fn diff_and_observation_include_commits_and_untracked_paths() {
        let fixture = Fixture::new();
        let git = Git::default();
        let repository = git.discover(&fixture.source).unwrap();
        let base = git.resolve_commit(&repository, "HEAD").unwrap();
        let plan = git
            .plan_worktree(&repository, &fixture.worktrees, "inspect", &base)
            .unwrap();
        let binding = git.create_worktree(&repository, &plan).unwrap();

        fs::write(binding.path.join("README.md"), "changed\n").unwrap();
        run(&binding.path, ["add", "README.md"]);
        run(&binding.path, ["commit", "-m", "change"]);
        fs::write(binding.path.join("untracked.txt"), "new\n").unwrap();

        let diff = git.diff(&binding.path, &base).unwrap();
        assert!(String::from_utf8_lossy(&diff.tracked_patch).contains("+changed"));
        assert_eq!(diff.untracked_paths, [PathBuf::from("untracked.txt")]);

        let observation = git
            .observe(&repository, &binding.path, "coco/inspect", &base)
            .unwrap();
        assert_eq!(observation.base_relation, BaseRelation::Descendant);
        assert_eq!(observation.ahead_by, Some(1));
        assert_eq!(observation.behind_by, Some(0));
        assert!(observation.dirty);
        assert!(observation.binding_valid);
    }

    #[test]
    fn validates_task_names_without_accepting_path_syntax() {
        assert!(validate_task_name("task-42").is_ok());
        for invalid in ["", "UPPER", "-leading", "trailing-", "path/name", "a_b"] {
            assert!(matches!(
                validate_task_name(invalid),
                Err(GitError::InvalidTaskName(_))
            ));
        }
    }

    fn run<const N: usize>(cwd: &Path, args: [&str; N]) {
        let result = Command::new("git")
            .current_dir(cwd)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "git failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}
