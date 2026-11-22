use super::*;
use crate::mcp_client::McpClient;
use crate::{
    CODEX_BINARY_ENV, HISTORY_COMMAND, RemoteAppServer, WORKSPACE_NAME, begin_fresh_attach,
    codex_compat, daemon_request, prepare_repository, release_attach, require_explicit_opt_in,
    run_cli, spawn_daemon, start_remote_thread, stop_daemon, verify_codex_version,
    verify_runtime_cleanup, wait_for_file, wait_for_remote_adoption,
};
use serde_json::json;

const RESOURCE: &str = "ui://coco/workspace-statusline";

#[path = "../../../src/codex_compat.rs"]
mod verified_releases;

fn footer_applicable(expected: &str) -> Result<bool> {
    let version = expected.strip_prefix("codex-cli ");
    ensure!(
        verified_releases::is_verified(version),
        "unverified native footer baseline"
    );
    match version {
        Some("0.171.0") => Ok(true),
        Some("0.166.0" | "0.169.0") => Ok(false),
        // Extending general compatibility must not silently skip review of a
        // new release's independent footer contract.
        _ => bail!("native footer applicability has not been reviewed"),
    }
}

#[test]
fn footer_applicability_skips_verified_older_releases_but_rejects_unverified_ones() {
    for version in verified_releases::VERIFIED_VERSIONS {
        assert_eq!(
            footer_applicable(&format!("codex-cli {version}")).unwrap(),
            *version == "0.171.0"
        );
    }
    for unknown in [
        "codex-cli 0.170.0",
        "codex-cli 0.172.0",
        "codex-cli 0.171.0-custom",
        "0.171.0",
    ] {
        assert!(footer_applicable(unknown).is_err());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires COCO_RUN_REAL_CODEX_COMPAT=1, Codex 0.171.0 and tmux; real native footer, shell-only history, no model"]
async fn native_remote_footer_renders_the_exact_workspace_and_survives_unavailable_coco()
-> Result<()> {
    require_explicit_opt_in()?;
    let expected = codex_compat::expected_version();
    if !footer_applicable(&expected)? {
        eprintln!(
            "Skipping native statusline proof for {expected}: extension is verified only on 0.171.0; the baseline compatibility tests still apply."
        );
        return Ok(());
    }
    ensure_tmux_available().await?;
    let temporary = tempfile::tempdir()?;
    let paths = TestPaths::new(temporary.path());
    for directory in [&paths.home, &paths.codex_home, &paths.data_dir] {
        fs::create_dir_all(directory)?;
    }
    let binary = std::env::var_os(CODEX_BINARY_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| "codex".into());
    verify_codex_version(&binary, &paths).await?;
    let repository = temporary.path().join("repository");
    prepare_repository(&repository)?;
    install(&paths, &repository)?;
    install_dummy_auth(&paths)?;
    trust_projects(&paths, &[&repository])?;
    let log = paths.data_dir.join("statusline-daemon.log");
    let mut daemon = spawn_daemon(&paths, &binary, &log)?;
    wait_for_file(&paths.socket, &mut daemon, &log).await?;
    let prepared = prepare(&paths, &binary, &repository).await;
    if let Err(error) = prepared {
        stop_daemon(&mut daemon, &log).await?;
        return Err(error);
    }
    let first = verify_wire(&paths, &binary, &repository).await;
    let first = match first {
        Ok(()) => {
            let measured = measure_warm_preparation(&paths, &repository).await;
            match measured {
                Ok(()) => verify_tui(&paths, &binary, &repository, false, "warm first TUI").await,
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error),
    };
    let first = match first {
        Ok(()) => verify_tui(&paths, &binary, &repository, false, "warm repeat TUI").await,
        Err(error) => Err(error),
    };
    stop_daemon(&mut daemon, &log).await?;
    first?;
    select(&paths, "coco_offline")?;
    let mut daemon = spawn_daemon(&paths, &binary, &log)?;
    wait_for_file(&paths.socket, &mut daemon, &log).await?;
    let second = verify_tui(
        &paths,
        &binary,
        &repository,
        true,
        "restarted daemon, offline footer",
    )
    .await;
    stop_daemon(&mut daemon, &log).await?;
    second?;
    verify_runtime_cleanup(&paths)
}

fn install(paths: &TestPaths, repository: &Path) -> Result<()> {
    let server = |socket: &Path| {
        json!({
            "command":env!("CARGO_BIN_EXE_coco"),
            "args":["mcp", "serve", "--repository", repository],
            "env":{"COCO_SOCKET_PATH":socket, "COCO_DATA_DIR":paths.data_dir},
        })
    };
    let config = json!({
        "model":"footer-proof", "model_provider":"footer-proof", "check_for_update_on_startup":false,
        "analytics":{"enabled":false},
        "model_providers":{"footer-proof":{"name":"No-inference footer fixture", "base_url":"http://127.0.0.1:1/v1",
            "wire_api":"responses", "requires_openai_auth":false, "supports_websockets":false}},
        "features":{"tui_extensions_statusline":true, "remote_plugin":false, "skill_mcp_dependency_install":false},
        "tui":{"status_line_provider":{"server":"coco_footer", "tool":"workspaces.status"},
            "status_line":["model-with-reasoning","context-remaining"], "show_tooltips":false},
        "mcp_servers":{"coco_footer":server(&paths.socket), "coco_offline":server(&paths.data_dir.join("not-running.sock"))},
    });
    fs::write(
        paths.codex_home.join("config.toml"),
        toml::to_string(&config)?,
    )?;
    Ok(())
}

fn select(paths: &TestPaths, server: &str) -> Result<()> {
    let path = paths.codex_home.join("config.toml");
    let mut config: toml::Table = toml::from_str(&fs::read_to_string(&path)?)?;
    config.get_mut("tui").context("missing TUI config")?["status_line_provider"]["server"] =
        toml::Value::String(server.into());
    fs::write(path, toml::to_string(&config)?)?;
    Ok(())
}

async fn prepare(paths: &TestPaths, binary: &Path, repository: &Path) -> Result<()> {
    run_cli(
        paths,
        binary,
        repository,
        &["create", WORKSPACE_NAME, "--model", "footer-proof"],
    )
    .await?;
    let lease = begin_fresh_attach(paths, repository).await?;
    let mut remote = RemoteAppServer::connect_with_capabilities(paths, true).await?;
    let thread = start_remote_thread(&mut remote, &lease, "footer-proof", "low", false).await?;
    remote
        .request(
            "thread/shellCommand",
            json!({"threadId":thread, "command":HISTORY_COMMAND}),
        )
        .await?;
    remote
        .wait_for_thread_notification("turn/completed", &thread)
        .await?;
    wait_for_remote_adoption(paths, &lease, &thread).await?;
    remote.close(&thread).await?;
    release_attach(paths, &lease).await?;
    run_cli(
        paths,
        binary,
        repository,
        &[
            "create",
            FORK_WORKSPACE_NAME,
            "--context",
            WORKSPACE_NAME,
            "--model",
            "footer-proof",
        ],
    )
    .await?;
    for name in [WORKSPACE_NAME, FORK_WORKSPACE_NAME] {
        let status = crate::workspace_status_for(paths, binary, repository, name).await?;
        trust_projects(
            paths,
            &[Path::new(
                status["workspace"]["worktreePath"]
                    .as_str()
                    .context("worktree missing")?,
            )],
        )?;
    }
    Ok(())
}

async fn verify_wire(paths: &TestPaths, binary: &Path, repository: &Path) -> Result<()> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_coco"));
    paths.apply(&mut command, binary);
    command
        .args(["mcp", "serve", "--repository"])
        .arg(repository);
    let mut client = McpClient::spawn(&mut command).await?;
    let listed = client.request("resources/list", json!({})).await?;
    ensure!(listed["resources"][0]["uri"] == RESOURCE);
    for name in [WORKSPACE_NAME, FORK_WORKSPACE_NAME, WORKSPACE_NAME] {
        let status = crate::workspace_status_for(paths, binary, repository, name).await?;
        let resource = client
            .request(
                "resources/read",
                json!({"uri":RESOURCE, "_meta":{
                    "com.openai/codex-statusline-state":{"thread":{
                        "id":status["workspace"]["codexThreadId"],
                        "model":{"display_name":"Wire fixture model", "reasoning_level":"high"},
                        "usage":{"context":{"used_percentage":25}},
                    }},
                }}),
            )
            .await?;
        let text = resource["contents"][0]["text"]
            .as_str()
            .context("resource text missing")?;
        ensure!(
            text.contains(name)
                && text.contains("Wire fixture model high")
                && text.contains("Ctx 75% left"),
            "stdio resource lost exact displayed-thread scope: {text}"
        );
    }
    client.stop().await
}

async fn verify_tui(
    paths: &TestPaths,
    binary: &Path,
    repository: &Path,
    offline: bool,
    label: &str,
) -> Result<()> {
    let server = TmuxServer::new(paths.data_dir.join(if offline {
        "footer-offline.sock"
    } else {
        "footer.sock"
    }));
    let started = Instant::now();
    let mut header_at = None;
    let mut history_at = None;
    start_jump(&server, &jump_command(paths, binary, repository)).await?;
    let deadline = Instant::now() + COMPATIBILITY_TIMEOUT;
    loop {
        let screen = server.capture().await?;
        ensure!(
            exit_status(&screen).is_none(),
            "native footer TUI exited: {screen}"
        );
        let footer = if offline {
            screen.contains("CoCo unavailable")
        } else if cfg!(target_os = "linux") {
            screen.contains("RAM ") || screen.contains("RSS ")
        } else {
            screen.contains(&format!("{FORK_WORKSPACE_NAME} ·"))
        };
        if screen.contains("OpenAI Codex") {
            header_at.get_or_insert_with(|| started.elapsed());
        }
        if screen.contains(INHERITED_HISTORY_MARKER) {
            history_at.get_or_insert_with(|| started.elapsed());
        }
        // Shell-only history has no token report. Native model/rendering is
        // observed here; supplied context is separately checked over real MCP
        // stdio above, without inventing a zero-usage native measurement.
        if footer && screen.contains("footer-proof") && screen.contains(INHERITED_HISTORY_MARKER) {
            ensure!(
                !screen.contains("Status line failed") && !screen.contains("Status line timed out"),
                "native provider rejected footer: {screen}"
            );
            eprintln!(
                "native jump observation ({}; includes private tmux/CLI startup; {} ms polling): header={} ms, inherited-history={} ms, footer={} ms",
                label,
                POLL_INTERVAL.as_millis(),
                header_at
                    .context("native TUI header was never observed")?
                    .as_millis(),
                history_at
                    .context("inherited history was never observed")?
                    .as_millis(),
                started.elapsed().as_millis(),
            );
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "native footer did not render (offline={offline}): {screen}"
        );
        sleep(POLL_INTERVAL).await;
    }
    // The footer, including a missing coordinator endpoint, must leave the
    // native UI usable and allow a normal detach without a model request.
    close_tui(&server, "statusline").await?;
    let health = daemon_request(paths, "health", json!({})).await?;
    ensure!(health["status"] == "ok");
    Ok(())
}

async fn measure_warm_preparation(paths: &TestPaths, repository: &Path) -> Result<()> {
    // Separate, explicitly warm sample. It is not subtracted from a later
    // end-to-end TUI observation or treated as a large-history benchmark.
    let started = Instant::now();
    let attached = daemon_request(
        paths,
        "workspace.attach",
        json!({
            "scope":{"kind":"repository", "path":repository}, "workspace":FORK_WORKSPACE_NAME,
        }),
    )
    .await?;
    let elapsed = started.elapsed();
    crate::release_returned_attach(paths, &attached).await?;
    ensure!(attached["launch"]["kind"] == "resume");
    eprintln!(
        "native jump observation: warm workspace.attach={} ms",
        elapsed.as_millis()
    );
    Ok(())
}
