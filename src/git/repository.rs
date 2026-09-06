use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::command::one_line_metadata;
use super::{Git, GitError, GitRepository};

impl Git {
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
}

pub(super) fn validate_object_id(value: &str) -> Result<(), GitError> {
    if matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(GitError::InvalidBaseSha(value.to_owned()))
    }
}

pub(super) fn canonicalize(path: &Path) -> Result<PathBuf, GitError> {
    fs::canonicalize(path).map_err(|source| GitError::Io {
        path: path.to_owned(),
        source,
    })
}
