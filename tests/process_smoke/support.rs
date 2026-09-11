use super::*;

#[derive(Debug)]
pub(super) struct TestPaths {
    pub(super) home: PathBuf,
    pub(super) data_dir: PathBuf,
    pub(super) database: PathBuf,
    pub(super) socket: PathBuf,
    pub(super) endpoint: PathBuf,
    pub(super) token: PathBuf,
    pub(super) worktrees: PathBuf,
    pub(super) codex_home: PathBuf,
    pub(super) fake_codex: PathBuf,
    pub(super) codex_args: PathBuf,
    pub(super) jump_args: PathBuf,
    pub(super) hooks: PathBuf,
    pub(super) hook_handler: PathBuf,
    pub(super) hook_capture: PathBuf,
    pub(super) guard_handler: PathBuf,
    pub(super) guard_capture: PathBuf,
}

impl TestPaths {
    pub(super) fn new(root: &Path) -> Self {
        let runtime = root.join("runtime");
        let data_dir = root.join("data");
        Self {
            home: root.join("home"),
            database: data_dir.join("coco.db"),
            socket: runtime.join("cocod.sock"),
            endpoint: runtime.join("codex-app-server.json"),
            token: runtime.join("codex-app-server.token"),
            worktrees: root.join("worktrees"),
            codex_home: root.join("codex-home"),
            fake_codex: root.join("fake-codex"),
            codex_args: root.join("fake-codex.args"),
            jump_args: root.join("fake-jump.args"),
            hooks: root.join("hooks.json"),
            hook_handler: root.join("hook-handler"),
            hook_capture: root.join("hook-events.jsonl"),
            guard_handler: root.join("guard-handler"),
            guard_capture: root.join("guard-requests.jsonl"),
            data_dir,
        }
    }

    pub(super) fn apply(&self, command: &mut Command) {
        command
            .env("HOME", &self.home)
            .env("COCO_DATA_DIR", &self.data_dir)
            .env("COCO_DATABASE_PATH", &self.database)
            .env("COCO_SOCKET_PATH", &self.socket)
            .env("COCO_CODEX_ENDPOINT_PATH", &self.endpoint)
            .env("COCO_CODEX_TOKEN_PATH", &self.token)
            .env("COCO_WORKTREES_DIR", &self.worktrees)
            .env("CODEX_HOME", &self.codex_home)
            .env("COCO_CODEX_BINARY", &self.fake_codex)
            .env("COCO_WORKSPACE_EXECUTION", "shared")
            .env("COCO_TEST_CODEX_ARGS", &self.codex_args)
            .env("COCO_TEST_JUMP_ARGS", &self.jump_args)
            .env("COCO_HOOKS_PATH", &self.hooks)
            .env("RUST_LOG", "warn");
    }
}

pub(super) fn prepare_repository(repository: &Path) -> Result<()> {
    fs::create_dir_all(repository)?;
    run_git(repository, &["init", "."])?;
    fs::write(repository.join("README.md"), "# process smoke test\n")?;
    run_git(repository, &["add", "README.md"])?;
    run_git(
        repository,
        &[
            "-c",
            "user.name=CoCo Test",
            "-c",
            "user.email=coco@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "initial",
        ],
    )?;
    Ok(())
}

pub(super) fn prepare_codex_profile(paths: &TestPaths) -> Result<()> {
    fs::create_dir_all(&paths.codex_home)?;
    fs::write(
        paths.codex_home.join(format!("{PROFILE_NAME}.config.toml")),
        format!("model = \"{PROFILE_MODEL}\"\n"),
    )?;
    Ok(())
}

