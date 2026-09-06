use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::time::timeout;

use crate::codex::{CodexClient, CodexClientOptions, CodexEvent};

const OPT_IN_ENV: &str = "COCO_RUN_REAL_GIT_APPROVAL";
const CODEX_BINARY_ENV: &str = "COCO_REAL_CODEX_BINARY";
const SUPPORTED_CODEX_VERSION: &str = "codex-cli 0.147.0";
const LIVE_TIMEOUT: Duration = Duration::from_secs(180);
const APPROVAL_POLICY: &str = "untrusted";
const PROOF_BRANCH: &str = "coco/approval-proof";
const PROOF_FILE: &str = "COCO_APPROVAL_PROOF.txt";
const PROOF_CONTENT: &str = "coco native approval proof\n";
const PROOF_COMMIT_SUBJECT: &str = "coco native approval proof";
const EXPECTED_COMMAND: &str = "printf 'coco native approval proof\\n' > COCO_APPROVAL_PROOF.txt && git add COCO_APPROVAL_PROOF.txt && git -c user.name='CoCo Approval Proof' -c user.email='coco-approval@example.invalid' -c commit.gpgsign=false commit -m 'coco native approval proof'";

#[derive(Debug)]
struct GitFixture {
    repository: PathBuf,
    worktree: PathBuf,
    base_sha: String,
    bash_path: String,
}

#[derive(Debug)]
struct ApprovalTrace {
    request_id: Value,
    item_id: String,
    turn_id: String,
    resolved: bool,
    item_completed: bool,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "consumes a live model turn and accepts one exact, allowlisted Git command"]
async fn pinned_codex_approves_a_commit_only_on_the_linked_workspace_branch() -> Result<()> {
    require_explicit_opt_in()?;
    let codex_binary = env::var_os(CODEX_BINARY_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("codex"));
    verify_codex_version(&codex_binary)?;

    let temporary = tempfile::tempdir()?;
    let fixture = prepare_linked_worktree(temporary.path())?;
    let (client, mut events) = CodexClient::spawn(CodexClientOptions {
        codex_binary,
        codex_home: None,
        event_buffer: 1024,
        ..CodexClientOptions::default()
    })
    .await
    .context("could not start the pinned Codex App Server")?;

    let proof = run_live_approval_proof(&client, &mut events, &fixture).await;
    let close = client.close().await;
    proof?;
    close.context("could not close the pinned Codex App Server")?;
    verify_git_result(&fixture)
}

fn require_explicit_opt_in() -> Result<()> {
    ensure!(
        env::var(OPT_IN_ENV).as_deref() == Ok("1"),
        "set {OPT_IN_ENV}=1 in addition to passing --ignored"
    );
    Ok(())
}

