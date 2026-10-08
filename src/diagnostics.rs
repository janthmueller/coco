//! Bounded read-only probes shared by diagnostic adapters. No raw subprocess
//! output is suitable for a report until its expected metadata is validated.

use std::io;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, Command};
use tokio::time::timeout;

pub(crate) const PROBE_TIMEOUT: Duration = Duration::from_secs(3);
pub(crate) const REPORT_TIMEOUT: Duration = Duration::from_secs(20);
pub(crate) const MAX_REPOSITORIES: usize = 32;
pub(crate) const MAX_WORKSPACES: usize = 64;
const MAX_OUTPUT_BYTES: usize = 32 * 1024;
const REAP_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProbeError {
    Unavailable,
    Failed,
    TimedOut,
    InvalidOutput,
}

pub(crate) async fn capture_command(mut command: Command) -> Result<String, ProbeError> {
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let mut owned = ProbeChild(Some(command.spawn().map_err(|_| ProbeError::Unavailable)?));
    let child = owned.0.as_mut().expect("owned diagnostic child");
    let stdout = child.stdout.take().expect("piped diagnostic stdout");
    let result = timeout(PROBE_TIMEOUT, async {
        let (status, output) = tokio::try_join!(child.wait(), read_output(stdout))?;
        Ok::<_, io::Error>((status, output))
    })
    .await;
    match result {
        Ok(Ok((status, output))) if status.success() => {
            String::from_utf8(output).map_err(|_| ProbeError::InvalidOutput)
        }
        Ok(Ok(_)) => Err(ProbeError::Failed),
        Ok(Err(_)) => {
            stop_child(child).await;
            Err(ProbeError::InvalidOutput)
        }
        Err(_) => {
            stop_child(child).await;
            Err(ProbeError::TimedOut)
        }
    }
}

struct ProbeChild(Option<Child>);

impl Drop for ProbeChild {
    fn drop(&mut self) {
        let Some(mut child) = self.0.take().filter(|child| child.id().is_some()) else {
            return;
        };
        let _ = child.start_kill();
        // Keep the handle until wait finishes instead of relying solely on
        // Tokio's best-effort orphan queue after cancellation. kill_on_drop
        // remains the fallback when the runtime itself is shutting down.
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _ = timeout(REAP_TIMEOUT, child.wait()).await;
            });
        }
    }
}

async fn stop_child(child: &mut Child) {
    let _ = child.start_kill();
    let _ = timeout(REAP_TIMEOUT, child.wait()).await;
}

async fn read_output(reader: impl AsyncRead + Unpin) -> io::Result<Vec<u8>> {
    let mut output = Vec::new();
    reader
        .take((MAX_OUTPUT_BYTES + 1) as u64)
        .read_to_end(&mut output)
        .await?;
    if output.len() > MAX_OUTPUT_BYTES {
        return Err(io::Error::other("diagnostic output exceeded its bound"));
    }
    Ok(output)
}

pub(crate) fn parse_version(output: &str, prefix: &str) -> Option<String> {
    let value = output.trim().strip_prefix(prefix)?;
    let value = if prefix == "git version " {
        // Apple Git appends its vendor/build identifier after the version.
        value.split_whitespace().next()?
    } else {
        value
    };
    valid_version(value).then(|| value.to_owned())
}

pub(crate) fn valid_version(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.as_bytes()[0].is_ascii_digit()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
        && value.split(['-', '+']).next().is_some_and(|core| {
            let parts: Vec<_> = core.split('.').collect();
            parts.len() >= 3
                && parts
                    .iter()
                    .take(3)
                    .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        })
}

#[cfg(test)]
mod tests;
