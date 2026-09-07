use std::ffi::OsStr;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::thread;

use super::{Git, GitError};

#[derive(Debug)]
pub(super) struct CommandOutput {
    pub(super) status: ExitStatus,
    pub(super) stdout: BoundedBytes,
    pub(super) stderr: BoundedBytes,
}

#[derive(Debug)]
pub(super) struct BoundedBytes {
    pub(super) bytes: Vec<u8>,
    pub(super) truncated: bool,
}

impl Git {
    pub(super) fn run<I, S>(
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

    pub(super) fn run_text<I, S>(
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

    pub(super) fn run_with_input<I, S>(
        &self,
        cwd: &Path,
        category: &'static str,
        args: I,
        input: &[u8],
    ) -> Result<CommandOutput, GitError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let output = self.execute_with_input(cwd, category, args, input)?;
        ensure_success(category, &output)?;
        if output.stdout.truncated || output.stderr.truncated {
            return Err(GitError::OutputTooLarge {
                category,
                limit: self.capture_limit,
            });
        }
        Ok(output)
    }

    pub(super) fn execute<I, S>(
        &self,
        cwd: &Path,
        _category: &'static str,
        args: I,
    ) -> Result<CommandOutput, GitError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.execute_inner(cwd, args, None)
    }

    pub(super) fn execute_with_input<I, S>(
        &self,
        cwd: &Path,
        _category: &'static str,
        args: I,
        input: &[u8],
    ) -> Result<CommandOutput, GitError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.execute_inner(cwd, args, Some(input))
    }

    fn execute_inner<I, S>(
        &self,
        cwd: &Path,
        args: I,
        input: Option<&[u8]>,
    ) -> Result<CommandOutput, GitError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = Command::new(&self.executable);
        command
            .current_dir(cwd)
            .args(args)
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
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
        let input_result =
            input.map(|input| child.stdin.take().expect("piped stdin").write_all(input));
        let status = child.wait().map_err(|source| GitError::Io {
            path: cwd.to_owned(),
            source,
        })?;
        if let Some(Err(source)) = input_result {
            return Err(GitError::Io {
                path: cwd.to_owned(),
                source,
            });
        }
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

pub(super) fn ensure_success(
    category: &'static str,
    output: &CommandOutput,
) -> Result<(), GitError> {
    if output.status.success() {
        Ok(())
    } else {
        Err(command_failed(category, output))
    }
}

pub(super) fn command_failed(category: &'static str, output: &CommandOutput) -> GitError {
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

pub(super) fn one_line_metadata(
    output: CommandOutput,
    category: &'static str,
) -> Result<String, GitError> {
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