fn verify_codex_version(codex_binary: &Path) -> Result<()> {
    let output = Command::new(codex_binary)
        .arg("--version")
        .output()
        .context("could not read the Codex version")?;
    ensure!(
        output.status.success(),
        "could not read the Codex version: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual = String::from_utf8(output.stdout)?.trim().to_owned();
    ensure!(
        actual == SUPPORTED_CODEX_VERSION,
        "unsupported Codex executable: expected {SUPPORTED_CODEX_VERSION:?}, received {actual:?}"
    );
    Ok(())
}

fn prepare_linked_worktree(root: &Path) -> Result<GitFixture> {
    let repository = root.join("repository");
    let worktree = root.join("workspace");
    fs::create_dir_all(&repository)?;
    run_git(&repository, &["init", "--initial-branch=main", "."])?;
    fs::write(repository.join("README.md"), "# native approval proof\n")?;
    run_git(&repository, &["add", "README.md"])?;
    run_git(
        &repository,
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
    let base_sha = git_stdout(&repository, &["rev-parse", "HEAD"])?;
    let bash_path = bash_path()?;
    run_git(
        &repository,
        &[
            "worktree",
            "add",
            "-b",
            PROOF_BRANCH,
            path_text(&worktree)?,
            &base_sha,
        ],
    )?;
    verify_shared_git_directory(&repository, &worktree)?;
    Ok(GitFixture {
        repository,
        worktree,
        base_sha,
        bash_path,
    })
}

fn verify_shared_git_directory(repository: &Path, worktree: &Path) -> Result<()> {
    let repository_common = fs::canonicalize(repository.join(".git"))?;
    let worktree_common = git_path(worktree, &["rev-parse", "--git-common-dir"])?;
    ensure!(
        repository_common == worktree_common,
        "fixture is not a native linked worktree: {} != {}",
        repository_common.display(),
        worktree_common.display()
    );
    Ok(())
}

async fn run_live_approval_proof(
    client: &CodexClient,
    events: &mut mpsc::Receiver<CodexEvent>,
    fixture: &GitFixture,
) -> Result<()> {
    let thread = client
        .request(
            "thread/start",
            json!({
                "cwd": fixture.worktree,
                "ephemeral": true,
                "approvalPolicy": APPROVAL_POLICY,
                "approvalsReviewer": "user",
                "sandbox": "workspace-write",
            }),
        )
        .await?;
    let thread_id = required_string(&thread, "/thread/id")?;
    let turn = client
        .request(
            "turn/start",
            json!({
                "threadId": thread_id,
                "cwd": fixture.worktree,
                "approvalPolicy": APPROVAL_POLICY,
                "approvalsReviewer": "user",
                "sandboxPolicy": {
                    "type": "workspaceWrite",
                    "networkAccess": false,
                    "writableRoots": [],
                },
                "input": [{
                    "type": "text",
                    "text": proof_prompt(),
                }],
            }),
        )
        .await
        .context("could not start the live approval-proof turn")?;
    let turn_id = required_string(&turn, "/turn/id")?;
    observe_approval_and_completion(client, events, fixture, &thread_id, &turn_id).await
}

fn proof_prompt() -> String {
    format!(
        "Run exactly the following command as one shell invocation, copied verbatim. Do not use any other command or file-editing tool, do not ask a question, and do not stop before attempting it:\n\n{EXPECTED_COMMAND}"
    )
}

async fn observe_approval_and_completion(
    client: &CodexClient,
    events: &mut mpsc::Receiver<CodexEvent>,
    fixture: &GitFixture,
    thread_id: &str,
    turn_id: &str,
) -> Result<()> {
    let deadline = Instant::now() + LIVE_TIMEOUT;
    let mut trace: Option<ApprovalTrace> = None;
    let mut turn_completed = false;
    let mut observed = Vec::new();

    while Instant::now() < deadline && !proof_is_complete(trace.as_ref(), turn_completed) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let event = match timeout(remaining, events.recv()).await {
            Ok(Some(event)) => event,
            Ok(None) => bail!("Codex closed its event stream during the native approval flow"),
            Err(_) => break,
        };
        match event {
            CodexEvent::ServerRequest { id, method, params } => {
                record_observation(&mut observed, format!("request:{method}"));
                handle_server_request(
                    client, fixture, thread_id, turn_id, &mut trace, id, &method, params,
                )
                .await?;
            }
            CodexEvent::Notification { method, params } => {
                record_notification(&mut observed, &method, &params);
                match method.as_str() {
                    "serverRequest/resolved" => mark_resolved(trace.as_mut(), &params),
                    "item/completed" => mark_item_completed(trace.as_mut(), &params),
                    "turn/completed"
                        if params.pointer("/turn/id").and_then(Value::as_str) == Some(turn_id) =>
                    {
                        ensure!(
                            params.pointer("/turn/status").and_then(Value::as_str)
                                == Some("completed"),
                            "approval-proof turn did not complete successfully: {params}"
                        );
                        turn_completed = true;
                    }
                    "error"
                        if !params
                            .get("willRetry")
                            .and_then(Value::as_bool)
                            .unwrap_or(false) =>
                    {
                        bail!("Codex reported an error during the approval proof: {params}")
                    }
                    _ => {}
                }
            }
        }
        if turn_completed {
            break;
        }
    }

    let git = git_diagnostics(fixture);
    let trace = trace.with_context(|| {
        format!("Codex never requested command approval; observed={observed:?}; git={git}")
    })?;
    ensure!(trace.resolved, "Codex did not emit serverRequest/resolved");
    ensure!(
        trace.item_completed,
        "Codex did not complete the approved command item"
    );
    ensure!(
        turn_completed,
        "Codex did not complete the approval-proof turn"
    );
    Ok(())
}

fn record_observation(observed: &mut Vec<String>, value: String) {
    const MAX_OBSERVATIONS: usize = 128;
    if observed.len() < MAX_OBSERVATIONS {
        observed.push(value);
    }
}

fn record_notification(observed: &mut Vec<String>, method: &str, params: &Value) {
    if matches!(
        method,
        "item/agentMessage/delta"
            | "mcpServer/startupStatus/updated"
            | "thread/tokenUsage/updated"
            | "account/rateLimits/updated"
    ) {
        return;
    }
    let detail = match method {
        "thread/status/changed" => params
            .get("status")
            .map_or_else(|| method.to_owned(), |status| format!("{method}:{status}")),
        "item/completed" | "item/started" => params
            .pointer("/item/type")
            .and_then(Value::as_str)
            .map_or_else(|| method.to_owned(), |kind| format!("{method}:{kind}")),
        "turn/completed" => params
            .pointer("/turn/status")
            .and_then(Value::as_str)
            .map_or_else(|| method.to_owned(), |status| format!("{method}:{status}")),
        _ => method.to_owned(),
    };
    record_observation(observed, detail);
}

fn git_diagnostics(fixture: &GitFixture) -> String {
    let head = git_stdout(&fixture.worktree, &["rev-parse", "HEAD"])
        .unwrap_or_else(|error| format!("error:{error}"));
    let status = git_stdout(&fixture.worktree, &["status", "--porcelain"])
        .unwrap_or_else(|error| format!("error:{error}"));
    format!("head={head}, base={}, status={status:?}", fixture.base_sha)
}

#[expect(
    clippy::too_many_arguments,
    reason = "the live protocol assertion keeps every correlation value explicit"
)]
async fn handle_server_request(
    client: &CodexClient,
    fixture: &GitFixture,
    thread_id: &str,
    turn_id: &str,
    trace: &mut Option<ApprovalTrace>,
    id: Value,
    method: &str,
    params: Value,
) -> Result<()> {
    ensure!(
        method == "item/commandExecution/requestApproval",
        "unexpected live Codex server request {method}: {params}"
    );
    ensure!(
        trace.is_none(),
        "Codex requested command approval more than once"
    );
    if let Err(error) = validate_approval_request(fixture, thread_id, turn_id, &params) {
        let _ = client.respond(id, json!({"decision": "cancel"})).await;
        return Err(error);
    }
    let item_id = required_string(&params, "/itemId")?;
    client
        .respond(id.clone(), json!({"decision": "accept"}))
        .await
        .context("could not accept the exact allowlisted Git command")?;
    *trace = Some(ApprovalTrace {
        request_id: id,
        item_id,
        turn_id: turn_id.to_owned(),
        resolved: false,
        item_completed: false,
    });
    Ok(())
}

