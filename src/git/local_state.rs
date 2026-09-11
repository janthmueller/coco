use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

use super::command::{command_failed, ensure_success};
use super::repository::{canonicalize, validate_object_id};
use super::{
    Git, GitError, GitRepository, IncludedFile, LocalStateManifest, LocalStatePolicy,
    LocalStateSnapshot,
};

const WORKTREE_INCLUDE_FILE: &str = ".worktreeinclude";
const AUTOMATIC_INCLUDE: &str = "AGENTS.override.md";
const MAX_INCLUDE_RULE_BYTES: u64 = 64 * 1024;
const MAX_CARRIED_FILES: usize = 256;
const MAX_CARRIED_FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_CARRIED_TOTAL_BYTES: usize = 32 * 1024 * 1024;

impl Git {
    pub fn snapshot_local_state(
        &self,
        repository: &GitRepository,
        base_sha: &str,
        policy: LocalStatePolicy,
    ) -> Result<LocalStateSnapshot, GitError> {
        validate_object_id(base_sha)?;
        let source_head = self.resolve_commit(repository, "HEAD")?;
        let carry_tracked = policy.carries_tracked();
        let carry_untracked = policy.carries_untracked();
        let (staged_patch, unstaged_patch) = if carry_tracked {
            if source_head != base_sha {
                return Err(GitError::LocalChangesBaseMismatch {
                    expected: base_sha.to_owned(),
                    actual: source_head,
                });
            }
            (
                self.tracked_patch(&repository.root_path, true)?,
                self.tracked_patch(&repository.root_path, false)?,
            )
        } else if policy == LocalStatePolicy::RequireClean {
            self.assert_clean(repository)?;
            (Vec::new(), Vec::new())
        } else {
            (Vec::new(), Vec::new())
        };
        let untracked_paths = if carry_untracked {
            self.snapshot_untracked_paths(&repository.root_path)?
        } else {
            if carry_tracked {
                self.reject_untracked_files(&repository.root_path)?;
            }
            Vec::new()
        };
        let included_paths = self.snapshot_included_paths(&repository.root_path)?;
        let carried_file_count = untracked_paths.len().saturating_add(included_paths.len());
        if carried_file_count > MAX_CARRIED_FILES {
            return Err(GitError::CarriedFileLimit {
                limit: MAX_CARRIED_FILES,
            });
        }
        let mut carried_bytes = 0;
        let untracked_files =
            self.snapshot_files(&repository.root_path, untracked_paths, &mut carried_bytes)?;
        let included_files =
            self.snapshot_files(&repository.root_path, included_paths, &mut carried_bytes)?;
        let untracked_file_bytes = file_bytes(&untracked_files);
        let included_file_bytes = file_bytes(&included_files);
        let manifest = LocalStateManifest {
            source_path: repository.root_path.clone(),
            source_head,
            carried_tracked_changes: carry_tracked,
            staged_patch_bytes: staged_patch.len(),
            unstaged_patch_bytes: unstaged_patch.len(),
            untracked_file_count: untracked_files.len(),
            untracked_file_bytes,
            included_file_count: included_files.len(),
            included_file_bytes,
            snapshot_hash: snapshot_hash(
                &staged_patch,
                &unstaged_patch,
                &untracked_files,
                &included_files,
            ),
        };
        Ok(LocalStateSnapshot {
            staged_patch,
            unstaged_patch,
            untracked_files,
            included_files,
            manifest,
        })
    }

    pub fn apply_local_state(
        &self,
        worktree_path: &Path,
        snapshot: &LocalStateSnapshot,
    ) -> Result<(), GitError> {
        let worktree_path = canonicalize(worktree_path)?;
        if !snapshot.staged_patch.is_empty() {
            self.run_with_input(
                &worktree_path,
                "apply-staged-changes",
                ["apply", "--index", "--binary", "--whitespace=nowarn", "-"],
                &snapshot.staged_patch,
            )?;
        }
        if !snapshot.unstaged_patch.is_empty() {
            self.run_with_input(
                &worktree_path,
                "apply-unstaged-changes",
                ["apply", "--binary", "--whitespace=nowarn", "-"],
                &snapshot.unstaged_patch,
            )?;
        }
        for file in snapshot
            .untracked_files
            .iter()
            .chain(&snapshot.included_files)
        {
            copy_carried_file(&worktree_path, file)?;
        }
        Ok(())
    }