pub(super) fn run_git(repository: &Path, arguments: &[&str]) -> Result<()> {
    let output = std::process::Command::new("git")
        .args(arguments)
        .current_dir(repository)
        .output()
        .context("could not run Git for the process test")?;
    ensure!(
        output.status.success(),
        "git {} failed: {}",
        arguments.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

pub(super) fn write_fake_codex(path: &Path) -> Result<()> {
    fs::write(
        path,
        r#"#!/bin/sh
set -eu
kind=""
case "${1:-}" in
  app-server)
    : "${COCO_TEST_CODEX_ARGS:?}"
    destination="${COCO_TEST_CODEX_ARGS}"
    kind="app-server"
    ;;
  resume)
    : "${COCO_TEST_JUMP_ARGS:?}"
    destination="${COCO_TEST_JUMP_ARGS}"
    kind="resume"
    ;;
  --remote)
    : "${COCO_TEST_JUMP_ARGS:?}"
    destination="${COCO_TEST_JUMP_ARGS}"
    kind="fresh"
    ;;
  *)
    exit 64
    ;;
esac
arguments_tmp="${destination}.tmp"
printf '%s\n' "$@" > "$arguments_tmp"
mv "$arguments_tmp" "$destination"
if [ "$kind" = "app-server" ]; then
  exec sleep 3600
fi
if [ "$kind" = "resume" ]; then
  exit "${COCO_TEST_JUMP_EXIT:-0}"
fi
if [ "$kind" = "fresh" ]; then
  : "${COCO_TEST_FAKE_TUI_BINARY:?}"
  remote_endpoint="${2:?missing fresh --remote endpoint}"
  export COCO_TEST_FRESH_REMOTE="$remote_endpoint"
  exec "$COCO_TEST_FAKE_TUI_BINARY" --exact fresh_jump::fake_tui_process --nocapture
fi
"#,
    )?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

pub(super) fn read_arguments(path: &Path) -> Result<Vec<String>> {
    Ok(fs::read_to_string(path)?
        .lines()
        .map(ToOwned::to_owned)
        .collect())
}

pub(super) fn verify_app_server_arguments(
    arguments: &[String],
    token_path: &Path,
) -> Result<String> {
    ensure!(
        arguments.len() == 7,
        "unexpected fake Codex arguments: {arguments:?}"
    );
    ensure!(arguments[0] == "app-server", "missing app-server command");
    ensure!(arguments[1] == "--listen", "missing --listen option");
    ensure!(arguments[3] == "--ws-auth", "missing --ws-auth option");
    ensure!(arguments[4] == "capability-token", "unexpected auth mode");
    ensure!(
        arguments[5] == "--ws-token-file",
        "missing token file option"
    );
    ensure!(
        Path::new(&arguments[6]) == token_path,
        "unexpected token path"
    );
    Ok(arguments[2].clone())
}

pub(super) fn verify_jump_arguments(
    arguments: &[String],
    endpoint: &str,
    worktree: &Path,
) -> Result<()> {
    let expected = [
        "resume".to_owned(),
        THREAD_ID.to_owned(),
        "--remote".to_owned(),
        endpoint.to_owned(),
        "--remote-auth-token-env".to_owned(),
        "COCO_CODEX_REMOTE_CAPABILITY_TOKEN".to_owned(),
        "-C".to_owned(),
        worktree.to_string_lossy().into_owned(),
    ];
    ensure!(
        arguments == expected,
        "unexpected fake jump arguments: {arguments:?}"
    );
    Ok(())
}

pub(super) fn spawn_daemon(paths: &TestPaths, log: &Path) -> Result<Child> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cocod"));
    paths.apply(&mut command);
    command
        .stdout(Stdio::null())
        .stderr(Stdio::from(fs::File::create(log)?))
        .kill_on_drop(true)
        .spawn()
        .context("could not start cocod")
}

pub(super) async fn run_cli(
    paths: &TestPaths,
    repository: &Path,
    arguments: &[&str],
) -> Result<Output> {
    let output = capture_cli(paths, repository, arguments, None).await?;
    ensure!(
        output.status.success(),
        "coco {} failed:\nstdout: {}\nstderr: {}",
        arguments.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output)
}

pub(super) async fn run_cli_with_jump_exit(
    paths: &TestPaths,
    repository: &Path,
    arguments: &[&str],
    exit_code: u8,
) -> Result<Output> {
    let output = capture_cli(paths, repository, arguments, Some(exit_code)).await?;
    ensure!(
        !output.status.success(),
        "coco {} unexpectedly succeeded",
        arguments.join(" ")
    );
    Ok(output)
}

