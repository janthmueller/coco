use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail, ensure};
use tokio::process::Command;
use tokio::time::{sleep, timeout};

use super::{COMPATIBILITY_TIMEOUT, FORK_WORKSPACE_NAME, POLL_INTERVAL, TestPaths, shell_word};

const SESSION: &str = "coco-real-tui";
const EXIT_MARKER: &str = "__COCO_REAL_TUI_EXIT__=";
const INHERITED_HISTORY_MARKER: &str = "coco-native-history";
const TRUST_PROMPT: &str = "Do you trust the contents of this directory?";

pub(super) async fn verify_inherited_context_resume(
    paths: &TestPaths,
    codex_binary: &Path,
    repository: &Path,
) -> Result<()> {
    ensure_tmux_available().await?;
    let socket = paths.data_dir.join("real-tui.tmux.sock");
    let server = TmuxServer::new(socket);
    let command = jump_command(paths, codex_binary, repository);
    server.start().await?;
    server.send_literal(&command).await?;
    server.send_keys(&["Enter"]).await?;

    let deadline = Instant::now() + COMPATIBILITY_TIMEOUT;
    let mut accepted_trust_prompt = false;
    let screen = loop {
        let screen = server.capture().await?;
        if let Some(status) = exit_status(&screen) {
            bail!(
                "the real Codex TUI exited before rendering inherited history with status \
                 {status}:\n{screen}"
            );
        }
        if screen.contains(INHERITED_HISTORY_MARKER) {
            break screen;
        }
        if !accepted_trust_prompt && screen.contains(TRUST_PROMPT) {
            server.send_keys(&["Enter"]).await?;
            accepted_trust_prompt = true;
        }
        if Instant::now() >= deadline {
            bail!("the real Codex TUI did not render inherited history before timeout:\n{screen}");
        }
        sleep(POLL_INTERVAL).await;
    };
    ensure!(
        screen.contains(FORK_WORKSPACE_NAME),
        "the real Codex TUI rendered history without the inherited workspace name:\n{screen}"
    );

    server.send_keys(&["C-d"]).await?;
    let deadline = Instant::now() + COMPATIBILITY_TIMEOUT;
    loop {
        let screen = server.capture().await?;
        if let Some(status) = exit_status(&screen) {
            ensure!(
                status == 0,
                "the real Codex TUI exited unsuccessfully after Ctrl+D:\n{screen}"
            );
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!("the real Codex TUI did not exit after Ctrl+D:\n{screen}");
        }
        sleep(POLL_INTERVAL).await;
    }
}

pub(super) fn install_dummy_auth(paths: &TestPaths) -> Result<()> {
    let auth = paths.codex_home.join("auth.json");
    fs::write(
        &auth,
        r#"{"OPENAI_API_KEY":"sk-coco-compat-test","tokens":null,"last_refresh":null}"#,
    )?;
    fs::set_permissions(auth, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

async fn ensure_tmux_available() -> Result<()> {
    let output = timeout(
        COMPATIBILITY_TIMEOUT,
        Command::new("tmux").arg("-V").output(),
    )
    .await
    .context("timed out while checking tmux")?
    .context("the real TUI compatibility check requires tmux on PATH")?;
    ensure!(
        output.status.success(),
        "the real TUI compatibility check requires a working tmux: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn jump_command(paths: &TestPaths, codex_binary: &Path, repository: &Path) -> String {
    let coco = Path::new(env!("CARGO_BIN_EXE_coco"));
    let assignments = [
        ("HOME", paths.home.as_path()),
        ("CODEX_HOME", paths.codex_home.as_path()),
        ("COCO_CODEX_BINARY", codex_binary),
        ("COCO_DATA_DIR", paths.data_dir.as_path()),
        ("COCO_DATABASE_PATH", paths.database.as_path()),
        ("COCO_SOCKET_PATH", paths.socket.as_path()),
        ("COCO_CODEX_ENDPOINT_PATH", paths.endpoint.as_path()),
        ("COCO_CODEX_TOKEN_PATH", paths.token.as_path()),
        ("COCO_WORKTREES_DIR", paths.worktrees.as_path()),
    ]
    .into_iter()
    .map(|(name, value)| format!("{name}={}", shell_word(value)))
    .collect::<Vec<_>>()
    .join(" ");
    format!(
        "cd {} && env TERM=xterm-256color RUST_LOG=warn {assignments} {} jump {}; \
         status=$?; printf '\\n{EXIT_MARKER}%s\\n' \"$status\"",
        shell_word(repository),
        shell_word(coco),
        shell_word(Path::new(FORK_WORKSPACE_NAME)),
    )
}

fn exit_status(screen: &str) -> Option<i32> {
    screen
        .lines()
        .find_map(|line| line.trim().strip_prefix(EXIT_MARKER))
        .and_then(|status| status.parse().ok())
}

struct TmuxServer {
    socket: PathBuf,
}

impl TmuxServer {
    fn new(socket: PathBuf) -> Self {
        Self { socket }
    }

    async fn start(&self) -> Result<()> {
        self.run([
            "-f",
            "/dev/null",
            "new-session",
            "-d",
            "-s",
            SESSION,
            "-x",
            "120",
            "-y",
            "40",
            "bash --noprofile --norc",
        ])
        .await
        .map(|_| ())
    }

    async fn capture(&self) -> Result<String> {
        let output = self
            .run(["capture-pane", "-p", "-t", SESSION, "-S", "-"])
            .await?;
        String::from_utf8(output.stdout).context("tmux captured non-UTF-8 terminal output")
    }

    async fn send_keys(&self, keys: &[&str]) -> Result<()> {
        let mut arguments = vec!["send-keys", "-t", SESSION];
        arguments.extend_from_slice(keys);
        self.run(arguments).await.map(|_| ())
    }

    async fn send_literal(&self, value: &str) -> Result<()> {
        self.run(["send-keys", "-l", "-t", SESSION, value])
            .await
            .map(|_| ())
    }

    async fn run<I, S>(&self, arguments: I) -> Result<std::process::Output>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let output = timeout(
            COMPATIBILITY_TIMEOUT,
            Command::new("tmux")
                .arg("-S")
                .arg(&self.socket)
                .args(arguments)
                .output(),
        )
        .await
        .context("tmux command timed out")??;
        ensure!(
            output.status.success(),
            "tmux command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(output)
    }
}

impl Drop for TmuxServer {
    fn drop(&mut self) {
        let _ = std::process::Command::new("tmux")
            .arg("-S")
            .arg(&self.socket)
            .arg("kill-server")
            .output();
    }
}