    fn tracked_patch(&self, source: &Path, staged: bool) -> Result<Vec<u8>, GitError> {
        let mut arguments = vec![
            OsString::from("diff"),
            OsString::from("--no-ext-diff"),
            OsString::from("--binary"),
            OsString::from("--full-index"),
        ];
        if staged {
            arguments.push(OsString::from("--cached"));
            arguments.push(OsString::from("HEAD"));
        }
        arguments.push(OsString::from("--"));
        Ok(self
            .run(source, "snapshot-tracked-changes", arguments)?
            .stdout
            .bytes)
    }

    fn reject_untracked_files(&self, source: &Path) -> Result<(), GitError> {
        if let Some(path) = self.snapshot_untracked_paths(source)?.into_iter().next() {
            return Err(GitError::UntrackedChanges(path));
        }
        Ok(())
    }

    fn snapshot_untracked_paths(&self, source: &Path) -> Result<Vec<PathBuf>, GitError> {
        let output = self.run(
            source,
            "snapshot-untracked-files",
            ["ls-files", "--others", "--exclude-standard", "-z"],
        )?;
        nul_paths(&output.stdout.bytes, "snapshot-untracked-files")
    }

    fn snapshot_included_paths(&self, source: &Path) -> Result<Vec<PathBuf>, GitError> {
        let rules = source.join(WORKTREE_INCLUDE_FILE);
        let mut candidates = Vec::new();
        match fs::symlink_metadata(&rules) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink()
                    || !metadata.is_file()
                    || metadata.len() > MAX_INCLUDE_RULE_BYTES
                {
                    return Err(GitError::UnsafeCarriedPath(rules));
                }
                let output = self.run(
                    source,
                    "worktree-include-candidates",
                    [
                        OsString::from("ls-files"),
                        OsString::from("--others"),
                        OsString::from("--ignored"),
                        OsString::from("-z"),
                        OsString::from("-X"),
                        rules.as_os_str().to_owned(),
                    ],
                )?;
                candidates = nul_paths(&output.stdout.bytes, "worktree-include-candidates")?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source_error) => {
                return Err(GitError::Io {
                    path: rules,
                    source: source_error,
                });
            }
        }

        let automatic = PathBuf::from(AUTOMATIC_INCLUDE);
        if fs::symlink_metadata(source.join(&automatic)).is_ok() {
            candidates.push(automatic);
        }
        candidates.sort();
        candidates.dedup();
        self.retain_ignored_paths(source, &candidates)
    }

    fn snapshot_files(
        &self,
        source: &Path,
        paths: Vec<PathBuf>,
        total_bytes: &mut usize,
    ) -> Result<Vec<IncludedFile>, GitError> {
        let mut files = Vec::with_capacity(paths.len());
        for path in paths {
            validate_relative_path(&path)?;
            let source_path = source.join(&path);
            let metadata = fs::symlink_metadata(&source_path).map_err(|source| GitError::Io {
                path: source_path.clone(),
                source,
            })?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                continue;
            }
            if metadata.len() > MAX_CARRIED_FILE_BYTES {
                return Err(GitError::CarriedByteLimit {
                    limit: MAX_CARRIED_FILE_BYTES as usize,
                });
            }
            let contents = fs::read(&source_path).map_err(|source| GitError::Io {
                path: source_path,
                source,
            })?;
            if contents.len() > MAX_CARRIED_FILE_BYTES as usize {
                return Err(GitError::CarriedByteLimit {
                    limit: MAX_CARRIED_FILE_BYTES as usize,
                });
            }
            *total_bytes = total_bytes.saturating_add(contents.len());
            if *total_bytes > MAX_CARRIED_TOTAL_BYTES {
                return Err(GitError::CarriedByteLimit {
                    limit: MAX_CARRIED_TOTAL_BYTES,
                });
            }
            files.push(IncludedFile { path, contents });
        }
        Ok(files)
    }

    fn retain_ignored_paths(
        &self,
        source: &Path,
        candidates: &[PathBuf],
    ) -> Result<Vec<PathBuf>, GitError> {
        if candidates.is_empty() {
            return Ok(Vec::new());
        }
        let mut input = Vec::new();
        for path in candidates {
            let path = path
                .to_str()
                .ok_or(GitError::NonUtf8Metadata {
                    category: "worktree-include-path",
                })?
                .as_bytes();
            input.extend_from_slice(path);
            input.push(0);
        }
        let output = self.execute_with_input(
            source,
            "worktree-include-ignore-check",
            ["check-ignore", "--stdin", "-z"],
            &input,
        )?;
        if output.stdout.truncated || output.stderr.truncated {
            return Err(GitError::OutputTooLarge {
                category: "worktree-include-ignore-check",
                limit: self.capture_limit,
            });
        }
        match output.status.code() {
            Some(0) => {
                ensure_success("worktree-include-ignore-check", &output)?;
                nul_paths(&output.stdout.bytes, "worktree-include-ignore-check")
            }
            Some(1) => Ok(Vec::new()),
            _ => Err(command_failed("worktree-include-ignore-check", &output)),
        }
    }
}