fn validate_approval_request(
    fixture: &GitFixture,
    thread_id: &str,
    turn_id: &str,
    params: &Value,
) -> Result<()> {
    ensure!(
        params.get("threadId").and_then(Value::as_str) == Some(thread_id),
        "approval request used a different thread: {params}"
    );
    ensure!(
        params.get("turnId").and_then(Value::as_str) == Some(turn_id),
        "approval request used a different turn: {params}"
    );
    ensure!(
        params.get("cwd").and_then(Value::as_str) == fixture.worktree.to_str(),
        "approval request used a different cwd: {params}"
    );
    validate_allowlisted_command(fixture, params)?;
    ensure!(
        params
            .get("availableDecisions")
            .and_then(Value::as_array)
            .is_none_or(|decisions| decisions.iter().any(|decision| decision == "accept")),
        "Codex did not offer the accept decision: {params}"
    );
    Ok(())
}

fn validate_allowlisted_command(fixture: &GitFixture, params: &Value) -> Result<()> {
    let expected_argv = json!([fixture.bash_path, "-lc", EXPECTED_COMMAND]);
    ensure!(
        params.get("proposedExecpolicyAmendment") == Some(&expected_argv),
        "refusing an approval whose executable argv differs from the allowlist: {params}"
    );
    let expected_display = format!(
        "{} -lc {}",
        fixture.bash_path,
        serde_json::to_string(EXPECTED_COMMAND)?
    );
    ensure!(
        params.get("command").and_then(Value::as_str) == Some(expected_display.as_str()),
        "refusing an approval whose displayed command differs from its allowlisted argv: {params}"
    );
    let actions = params
        .get("commandActions")
        .and_then(Value::as_array)
        .context("approval request did not expose commandActions")?;
    ensure!(
        actions.len() == 1
            && actions[0].get("command").and_then(Value::as_str) == Some(EXPECTED_COMMAND),
        "refusing an approval whose parsed command action differs from the allowlist: {params}"
    );
    Ok(())
}