pub(super) async fn run_cli_with_fresh_tui(
    paths: &TestPaths,
    repository: &Path,
    arguments: &[&str],
    mode: &str,
) -> Result<Output> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_coco"));
    paths.apply(&mut command);
    command
        .args(arguments)
        .current_dir(repository)
        .env("COCO_TEST_FAKE_TUI_BINARY", std::env::current_exe()?)
        .env("COCO_TEST_FRESH_TUI_MODE", mode)
        .kill_on_drop(true);
    let output = timeout(PROCESS_TIMEOUT, command.output())
        .await
        .with_context(|| format!("coco {} timed out", arguments.join(" ")))??;
    ensure!(
        output.status.success(),
        "coco {} failed:\nstdout: {}\nstderr: {}",
        arguments.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output)
}

pub(super) async fn run_cli_until_interrupt(
    paths: &TestPaths,
    repository: &Path,
    arguments: &[&str],
) -> Result<Output> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_coco"));
    paths.apply(&mut command);
    command
        .args(arguments)
        .current_dir(repository)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().context("could not start coco follower")?;
    sleep(Duration::from_millis(750)).await;
    ensure!(
        child.try_wait()?.is_none(),
        "coco {} stopped following before it was interrupted",
        arguments.join(" ")
    );
    interrupt(&child).await?;
    let output = timeout(PROCESS_TIMEOUT, child.wait_with_output())
        .await
        .with_context(|| format!("coco {} did not detach", arguments.join(" ")))??;
    ensure!(
        output.status.success(),
        "coco {} failed:\nstdout: {}\nstderr: {}",
        arguments.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output)
}

pub(super) async fn capture_cli(
    paths: &TestPaths,
    repository: &Path,
    arguments: &[&str],
    jump_exit: Option<u8>,
) -> Result<Output> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_coco"));
    paths.apply(&mut command);
    command
        .args(arguments)
        .current_dir(repository)
        .kill_on_drop(true);
    if let Some(exit_code) = jump_exit {
        command.env("COCO_TEST_JUMP_EXIT", exit_code.to_string());
    }
    let output = timeout(PROCESS_TIMEOUT, command.output())
        .await
        .with_context(|| format!("coco {} timed out", arguments.join(" ")))??;
    Ok(output)
}

pub(super) fn cli_json(output: &Output) -> Result<Value> {
    serde_json::from_slice(&output.stdout).context("coco did not emit valid JSON")
}

pub(super) async fn workspace_status(paths: &TestPaths, repository: &Path) -> Result<Value> {
    named_workspace_status(paths, repository, WORKSPACE_NAME).await
}

pub(super) async fn named_workspace_status(
    paths: &TestPaths,
    repository: &Path,
    workspace: &str,
) -> Result<Value> {
    cli_json(&run_cli(paths, repository, &["status", workspace, "--json"]).await?)
}

pub(super) async fn wait_for_workspace_phase(
    paths: &TestPaths,
    repository: &Path,
    expected: &str,
) -> Result<Value> {
    wait_for_named_workspace_phase(paths, repository, WORKSPACE_NAME, expected).await
}

pub(super) async fn wait_for_named_workspace_phase(
    paths: &TestPaths,
    repository: &Path,
    workspace: &str,
    expected: &str,
) -> Result<Value> {
    let deadline = Instant::now() + PROCESS_TIMEOUT;
    loop {
        let status = named_workspace_status(paths, repository, workspace).await?;
        if status.pointer("/workspace/phase").and_then(Value::as_str) == Some(expected) {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            bail!("workspace did not reach phase {expected:?}: {status}");
        }
        sleep(POLL_INTERVAL).await;
    }
}

pub(super) async fn wait_for_pending_decision(
    paths: &TestPaths,
    repository: &Path,
) -> Result<Value> {
    let deadline = Instant::now() + PROCESS_TIMEOUT;
    loop {
        let status = workspace_status(paths, repository).await?;
        if status
            .get("openDecisions")
            .and_then(Value::as_array)
            .is_some_and(|decisions| {
                decisions
                    .iter()
                    .any(|decision| decision.get("state") == Some(&json!("pending")))
            })
            && status.pointer("/workspace/phase").and_then(Value::as_str)
                == Some("waiting_for_approval")
        {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            bail!("workspace did not expose a pending decision: {status}");
        }
        sleep(POLL_INTERVAL).await;
    }
}

