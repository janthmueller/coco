use super::*;
use std::net::SocketAddr;
use tokio::net::TcpListener;

#[path = "scripted.rs"]
mod scripted;

const MESSAGE: &str = "Saved provider snapshot proof";
const WORKSPACE: &str = "saved-provider";

struct Fixture {
    _temporary: tempfile::TempDir,
    paths: TestPaths,
    binary: PathBuf,
    repository: PathBuf,
}

impl Fixture {
    async fn new() -> Result<Self> {
        let binary = env::var_os(CODEX_BINARY_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(|| "codex".into());
        let temporary = tempfile::tempdir()?;
        let paths = TestPaths::new(temporary.path());
        for directory in [&paths.home, &paths.codex_home, &paths.data_dir] {
            fs::create_dir_all(directory)?;
        }
        verify_codex_version(&binary, &paths).await?;
        let repository = temporary.path().join("repository");
        prepare_repository(&repository)?;
        Ok(Self {
            _temporary: temporary,
            paths,
            binary,
            repository,
        })
    }
}

pub(super) async fn native_saved_provider_survives_resume() -> Result<()> {
    let fixture = Fixture::new().await?;
    let Fixture {
        paths,
        binary,
        repository,
        ..
    } = &fixture;
    let forbidden_default = TcpListener::bind("127.0.0.1:0").await?;
    let mut provider = scripted::Provider::start().await?;
    install(paths, forbidden_default.local_addr()?, provider.address)?;
    let mut server = RealAppServer::spawn(paths, binary, repository, "provider-initial").await?;
    let started = server
        .request(
            "thread/start",
            json!({
                "cwd": repository, "model": "provider-proof", "ephemeral": false,
                "config": {"model_provider": "provider-other"},
            }),
        )
        .await?;
    ensure!(started["modelProvider"] == "provider-other");
    let thread = started["thread"]["id"]
        .as_str()
        .context("thread ID missing")?;
    server
        .request(
            "thread/shellCommand",
            json!({
                "threadId": thread, "command": "printf coco-provider-contract", "timeoutMs": 10000,
            }),
        )
        .await?;
    server
        .wait_for_thread_notification("turn/completed", thread)
        .await?;
    server.stop().await?;

    // The new process defaults to provider-current; only persisted native
    // metadata can restore provider-other. No provider override is passed on
    // resume/input; shell-only history does not preserve an inference model.
    let mut server = RealAppServer::spawn(paths, binary, repository, "provider-resumed").await?;
    let resumed = server
        .request(
            "thread/resume",
            json!({
                "threadId": thread, "excludeTurns": true, "model": "provider-proof",
            }),
        )
        .await?;
    ensure!(
        resumed["modelProvider"] == "provider-other",
        "saved provider lost: {resumed}"
    );
    let sent = server
        .request(
            "turn/start",
            json!({
                "threadId": thread, "input": [{"type": "text", "text": MESSAGE}],
            }),
        )
        .await?;
    let completed = provider
        .watch(server.wait_for_thread_notification("turn/completed", thread))
        .await?;
    ensure!(completed["params"]["turn"]["id"] == sent["turn"]["id"]);
    ensure!(
        completed["params"]["turn"]["status"] == "completed",
        "saved provider turn failed: {completed}"
    );
    server.stop().await?;
    provider.finish(1).await?;
    ensure!(
        timeout(Duration::from_millis(20), forbidden_default.accept())
            .await
            .is_err(),
        "default provider contacted"
    );
    Ok(())
}

pub(super) async fn coco_saved_provider_survives_restart() -> Result<()> {
    let fixture = Fixture::new().await?;
    let Fixture {
        paths,
        binary,
        repository,
        ..
    } = &fixture;
    let forbidden_default = TcpListener::bind("127.0.0.1:0").await?;
    let mut provider = scripted::Provider::start().await?;
    install(paths, forbidden_default.local_addr()?, provider.address)?;
    fs::write(
        paths.codex_home.join("other.config.toml"),
        "model_provider='provider-other'\n",
    )?;
    let log = paths.data_dir.join("saved-provider.log");
    let mut daemon = spawn_daemon(paths, binary, &log)?;
    wait_for_file(&paths.socket, &mut daemon, &log).await?;
    run_cli(
        paths,
        binary,
        repository,
        &[
            "create",
            WORKSPACE,
            "--profile",
            "other",
            "--model",
            "provider-proof",
        ],
    )
    .await?;
    let first = provider
        .watch(send(paths, binary, repository, "provider-first"))
        .await?;
    stop_daemon(&mut daemon, &log).await?;
    let mut daemon = spawn_daemon(paths, binary, &log)?;
    wait_for_file(&paths.socket, &mut daemon, &log).await?;
    let second = provider
        .watch(send(paths, binary, repository, "provider-second"))
        .await?;
    ensure!(first["codexThreadId"] == second["codexThreadId"]);
    ensure!(first["codexTurnId"] != second["codexTurnId"]);
    // The previous operation still resolves to its exact completed turn after
    // restart; changing provider behavior must not change send idempotency.
    let replay = send(paths, binary, repository, "provider-first").await?;
    ensure!(
        first == replay,
        "provider-preserving resume lost operation identity"
    );
    stop_daemon(&mut daemon, &log).await?;
    provider.finish(2).await?;
    ensure!(
        timeout(Duration::from_millis(20), forbidden_default.accept())
            .await
            .is_err(),
        "default provider contacted"
    );
    verify_runtime_cleanup(paths)
}

async fn send(
    paths: &TestPaths,
    binary: &Path,
    repository: &Path,
    operation: &str,
) -> Result<Value> {
    run_cli(
        paths,
        binary,
        repository,
        &["send", WORKSPACE, MESSAGE, "--operation-id", operation],
    )
    .await?;
    timeout(COMPATIBILITY_TIMEOUT, async {
        loop {
            let turn = daemon_request(paths, "turn.get", json!({
                "scope": {"kind":"repository", "path":repository}, "workspace": WORKSPACE, "operationId": operation,
            })).await?;
            if turn["state"] == "finished" {
                ensure!(turn["outcome"] == "completed", "scripted provider turn failed: {turn}");
                return Ok(turn);
            }
            sleep(POLL_INTERVAL).await;
        }
    }).await?
}

fn install(paths: &TestPaths, default: SocketAddr, saved: SocketAddr) -> Result<()> {
    let mut config = "model_provider='provider-current'\n".to_owned();
    for (name, address) in [("provider-current", default), ("provider-other", saved)] {
        config.push_str(&format!(
            "[model_providers.{name}]\nname='{name}'\nbase_url='http://{address}/v1'\n\
             wire_api='responses'\nrequires_openai_auth=false\nsupports_websockets=false\n"
        ));
    }
    fs::write(paths.codex_home.join("config.toml"), config)?;
    Ok(())
}