fn mark_resolved(trace: Option<&mut ApprovalTrace>, params: &Value) {
    if let Some(trace) = trace
        && params.get("requestId") == Some(&trace.request_id)
    {
        trace.resolved = true;
    }
}

fn mark_item_completed(trace: Option<&mut ApprovalTrace>, params: &Value) {
    if let Some(trace) = trace
        && params.pointer("/item/id").and_then(Value::as_str) == Some(&trace.item_id)
        && params.get("turnId").and_then(Value::as_str) == Some(&trace.turn_id)
    {
        trace.item_completed =
            params.pointer("/item/status").and_then(Value::as_str) == Some("completed");
    }
}

fn proof_is_complete(trace: Option<&ApprovalTrace>, turn_completed: bool) -> bool {
    trace.is_some_and(|trace| trace.resolved && trace.item_completed && turn_completed)
}

fn verify_git_result(fixture: &GitFixture) -> Result<()> {
    let workspace_head = git_stdout(&fixture.worktree, &["rev-parse", "HEAD"])?;
    ensure!(
        workspace_head != fixture.base_sha,
        "workspace branch did not advance"
    );
    ensure!(
        git_stdout(&fixture.repository, &["rev-parse", "refs/heads/main"])? == fixture.base_sha,
        "source branch advanced during the workspace commit"
    );
    ensure!(
        git_stdout(&fixture.repository, &["rev-parse", PROOF_BRANCH])? == workspace_head,
        "the approved commit did not advance the bound workspace branch"
    );
    ensure!(
        git_stdout(&fixture.worktree, &["rev-parse", "HEAD^"])? == fixture.base_sha,
        "the approved commit does not have the fixed base as its parent"
    );
    ensure!(
        git_stdout(&fixture.worktree, &["show", "-s", "--format=%s", "HEAD"])?
            == PROOF_COMMIT_SUBJECT,
        "the workspace commit subject differs from the proof command"
    );
    ensure!(
        fs::read_to_string(fixture.worktree.join(PROOF_FILE))? == PROOF_CONTENT,
        "the committed proof file has unexpected content"
    );
    ensure!(
        !fixture.repository.join(PROOF_FILE).exists(),
        "the proof file appeared in the source checkout"
    );
    ensure!(
        git_stdout(&fixture.worktree, &["status", "--porcelain"])?.is_empty(),
        "the linked workspace is dirty after the approved commit"
    );
    ensure!(
        git_stdout(
            &fixture.worktree,
            &["diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD"]
        )? == PROOF_FILE,
        "the approved commit changed files outside the allowlisted proof"
    );
    Ok(())
}

fn required_string(value: &Value, pointer: &'static str) -> Result<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .with_context(|| format!("Codex response is missing {pointer}: {value}"))
}

fn git_path(repository: &Path, arguments: &[&str]) -> Result<PathBuf> {
    let value = PathBuf::from(git_stdout(repository, arguments)?);
    let absolute = if value.is_absolute() {
        value
    } else {
        repository.join(value)
    };
    fs::canonicalize(&absolute)
        .with_context(|| format!("could not canonicalize Git path {}", absolute.display()))
}

fn path_text(path: &Path) -> Result<&str> {
    path.to_str()
        .with_context(|| format!("test path is not valid UTF-8: {}", path.display()))
}

fn bash_path() -> Result<String> {
    let output = Command::new("bash")
        .args(["-lc", "printf %s \"$BASH\""])
        .output()
        .context("could not resolve the Bash executable used by Codex")?;
    ensure!(
        output.status.success(),
        "could not resolve the Bash executable: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let path = String::from_utf8(output.stdout)?;
    ensure!(
        Path::new(&path).is_absolute()
            && Path::new(&path)
                .file_name()
                .is_some_and(|name| name == "bash"),
        "resolved Bash path is not an absolute bash executable: {path:?}"
    );
    Ok(path)
}

fn git_stdout(repository: &Path, arguments: &[&str]) -> Result<String> {
    let output = git_output(repository, arguments)?;
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn run_git(repository: &Path, arguments: &[&str]) -> Result<()> {
    git_output(repository, arguments).map(|_| ())
}

fn git_output(repository: &Path, arguments: &[&str]) -> Result<std::process::Output> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(repository)
        .output()
        .context("could not run Git for the native approval proof")?;
    ensure!(
        output.status.success(),
        "git {} failed: {}",
        arguments.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output)
}