fn copy_carried_file(worktree: &Path, file: &IncludedFile) -> Result<(), GitError> {
    validate_relative_path(&file.path)?;
    let destination = worktree.join(&file.path);
    let parent = destination
        .parent()
        .ok_or_else(|| GitError::UnsafeCarriedPath(file.path.clone()))?;
    secure_relative_directories(worktree, parent)?;
    if fs::symlink_metadata(&destination).is_ok() {
        return Err(GitError::CarriedDestinationExists(file.path.clone()));
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut destination_file = options.open(&destination).map_err(|source| GitError::Io {
        path: destination.clone(),
        source,
    })?;
    destination_file
        .write_all(&file.contents)
        .map_err(|source| GitError::Io {
            path: destination,
            source,
        })
}

fn secure_relative_directories(worktree: &Path, parent: &Path) -> Result<(), GitError> {
    let relative = parent
        .strip_prefix(worktree)
        .map_err(|_| GitError::UnsafeCarriedPath(parent.to_owned()))?;
    let mut current = worktree.to_owned();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err(GitError::UnsafeCarriedPath(relative.to_owned()));
        };
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => return Err(GitError::UnsafeCarriedPath(relative.to_owned())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current).map_err(|source| GitError::Io {
                    path: current.clone(),
                    source,
                })?;
                #[cfg(unix)]
                fs::set_permissions(&current, fs::Permissions::from_mode(0o700)).map_err(
                    |source| GitError::Io {
                        path: current.clone(),
                        source,
                    },
                )?;
            }
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

fn validate_relative_path(path: &Path) -> Result<(), GitError> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(GitError::UnsafeCarriedPath(path.to_owned()));
    }
    Ok(())
}

fn nul_paths(bytes: &[u8], category: &'static str) -> Result<Vec<PathBuf>, GitError> {
    bytes
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| {
            std::str::from_utf8(path)
                .map(PathBuf::from)
                .map_err(|_| GitError::NonUtf8Metadata { category })
        })
        .collect()
}

fn snapshot_hash(
    staged: &[u8],
    unstaged: &[u8],
    untracked: &[IncludedFile],
    included: &[IncludedFile],
) -> String {
    let mut digest = Sha256::new();
    update_hash_part(&mut digest, b"staged");
    update_hash_part(&mut digest, staged);
    update_hash_part(&mut digest, b"unstaged");
    update_hash_part(&mut digest, unstaged);
    update_file_hashes(&mut digest, b"untracked", untracked);
    update_file_hashes(&mut digest, b"included", included);
    hex::encode(digest.finalize())
}

fn update_file_hashes(digest: &mut Sha256, category: &[u8], files: &[IncludedFile]) {
    update_hash_part(digest, category);
    for file in files {
        update_hash_part(digest, file.path.as_os_str().as_encoded_bytes());
        update_hash_part(digest, &file.contents);
    }
}

fn file_bytes(files: &[IncludedFile]) -> usize {
    files.iter().map(|file| file.contents.len()).sum()
}

fn update_hash_part(digest: &mut Sha256, value: &[u8]) {
    digest.update(value.len().to_le_bytes());
    digest.update(value);
}