pub(super) async fn wait_for_file(path: &Path, daemon: &mut Child, log: &Path) -> Result<()> {
    let deadline = Instant::now() + PROCESS_TIMEOUT;
    loop {
        if path.exists() {
            return Ok(());
        }
        if let Some(status) = daemon.try_wait()? {
            bail!("cocod exited with {status}: {}", read_log(log));
        }
        if Instant::now() >= deadline {
            bail!(
                "timed out waiting for {}: {}",
                path.display(),
                read_log(log)
            );
        }
        sleep(POLL_INTERVAL).await;
    }
}

pub(super) async fn interrupt(child: &Child) -> Result<()> {
    let pid = child.id().context("cocod had no process id")?;
    let output = Command::new("kill")
        .args(["-INT", &pid.to_string()])
        .output()
        .await
        .context("could not send SIGINT to cocod")?;
    ensure!(
        output.status.success(),
        "could not interrupt cocod: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

pub(super) fn verify_codex_requests(requests: &[Value], worktree: &Path) -> Result<()> {
    let initialize = request(requests, "initialize")?;
    assert_eq!(
        initialize.pointer("/params/clientInfo/name"),
        Some(&json!("coco"))
    );
    assert_eq!(
        initialize.pointer("/params/capabilities/experimentalApi"),
        Some(&json!(true)),
        "CoCo uses an experimental thread/fork field without negotiating its capability"
    );

    let model_requests = requests
        .iter()
        .filter(|request| request.get("method") == Some(&json!("model/list")))
        .collect::<Vec<_>>();
    assert_eq!(model_requests.len(), 4);
    for request in &model_requests {
        assert_eq!(request.pointer("/params/limit"), Some(&json!(100)));
        assert_eq!(
            request.pointer("/params/includeHidden"),
            Some(&json!(false))
        );
    }
    assert_eq!(
        model_requests
            .iter()
            .filter(|request| request.pointer("/params/cursor").is_none())
            .count(),
        2
    );
    assert_eq!(
        model_requests
            .iter()
            .filter(|request| {
                request.pointer("/params/cursor") == Some(&json!("models-page-2"))
            })
            .count(),
        2
    );

    let thread_start = request(requests, "thread/start")?;
    let worktree_value = Value::String(worktree.to_string_lossy().into_owned());
    assert_eq!(thread_start.pointer("/params/cwd"), Some(&worktree_value));
    ensure!(
        thread_start
            .pointer("/params/runtimeWorkspaceRoots")
            .is_none(),
        "thread/start used an experimental field without negotiating the capability"
    );
    assert_eq!(
        thread_start.pointer("/params/config"),
        Some(&json!({"model": PROFILE_MODEL}))
    );
    assert_eq!(
        thread_start.pointer("/params/model"),
        Some(&json!(MODEL_OVERRIDE))
    );
    assert_eq!(
        thread_start.pointer("/params/ephemeral"),
        Some(&json!(false))
    );

    let thread_name = request(requests, "thread/name/set")?;
    assert_eq!(
        thread_name.pointer("/params/threadId"),
        Some(&json!(THREAD_ID))
    );
    assert_eq!(
        thread_name.pointer("/params/name"),
        Some(&json!(WORKSPACE_NAME))
    );

    let turn_start = request(requests, "turn/start")?;
    assert_eq!(
        turn_start.pointer("/params/threadId"),
        Some(&json!(THREAD_ID))
    );
    assert_eq!(turn_start.pointer("/params/cwd"), Some(&worktree_value));
    assert_eq!(
        turn_start.pointer("/params/input/0/text"),
        Some(&json!("Complete the process smoke test"))
    );
    ensure!(
        turn_start
            .pointer("/params/clientUserMessageId")
            .and_then(Value::as_str)
            .is_some_and(|value| value.starts_with("coco-")),
        "turn/start had no CoCo message id"
    );
    let usage_reads = requests
        .iter()
        .filter(|request| request.get("method") == Some(&json!("account/usage/read")))
        .collect::<Vec<_>>();
    assert_eq!(usage_reads.len(), 1, "native cost reads were not cached");
    assert_eq!(
        usage_reads[0].pointer("/params/threadId"),
        Some(&json!(THREAD_ID))
    );
    verify_bound_thread_reads(requests)
}

fn verify_bound_thread_reads(requests: &[Value]) -> Result<()> {
    let thread_reads = requests
        .iter()
        .filter(|request| request.get("method") == Some(&json!("thread/read")))
        .collect::<Vec<_>>();
    ensure!(
        !thread_reads.is_empty(),
        "workspace reads did not consult native thread state"
    );
    for read in thread_reads {
        ensure!(
            matches!(
                read.pointer("/params/threadId").and_then(Value::as_str),
                Some(THREAD_ID | super::multi_client::SECOND_THREAD)
            ),
            "unexpected workspace was read"
        );
    }
    assert_eq!(
        requests
            .iter()
            .filter(|frame| frame["method"] == "thread/start")
            .count(),
        2
    );
    assert_eq!(
        requests
            .iter()
            .filter(|frame| frame["method"] == "turn/start")
            .count(),
        4
    );
    Ok(())
}

pub(super) fn verify_fork_requests(requests: &[Value], child_worktree: &Path) -> Result<()> {
    assert_eq!(
        request(requests, "initialize")?.pointer("/params/capabilities/experimentalApi"),
        Some(&json!(true)),
        "context forks require the negotiated experimental API capability"
    );
    let methods = requests
        .iter()
        .filter_map(|frame| frame.get("method").and_then(Value::as_str))
        .filter(|method| *method != "thread/read")
        .collect::<Vec<_>>();
    assert_eq!(
        methods,
        [
            "initialize",
            "initialized",
            "thread/start",
            "thread/name/set",
            "turn/start",
            "thread/fork",
            "thread/name/set",
            "thread/compact/start",
            "turn/start",
        ]
    );
    let source_start = request(requests, "thread/start")?;
    ensure!(
        source_start.pointer("/params/model").is_none(),
        "thread/start invented an explicit model when none was requested"
    );
    let fork = request(requests, "thread/fork")?;
    assert_eq!(
        fork.pointer("/params/threadId"),
        Some(&json!(FORK_SOURCE_THREAD_ID))
    );
    assert_eq!(
        fork.pointer("/params/cwd"),
        Some(&json!(child_worktree.to_string_lossy()))
    );
    assert_eq!(fork.pointer("/params/config"), Some(&json!({})));
    assert_eq!(fork.pointer("/params/model"), Some(&json!(MODEL_OVERRIDE)));
    assert_eq!(fork.pointer("/params/ephemeral"), Some(&json!(false)));
    assert_eq!(fork.pointer("/params/excludeTurns"), Some(&json!(true)));
    assert_eq!(
        fork.pointer("/params/deferGoalContinuation"),
        Some(&json!(true))
    );
    assert_eq!(
        request(requests, "thread/compact/start")?.pointer("/params/threadId"),
        Some(&json!(FORK_CHILD_THREAD_ID))
    );
    let turn = requests
        .iter()
        .find(|request| {
            request.get("method") == Some(&json!("turn/start"))
                && request.pointer("/params/threadId") == Some(&json!(FORK_CHILD_THREAD_ID))
        })
        .context("fake App Server did not receive the child turn/start")?;
    assert_eq!(
        turn.pointer("/params/threadId"),
        Some(&json!(FORK_CHILD_THREAD_ID))
    );
    assert_eq!(
        turn.pointer("/params/cwd"),
        Some(&json!(child_worktree.to_string_lossy()))
    );
    assert_eq!(
        turn.pointer("/params/input/0/text"),
        Some(&json!(FORK_CHILD_MESSAGE))
    );
    let binding = turn
        .pointer("/params/additionalContext/coco.workspace-binding/value")
        .and_then(Value::as_str)
        .context("forked turn had no CoCo workspace binding")?;
    let binding: Value = serde_json::from_str(binding)?;
    assert_eq!(binding["worktreePath"], json!(child_worktree));
    assert_eq!(binding["sourceWorkspaceName"], FORK_SOURCE_WORKSPACE);
    ensure!(
        requests.iter().any(|request| {
            request.get("method") == Some(&json!("thread/read"))
                && request.pointer("/params/threadId") == Some(&json!(FORK_CHILD_THREAD_ID))
        }),
        "fork status did not consult the bound native child thread"
    );
    Ok(())
}

pub(super) fn verify_recovery_requests(requests: &[Value], worktree: &Path) -> Result<()> {
    assert_eq!(
        request(requests, "initialize")?.pointer("/params/capabilities/experimentalApi"),
        Some(&json!(true))
    );
    let methods = requests
        .iter()
        .filter_map(|request| request.get("method").and_then(Value::as_str))
        .filter(|method| *method != "thread/read")
        .collect::<Vec<_>>();
    assert_eq!(
        methods,
        [
            "initialize",
            "initialized",
            "account/usage/read",
            "thread/resume"
        ]
    );
    let resume = request(requests, "thread/resume")?;
    assert_eq!(resume.pointer("/params/threadId"), Some(&json!(THREAD_ID)));
    assert_eq!(
        resume.pointer("/params/cwd"),
        Some(&json!(worktree.to_string_lossy()))
    );
    assert_eq!(
        resume.pointer("/params/config"),
        Some(&json!({"model": PROFILE_MODEL}))
    );
    assert_eq!(
        resume.pointer("/params/model"),
        Some(&json!(MODEL_OVERRIDE))
    );
    assert_eq!(resume.pointer("/params/excludeTurns"), Some(&json!(true)));
    ensure!(
        request(requests, "thread/start").is_err(),
        "recovery created a replacement thread"
    );
    ensure!(
        requests.iter().any(|read| read["method"] == "thread/read"
            && read.pointer("/params/threadId") == Some(&json!(THREAD_ID))),
        "main workspace was not read after restart"
    );
    let resume_index = requests
        .iter()
        .position(|request| request.get("method") == Some(&json!("thread/resume")))
        .context("on-demand attach never resumed the unloaded thread")?;
    let reads_before_resume = requests[..resume_index]
        .iter()
        .filter(|request| request.get("method") == Some(&json!("thread/read")))
        .count();
    ensure!(
        reads_before_resume >= 2,
        "thread/resume happened before passive status and attach validated native state"
    );
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.get("method") == Some(&json!("thread/resume")))
            .count(),
        1
    );
    Ok(())
}

