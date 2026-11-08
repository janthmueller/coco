use super::*;
use crate::{
    CODEX_BINARY_ENV, RealAppServer, prepare_repository, require_explicit_opt_in, run_cli,
    spawn_daemon, stop_daemon, verify_codex_version, wait_for_file, workspace_status_for,
};
use std::env;

#[path = "temporary/source.rs"]
mod source;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires COCO_RUN_REAL_CODEX_COMPAT=1, selected Codex and tmux; no model calls"]
async fn native_empty_temporary_tuis_cleanup_after_quit_ctrl_d_and_ctrl_c() -> Result<()> {
    verify_temporary_tuis(false).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires COCO_RUN_REAL_CODEX_COMPAT=1, selected Codex and tmux; shell-only source, no model calls"]
async fn native_forked_temporary_tuis_cleanup_after_quit_ctrl_d_and_ctrl_c() -> Result<()> {
    verify_temporary_tuis(true).await
}

async fn verify_temporary_tuis(inherit_context: bool) -> Result<()> {
    require_explicit_opt_in()?;
    ensure_tmux_available().await?;
    let binary = env::var_os(CODEX_BINARY_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| "codex".into());
    let temporary = tempfile::tempdir()?;
    let paths = TestPaths::new(temporary.path());
    for directory in [&paths.home, &paths.codex_home, &paths.data_dir] {
        fs::create_dir_all(directory)?;
    }
    verify_codex_version(&binary, &paths).await?;
    let repository = temporary.path().join("repo");
    prepare_repository(&repository)?;
    install_dummy_auth(&paths)?;
    fs::write(
        paths.codex_home.join("config.toml"),
        "model = \"coco-empty-probe\"\nmodel_provider = \"coco-probe\"\n\
         [model_providers.coco-probe]\nname = \"coco-probe\"\n\
         base_url = \"http://127.0.0.1:1/v1\"\nwire_api = \"responses\"\n\
         requires_openai_auth = false\nsupports_websockets = false\n",
    )?;
    trust_projects(&paths, &[&repository])?;
    let inherited = if inherit_context {
        Some(prepare_context(&paths, &binary, &repository).await?)
    } else {
        None
    };
    let log = paths.data_dir.join("temporary-daemon.log");
    let mut daemon = spawn_daemon(&paths, &binary, &log)?;
    wait_for_file(&paths.socket, &mut daemon, &log).await?;
    let source = if let Some(context) = inherited {
        Some(source::Source::create(&paths, &binary, &repository, &context).await?)
    } else {
        None
    };
    let proof = verify_exits(&paths, &binary, &repository, source.as_ref()).await;
    let preserved = if let Some(source) = source {
        source.verify(&paths, &binary, &repository).await
    } else {
        Ok(())
    };
    let stopped = stop_daemon(&mut daemon, &log).await;
    preserved?;
    proof?;
    stopped
}

async fn prepare_context(paths: &TestPaths, binary: &Path, repository: &Path) -> Result<String> {
    let mut native = RealAppServer::spawn(paths, binary, repository, "temporary-source").await?;
    let started = native
        .request(
            "thread/start",
            serde_json::json!({
                "cwd": repository, "model": "coco-empty-probe", "ephemeral": false
            }),
        )
        .await?;
    let id = started["thread"]["id"]
        .as_str()
        .context("source thread missing")?
        .to_owned();
    native.request("thread/shellCommand", serde_json::json!({
        "threadId": id, "command": "printf coco-inherited-temporary-context", "timeoutMs": 10000
    })).await?;
    native
        .wait_for_thread_notification("turn/completed", &id)
        .await?;
    native.stop().await?;
    Ok(format!("thread:{id}"))
}

async fn verify_exits(
    paths: &TestPaths,
    binary: &Path,
    repository: &Path,
    context: Option<&source::Source>,
) -> Result<()> {
    for (name, exit) in [
        ("scratch/ctrl-d", "C-d"),
        ("scratch/quit", "/quit"),
        ("scratch/ctrl-c", "C-c"),
    ] {
        let mut arguments = vec!["create", name, "--temporary"];
        if let Some(context) = context {
            arguments.extend(["--context", context.workspace_id.as_str()]);
        }
        run_cli(paths, binary, repository, &arguments).await?;
        let status = workspace_status_for(paths, binary, repository, name).await?;
        if let Some(context) = context {
            ensure!(
                status["workspace"]["contextMode"] == "fork",
                "context was not inherited"
            );
            ensure!(status["workspace"]["parentThreadId"] == context.thread_id);
            ensure!(status["workspace"]["codexThreadId"] != context.thread_id);
            ensure!(
                status["workspace"]["codexThreadId"].is_string(),
                "fork was not bound at create time"
            );
            ensure!(
                status["workspace"]["temporary"].is_object(),
                "cleanup policy missing at creation"
            );
        }
        let worktree = PathBuf::from(
            status["workspace"]["worktreePath"]
                .as_str()
                .context("missing worktree")?,
        );
        trust_projects(paths, &[&worktree])?;
        let server = TmuxServer::new(
            paths
                .data_dir
                .join(format!("{}.tmux.sock", exit.replace('/', ""))),
        );
        start_jump(&server, &command(paths, binary, repository, name)).await?;
        wait_for_prompt(&server).await?;
        if exit == "/quit" {
            // Type the command as separate human-paced key events so native
            // paste detection cannot reinterpret the synthetic input.
            for character in exit.chars() {
                server.send_literal(&character.to_string()).await?;
                sleep(std::time::Duration::from_millis(40)).await;
            }
            server.send_keys(&["Enter"]).await?;
        } else {
            server.send_keys(&[exit]).await?;
        }
        let screen = wait_for_exit(&server, exit == "C-c").await?;
        ensure!(exit_status(&screen) == Some(0), "{exit} failed:\n{screen}");
        ensure!(
            screen.contains(&format!("Removed {name}")),
            "cleanup not reported after {exit}:\n{screen}"
        );
        ensure!(!worktree.exists(), "worktree retained after {exit}");
        let list = run_cli(paths, binary, repository, &["list", "--json"]).await?;
        let list: serde_json::Value = serde_json::from_slice(&list.stdout)?;
        let remaining = list["workspaces"]
            .as_array()
            .context("workspace list missing")?;
        if let Some(source) = context {
            ensure!(
                remaining.len() == 1 && remaining[0]["id"] == source.workspace_id,
                "temporary cleanup changed the original workspace list: {list}"
            );
        } else {
            ensure!(remaining.is_empty());
        }
    }
    Ok(())
}

fn command(paths: &TestPaths, binary: &Path, repository: &Path, name: &str) -> String {
    let mut cli = Command::new(env!("CARGO_BIN_EXE_coco"));
    paths.apply(&mut cli, binary);
    let environment = cli
        .as_std()
        .get_envs()
        .filter_map(|(name, value)| {
            value.map(|value| {
                format!(
                    "{}={}",
                    name.to_string_lossy(),
                    shell_word(Path::new(value))
                )
            })
        })
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "cd {} && env TERM=xterm-256color {environment} {} jump {}; coco_test_exit=$?; printf '\\n{EXIT_MARKER}%s\\n' \"$coco_test_exit\"",
        shell_word(repository),
        shell_word(Path::new(env!("CARGO_BIN_EXE_coco"))),
        shell_word(Path::new(name))
    )
}

