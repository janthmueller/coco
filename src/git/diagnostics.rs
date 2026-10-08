use std::path::{Path, PathBuf};

use tokio::process::Command;

use crate::diagnostics::{ProbeError, capture_command};

use super::Git;

pub(crate) struct DiagnosticGitRepository {
    pub(crate) root_path: PathBuf,
    pub(crate) common_dir: PathBuf,
}

pub(crate) struct DiagnosticGitBinding {
    pub(crate) root_path: PathBuf,
    pub(crate) common_dir: PathBuf,
    pub(crate) branch_name: Option<String>,
}

impl Git {
    /// Inspects only Git metadata, never the index, working changes, hooks,
    /// remotes, filters, or repository-local commands.
    pub(crate) async fn diagnostic_repository(
        &self,
        path: &Path,
    ) -> Result<DiagnosticGitRepository, ProbeError> {
        let output = capture_command(self.diagnostic_command(path)).await?;
        let mut lines = output.lines();
        let repository = repository_paths(&mut lines).await?;
        if lines.next().is_some() {
            return Err(ProbeError::InvalidOutput);
        }
        Ok(repository)
    }

    /// Checkout inspection additionally requires a resolved HEAD; an unborn
    /// registered repository is valid but cannot back an open workspace.
    pub(crate) async fn diagnostic_binding(
        &self,
        path: &Path,
    ) -> Result<DiagnosticGitBinding, ProbeError> {
        let mut command = self.diagnostic_command(path);
        command.args(["--symbolic-full-name", "HEAD"]);
        let output = capture_command(command).await?;
        let mut lines = output.lines();
        let repository = repository_paths(&mut lines).await?;
        let branch = lines.next().ok_or(ProbeError::InvalidOutput)?;
        if lines.next().is_some() {
            return Err(ProbeError::InvalidOutput);
        }
        let branch_name = if branch == "HEAD" {
            None
        } else {
            Some(
                branch
                    .strip_prefix("refs/heads/")
                    .ok_or(ProbeError::InvalidOutput)?
                    .to_owned(),
            )
        };
        Ok(DiagnosticGitBinding {
            root_path: repository.root_path,
            common_dir: repository.common_dir,
            branch_name,
        })
    }

    fn diagnostic_command(&self, path: &Path) -> Command {
        let mut command = Command::new(&self.executable);
        command
            .current_dir(path)
            .args([
                "--no-optional-locks",
                "rev-parse",
                "--path-format=absolute",
                "--show-toplevel",
                "--git-common-dir",
            ])
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("LC_ALL", "C");
        // Clear config-path overrides like ordinary Git execution, retaining
        // protected user configuration and its safe.directory trust decisions.
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

async fn repository_paths(
    lines: &mut std::str::Lines<'_>,
) -> Result<DiagnosticGitRepository, ProbeError> {
    let root = lines.next().ok_or(ProbeError::InvalidOutput)?;
    let common = lines.next().ok_or(ProbeError::InvalidOutput)?;
    if !Path::new(root).is_absolute() || !Path::new(common).is_absolute() {
        return Err(ProbeError::InvalidOutput);
    }
    Ok(DiagnosticGitRepository {
        root_path: tokio::fs::canonicalize(root)
            .await
            .map_err(|_| ProbeError::Unavailable)?,
        common_dir: tokio::fs::canonicalize(common)
            .await
            .map_err(|_| ProbeError::Unavailable)?,
    })
}

#[cfg(test)]
mod tests;
