use super::super::progress::Progress;
use super::*;
use std::process::{Output, Stdio};

pub(super) struct Fixture {
    _temporary: tempfile::TempDir,
    pub paths: TestPaths,
    pub binary: PathBuf,
    pub first: PathBuf,
    pub second: PathBuf,
    pub log: PathBuf,
}

impl Fixture {
    pub async fn new() -> Result<Self> {
        let source = std::env::var_os("COCO_LIVE_CODEX_HOME")
            .map(PathBuf::from)
            .context("set COCO_LIVE_CODEX_HOME to the approved auth home")?;
        let source = source.join("auth.json");
        ensure!(
            fs::metadata(&source)?.is_file() && fs::metadata(&source)?.len() <= 64 * 1024,
            "the selected authentication cache is not a bounded regular file"
        );
        let temporary = tempfile::tempdir()?;
        let paths = TestPaths::new(temporary.path());
        for directory in [
            paths.home.clone(),
            paths.codex_home.clone(),
            paths.data_dir.clone(),
            paths.data_dir.join("xdg-runtime"),
        ] {
            fs::create_dir_all(&directory)?;
            fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
        }
        let auth = paths.codex_home.join("auth.json");
        fs::copy(source, &auth)?;
        fs::set_permissions(auth, fs::Permissions::from_mode(0o600))?;
        fs::write(
            paths.codex_home.join("config.toml"),
            "cli_auth_credentials_store='file'\ncheck_for_update_on_startup=false\nmodel_reasoning_effort='low'\napproval_policy='never'\nsandbox_mode='read-only'\n[analytics]\nenabled=false\n[features]\napps=false\nconnectors=false\nmemories=false\nmemory_tool=false\nhooks=false\nplugins=false\nshell_tool=false\nshell_snapshot=false\nremote_plugin=false\nskill_mcp_dependency_install=false\n",
        )?;
        let first = temporary.path().join("frontend");
        let second = temporary.path().join("backend");
        prepare_repository(&first)?;
        prepare_repository(&second)?;
        trust_projects(&paths, &[&first, &second])?;
        let binary = std::env::var_os(CODEX_BINARY_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(|| "codex".into());
        verify_codex_version(&binary, &paths).await?;
        let log = paths.data_dir.join("live-ai-daemon.log");
        let fixture = Self {
            _temporary: temporary,
            paths,
            binary,
            first,
            second,
            log,
        };
        let mut login = fixture.command(&fixture.binary);
        login.args(["login", "status"]);
        let output = timeout(COMPATIBILITY_TIMEOUT, login.output()).await??;
        ensure!(
            output.status.success(),
            "the copied Codex login is unavailable; no login flow was started"
        );
        Ok(fixture)
    }

    fn command(&self, program: &Path) -> Command {
        let mut command = Command::new(program);
        self.paths.apply(&mut command, &self.binary);
        command
            .env("XDG_CONFIG_HOME", self.paths.home.join(".config"))
            .env("XDG_DATA_HOME", self.paths.home.join(".local/share"))
            .env("XDG_CACHE_HOME", self.paths.home.join(".cache"))
            .env("XDG_RUNTIME_DIR", self.paths.data_dir.join("xdg-runtime"))
            .env(
                "COCO_HOOKS_PATH",
                self.paths.home.join(".config/coco/hooks.json"),
            )
            .env("CODEX_SQLITE_HOME", &self.paths.codex_home)
            .env("COCO_WORKSPACE_EXECUTION", "shared")
            .env("COCO_WORKSPACE_CONTAINMENT", "process-tree")
            .env_remove("OPENAI_API_KEY")
            .env_remove("CODEX_API_KEY")
            .env_remove("OPENAI_BASE_URL")
            .env_remove("CODEX_THREAD_ID")
            .env_remove("CODEX_SESSION_ID")
            .env_remove("BASH_ENV")
            .env_remove("ENV")
            .kill_on_drop(true);
        command
    }

    pub fn spawn_daemon(&self) -> Result<Child> {
        self.command(Path::new(env!("CARGO_BIN_EXE_cocod")))
            .stdout(Stdio::null())
            .stderr(Stdio::from(fs::File::create(&self.log)?))
            .spawn()
            .context("could not start the isolated live coordinator")
    }

    pub async fn cli(&self, repo: &Path, args: &[&str]) -> Result<Output> {
        let output = timeout(
            Duration::from_secs(90),
            self.command(Path::new(env!("CARGO_BIN_EXE_coco")))
                .args(args)
                .current_dir(repo)
                .stdin(Stdio::null())
                .output(),
        )
        .await??;
        ensure!(
            output.status.success(),
            "isolated coco {} failed (status {}); no automatic retry",
            args[0],
            output.status
        );
        Ok(output)
    }

    pub async fn status(&self, repo: &Path, workspace: &str) -> Result<Value> {
        let output = self.cli(repo, &["status", workspace, "--json"]).await?;
        Ok(serde_json::from_slice(&output.stdout)?)
    }

    fn dialog(&self, model: &str, mode: &str, prompt: &str) -> Result<String> {
        terminal_script(
            &self.paths.data_dir.join("ai-dialog.sh"),
            &self.dialog_command(model, mode, prompt),
        )
    }

    fn dialog_command(&self, model: &str, mode: &str, prompt: &str) -> String {
        let environment = self.command(Path::new("/unused-live-environment"));
        let (unset, values): (Vec<_>, Vec<_>) = environment
            .as_std()
            .get_envs()
            .partition(|(_, value)| value.is_none());
        let unset = unset
            .into_iter()
            .map(|(key, _)| format!("-u {}", key.to_string_lossy()))
            .collect::<Vec<_>>()
            .join(" ");
        let values = values
            .into_iter()
            .map(|(key, value)| {
                format!(
                    "{}={}",
                    key.to_string_lossy(),
                    shell_word(Path::new(value.unwrap()))
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        format!(
            "cd {} && env {unset} TERM=xterm-256color {values} {} ai -m {} {mode} {}; status=$?; printf '\\n{EXIT_MARKER}%s\\n' \"$status\"",
            shell_word(&self.first),
            shell_word(Path::new(env!("CARGO_BIN_EXE_coco"))),
            shell_word(Path::new(model)),
            shell_word(Path::new(prompt))
        )
    }
}

/// Send only a short path through the initial terminal's bounded line input.
/// The environment/script contains synthetic paths, never credential contents.
fn terminal_script(path: &Path, command: &str) -> Result<String> {
    fs::write(path, command)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(format!(
        "env -u BASH_ENV -u ENV bash --noprofile --norc {}",
        shell_word(path)
    ))
}

pub(super) async fn inventory(fixture: &Fixture) -> Result<Vec<(String, String, String)>> {
    let output = fixture
        .cli(&fixture.first, &["list", "-a", "--json"])
        .await?;
    let value: Value = serde_json::from_slice(&output.stdout)?;
    let mut workspaces = value["workspaces"]
        .as_array()
        .context("workspace inventory missing")?
        .iter()
        .map(|workspace| {
            Ok((
                workspace["id"]
                    .as_str()
                    .context("workspace ID missing")?
                    .into(),
                workspace["name"]
                    .as_str()
                    .context("workspace name missing")?
                    .into(),
                workspace["repositoryId"]
                    .as_str()
                    .context("workspace repository missing")?
                    .into(),
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    workspaces.sort();
    Ok(workspaces)
}

pub(super) async fn run_dialog(
    fixture: &Fixture,
    model: &str,
    label: &str,
    mode: &str,
    prompt: &str,
    confirmation: Option<bool>,
) -> Result<()> {
    let server = TmuxServer::new(
        fixture
            .paths
            .data_dir
            .join(format!("live-ai-{label}.tmux.sock")),
    );
    let progress = Progress::snapshot(&fixture.paths, &fixture.first)?;
    start_jump(&server, &fixture.dialog(model, mode, prompt)?).await?;
    if let Some(approve) = confirmation {
        let screen = wait(&server, label, true, &progress).await?;
        let workspace = if approve {
            "review/context"
        } else {
            "review/declined"
        };
        ensure!(
            screen.contains("frontend")
                && screen.contains(workspace)
                && screen.contains("main")
                && screen.contains("› 2. False"),
            "wrong action/default offered by real model"
        );
        if approve {
            ensure!(
                screen.contains("context/source"),
                "model offered the wrong conversation context"
            );
            server.send_keys(&["Up"]).await?;
            let deadline = Instant::now() + Duration::from_secs(5);
            while !pane(&server).await?.contains("› 1. True") {
                ensure!(
                    Instant::now() < deadline,
                    "native confirmation did not select True"
                );
                sleep(Duration::from_millis(50)).await;
            }
        }
        server.send_keys(&["Enter"]).await?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while pending_confirmation(&pane(&server).await?) {
            ensure!(
                Instant::now() < deadline,
                "native confirmation was not dismissed"
            );
            sleep(Duration::from_millis(50)).await;
        }
    }
    wait(&server, label, false, &progress).await?;
    cleanup(close_tui(&server, label), LiveFailure::Terminal).await?;
    Ok(())
}

async fn wait(server: &TmuxServer, label: &str, form: bool, progress: &Progress) -> Result<String> {
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let screen = pane(server).await?;
        let pending = pending_confirmation(&screen);
        let completed = progress.completed()?;
        if (form && pending) || (!form && completed && !pending) {
            return Ok(screen);
        }
        ensure!(
            form || !pending,
            "unexpected additional confirmation in {label}; not auto-approved"
        );
        ensure!(
            !form || !completed,
            "live {label} completed without offering the requested confirmation"
        );
        ensure!(
            exit_status(&screen).is_none(),
            "native live {label} exited before completion"
        );
        ensure!(
            !screen.contains("Sign in with ChatGPT"),
            "live authentication needs user intervention"
        );
        ensure!(
            Instant::now() < deadline,
            "live {label} timed out waiting for {}; screen signals: {}; no transcript or credentials printed",
            if form { "confirmation" } else { "completion" },
            screen_signals(&screen)
        );
        sleep(Duration::from_millis(500)).await;
    }
}

async fn pane(server: &TmuxServer) -> Result<String> {
    let output = server.run(["capture-pane", "-p", "-t", SESSION]).await?;
    Ok(String::from_utf8(output.stdout)?)
}

fn pending_confirmation(screen: &str) -> bool {
    screen.contains("Run this action?") && screen.contains("enter to submit | esc to cancel")
}

/// Only fixed labels/booleans may escape the private authenticated fixture.
fn screen_signals(screen: &str) -> String {
    [
        ("confirmation_title", "Run this action?"),
        ("confirmation_footer", "enter to submit | esc to cancel"),
        ("false_option", "2. False"),
        ("working", "Working"),
        ("error", "Error"),
        ("connection_failed", "Connection failed"),
        ("login", "Sign in"),
    ]
    .map(|(label, marker)| format!("{label}={}", screen.contains(marker)))
    .join(", ")
}

#[test]
fn live_diagnostics_do_not_include_terminal_content() {
    let private = "synthetic-private-value-not-for-output";
    let summary = screen_signals(&format!("Working\n{private}\nRun this action?"));
    assert!(summary.contains("working=true"));
    assert!(summary.contains("confirmation_title=true"));
    assert!(summary.contains("confirmation_footer=false"));
    assert!(!summary.contains(private));
}

#[test]
fn live_confirmation_waits_for_its_input_footer() {
    assert!(!pending_confirmation("Run this action?\n2. False"));
    assert!(!pending_confirmation("enter to submit | esc to cancel"));
    assert!(pending_confirmation(
        "Run this action?\n› 2. False\nenter to submit | esc to cancel"
    ));
}

#[tokio::test]
#[ignore = "requires COCO_RUN_REAL_CODEX_COMPAT=1 and tmux; terminal transport only, no model or authentication"]
async fn isolated_terminal_launch_preserves_long_commands() -> Result<()> {
    require_explicit_opt_in()?;
    ensure_tmux_available().await?;
    let temporary = tempfile::tempdir()?;
    let paths = TestPaths::new(temporary.path());
    fs::create_dir_all(&paths.data_dir)?;
    let fixture = Fixture {
        first: temporary.path().into(),
        second: temporary.path().into(),
        log: paths.data_dir.join("unused.log"),
        binary: "codex".into(),
        paths,
        _temporary: temporary,
    };
    // Substitute only the executable; no AI, auth or daemon is involved.
    let command = fixture
        .dialog_command("unused", "", &"x".repeat(6 * 1024))
        .replace(&shell_word(Path::new(env!("CARGO_BIN_EXE_coco"))), "true");
    let command = terminal_script(&fixture.paths.data_dir.join("transport.sh"), &command)?;
    let server = TmuxServer::new(fixture.paths.data_dir.join("long-command.sock"));
    start_jump(&server, &command).await?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let screen = pane(&server).await?;
        if let Some(status) = exit_status(&screen) {
            ensure!(status == 0, "terminal transport exited unsuccessfully");
            return Ok(());
        }
        ensure!(
            Instant::now() < deadline,
            "long terminal launch did not reach the child"
        );
        sleep(Duration::from_millis(50)).await;
    }
}