async fn wait_for_prompt(server: &TmuxServer) -> Result<()> {
    let deadline = Instant::now() + COMPATIBILITY_TIMEOUT;
    loop {
        let screen = server.capture().await?;
        ensure!(
            exit_status(&screen).is_none(),
            "TUI exited before input:\n{screen}"
        );
        if screen.contains("coco-empty-probe") && screen.contains("for shortcuts") {
            return Ok(());
        }
        ensure!(
            Instant::now() < deadline,
            "TUI did not become ready:\n{screen}"
        );
        sleep(POLL_INTERVAL).await;
    }
}

async fn wait_for_exit(server: &TmuxServer, mut confirm_ctrl_c: bool) -> Result<String> {
    let deadline = Instant::now() + COMPATIBILITY_TIMEOUT;
    loop {
        let screen = server.capture().await?;
        if exit_status(&screen).is_some() {
            return Ok(screen);
        }
        let lower = screen.to_lowercase();
        if confirm_ctrl_c && (lower.contains("ctrl+c again") || lower.contains("ctrl-c again")) {
            server.send_keys(&["C-c"]).await?;
            confirm_ctrl_c = false;
        }
        ensure!(Instant::now() < deadline, "TUI did not exit:\n{screen}");
        sleep(POLL_INTERVAL).await;
    }
}