pub(super) fn verify_remote_tui_requests(requests: &[Value]) -> Result<()> {
    let methods = requests
        .iter()
        .map(|request| {
            request
                .get("method")
                .and_then(Value::as_str)
                .context("remote TUI request had no method")
        })
        .collect::<Result<Vec<_>>>()?;
    assert_eq!(
        methods,
        [
            "initialize",
            "initialized",
            "thread/resume",
            "thread/unsubscribe",
            "initialize",
            "initialized",
            "thread/resume",
        ]
    );
    for request in requests
        .iter()
        .filter(|request| request.get("method") == Some(&json!("thread/resume")))
    {
        assert_eq!(request.pointer("/params/threadId"), Some(&json!(THREAD_ID)));
    }
    ensure!(
        !methods.contains(&"turn/interrupt"),
        "leaving a remote TUI unexpectedly interrupted the turn"
    );
    Ok(())
}

pub(super) fn request<'a>(requests: &'a [Value], method: &str) -> Result<&'a Value> {
    requests
        .iter()
        .find(|request| request.get("method").and_then(Value::as_str) == Some(method))
        .with_context(|| format!("fake App Server did not receive {method}"))
}

pub(super) fn assert_mode(path: &Path, expected: u32) -> Result<()> {
    let actual = fs::metadata(path)?.permissions().mode() & 0o777;
    ensure!(
        actual == expected,
        "{} had mode {actual:o}, expected {expected:o}",
        path.display()
    );
    Ok(())
}

pub(super) fn read_log(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| format!("could not read daemon log: {error}"))
}
