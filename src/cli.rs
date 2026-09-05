use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::codex::AppServerEndpoint;
use crate::paths::CocoPaths;
use crate::rpc::RpcClient;

#[derive(Debug, Parser)]
#[command(name = "coco", version, about = "Coordinate isolated Codex work")]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Manage repositories known to CoCo.
    Repo {
        #[command(subcommand)]
        command: RepoCommand,
    },
    /// Prepare a fresh Codex task in an isolated worktree.
    New(NewArgs),
    /// List tasks in the current repository.
    Ls {
        /// Emit stable, machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Show a task's current state, optionally following it until it pauses.
    Status {
        /// Task name or ID.
        task: String,
        /// Keep updating until the task becomes ready, pauses, or finishes.
        #[arg(long, conflicts_with = "json")]
        follow: bool,
        /// Emit stable, machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Start the first or next turn for a ready task.
    Send {
        /// Task name or ID.
        task: String,
        /// Instruction to send to Codex.
        message: String,
    },
    /// Open the task's existing Codex thread in its managed worktree.
    Jump {
        /// Task name or ID.
        task: String,
    },
    /// Show all tracked and untracked changes from the immutable base.
    Diff { task: String },
    /// Run CoCo as a local MCP server.
    Mcp {
        #[command(subcommand)]
        command: McpCommand,
    },
}

#[derive(Debug, Subcommand)]
enum RepoCommand {
    /// Register the Git repository containing PATH.
    Add {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
}

#[derive(Debug, Args)]
struct NewArgs {
    /// Short task name, also used to derive its branch and worktree.
    name: String,
    /// Git revision from which to prepare the task.
    #[arg(long, default_value = "HEAD")]
    base: String,
    /// Apply `[profiles.<PROFILE>]` from `$CODEX_HOME/config.toml` to the thread.
    #[arg(long, default_value = "default")]
    profile: String,
}

#[derive(Debug, Subcommand)]
enum McpCommand {
    /// Serve repository-scoped CoCo tools over stdio.
    Serve {
        #[arg(long)]
        repository: PathBuf,
        /// Advertise the mutating agents.send tool.
        #[arg(long)]
        allow_send: bool,
    },
}

pub async fn run_from_env() -> Result<()> {
    run(Cli::parse()).await
}

async fn run(cli: Cli) -> Result<()> {
    let paths = CocoPaths::from_env()?;
    let repository_path =
        std::env::current_dir().context("could not determine current directory")?;
    match cli.command {
        Command::Mcp {
            command:
                McpCommand::Serve {
                    repository,
                    allow_send,
                },
        } => crate::mcp::serve(repository, allow_send, paths.socket_path).await?,
        Command::Repo {
            command: RepoCommand::Add { path },
        } => {
            let client = RpcClient::new(paths.socket_path);
            let path = if path.is_absolute() {
                path
            } else {
                repository_path.join(path)
            };
            let result = client
                .request("repository.register", json!({ "path": path }))
                .await?;
            print_human(&result);
        }
        Command::New(args) => {
            let client = RpcClient::new(paths.socket_path);
            let result = client
                .request(
                    "task.create",
                    json!({
                        "repositoryPath": repository_path,
                        "name": args.name,
                        "baseRef": args.base,
                        "contextMode": "fresh",
                        "profile": args.profile,
                        "operationId": Uuid::new_v4(),
                    }),
                )
                .await?;
            print_human(&result);
        }
        Command::Ls { json: json_output } => {
            let client = RpcClient::new(paths.socket_path);
            let result = client
                .request("task.list", json!({ "repositoryPath": repository_path }))
                .await?;
            if json_output {
                print_json(versioned_array("tasks", result))?;
            } else {
                print_task_list(&result);
            }
        }
        Command::Status {
            task,
            follow,
            json: json_output,
        } => {
            let client = RpcClient::new(paths.socket_path);
            if follow {
                follow_status(&client, &repository_path, &task).await?;
            } else {
                let result = client
                    .request(
                        "task.get",
                        json!({ "repositoryPath": repository_path, "task": task }),
                    )
                    .await?;
                if json_output {
                    print_json(versioned(result))?;
                } else {
                    print_status(&result);
                }
            }
        }
        Command::Send { task, message } => {
            let client = RpcClient::new(paths.socket_path);
            if message.trim().is_empty() {
                bail!("message must not be empty");
            }
            let result = client
                .request(
                    "turn.start",
                    json!({
                        "repositoryPath": repository_path,
                        "task": task,
                        "message": message,
                        "operationId": Uuid::new_v4(),
                    }),
                )
                .await?;
            print_human(&result);
        }
        Command::Jump { task } => {
            let client = RpcClient::new(paths.socket_path.clone());
            let result = client
                .request(
                    "task.get",
                    json!({ "repositoryPath": repository_path, "task": task }),
                )
                .await?;
            jump(&paths, &result).await?;
        }
        Command::Diff { task } => {
            let client = RpcClient::new(paths.socket_path);
            let result = client
                .request(
                    "task.diff",
                    json!({ "repositoryPath": repository_path, "task": task }),
                )
                .await?;
            print_diff(&result);
        }
    }
    Ok(())
}

async fn follow_status(client: &RpcClient, repository: &Path, task: &str) -> Result<()> {
    let mut after_sequence = 0_i64;
    let mut last_phase: Option<String> = None;
    let mut last_message: Option<String> = None;
    let interactive = io::stdout().is_terminal();
    let spinner = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let mut spinner_index = 0_usize;
    loop {
        let response = client
            .request(
                "event.list",
                json!({
                    "repositoryPath": repository,
                    "task": task,
                    "afterSequence": after_sequence,
                }),
            )
            .await?;
        let events = response
            .get("events")
            .and_then(Value::as_array)
            .context("cocod returned event.list without an events array")?;
        for event in events {
            let kind = event.get("kind").and_then(Value::as_str);
            if kind == Some("turn.started") {
                last_message = None;
            } else if kind == Some("agent.message.completed")
                && let Some(message) = event.pointer("/payload/text").and_then(Value::as_str)
            {
                last_message = Some(message.to_owned());
            }
        }
        after_sequence = response
            .get("nextSequence")
            .and_then(Value::as_i64)
            .unwrap_or(after_sequence);
        let phase = response
            .pointer("/task/phase")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let name = response
            .pointer("/task/name")
            .and_then(Value::as_str)
            .unwrap_or(task);
        if !interactive && last_phase.as_deref() != Some(phase) {
            println!("{name}: {}", phase_label(phase));
        }
        last_phase = Some(phase.to_owned());
        if follow_stops_at(phase) {
            if interactive {
                clear_status_line()?;
                println!("{name}: {}", phase_label(phase));
            }
            if let Some(message) = last_message {
                println!("\n{message}");
            }
            return Ok(());
        }
        for _ in 0..4 {
            if interactive {
                print!(
                    "\r\x1b[2K{} {name}: {}",
                    spinner[spinner_index % spinner.len()],
                    phase_label(phase)
                );
                io::stdout().flush()?;
                spinner_index += 1;
            }
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    if interactive {
                        clear_status_line()?;
                    }
                    return Ok(());
                },
                _ = tokio::time::sleep(Duration::from_millis(125)) => {}
            }
        }
    }
}

async fn jump(paths: &CocoPaths, result: &Value) -> Result<()> {
    let target = load_jump_target(paths, result).await?;
    let codex_binary = std::env::var_os("COCO_CODEX_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("codex"));
    let status = jump_command(&target, codex_binary)
        .status()
        .await
        .context("could not start the Codex terminal UI")?;
    if !status.success() {
        bail!("Codex terminal UI exited with {status}");
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
struct JumpTarget {
    worktree: PathBuf,
    thread_id: String,
    endpoint_url: String,
    capability_token: String,
}

async fn load_jump_target(paths: &CocoPaths, result: &Value) -> Result<JumpTarget> {
    let task = result
        .get("task")
        .context("cocod returned task.get without a task")?;
    let worktree = task
        .get("worktreePath")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .context("task has no managed worktree yet")?;
    let thread_id = task
        .get("codexThreadId")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .context("task has no Codex thread yet")?;
    let descriptor_bytes = tokio::fs::read(&paths.codex_endpoint_path)
        .await
        .with_context(|| {
            format!(
                "could not read {}; is cocod running?",
                paths.codex_endpoint_path.display()
            )
        })?;
    let descriptor: AppServerEndpoint = serde_json::from_slice(&descriptor_bytes)
        .context("cocod published an invalid App Server endpoint")?;
    let port = descriptor.url.strip_prefix("ws://127.0.0.1:");
    if descriptor.schema_version != 1
        || port
            .and_then(|port| port.parse::<u16>().ok())
            .is_none_or(|port| port == 0)
    {
        bail!("cocod published an unsupported App Server endpoint");
    }
    let token = tokio::fs::read_to_string(&paths.codex_token_path)
        .await
        .with_context(|| format!("could not read {}", paths.codex_token_path.display()))?;
    let capability_token = token.trim().to_owned();
    if capability_token.is_empty() {
        bail!("cocod published an empty App Server capability token");
    }

    Ok(JumpTarget {
        worktree,
        thread_id,
        endpoint_url: descriptor.url,
        capability_token,
    })
}

fn jump_command(target: &JumpTarget, codex_binary: PathBuf) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(codex_binary);
    command
        .arg("resume")
        .arg(&target.thread_id)
        .args(["--remote", &target.endpoint_url])
        .args([
            "--remote-auth-token-env",
            "COCO_CODEX_REMOTE_CAPABILITY_TOKEN",
        ])
        .arg("-C")
        .arg(&target.worktree)
        .current_dir(&target.worktree)
        .env(
            "COCO_CODEX_REMOTE_CAPABILITY_TOKEN",
            &target.capability_token,
        )
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    command
}

fn clear_status_line() -> Result<()> {
    print!("\r\x1b[2K");
    io::stdout().flush()?;
    Ok(())
}

fn phase_label(phase: &str) -> &'static str {
    match phase {
        "provisioning" => "Preparing worktree",
        "starting" => "Starting Codex",
        "active" => "Working",
        "waiting_for_approval" => "Waiting for approval",
        "waiting_for_input" => "Waiting for input",
        "idle" => "Ready",
        "completed" => "Completed",
        "failed" => "Failed",
        "interrupted" => "Interrupted",
        _ => "Unknown",
    }
}

fn follow_stops_at(phase: &str) -> bool {
    matches!(
        phase,
        "waiting_for_approval"
            | "waiting_for_input"
            | "idle"
            | "completed"
            | "failed"
            | "interrupted"
    )
}

fn versioned(value: Value) -> Value {
    match value {
        Value::Object(mut object) => {
            object.insert("schemaVersion".into(), Value::from(1));
            Value::Object(object)
        }
        value => json!({ "schemaVersion": 1, "result": value }),
    }
}

fn versioned_array(key: &str, value: Value) -> Value {
    let mut object = serde_json::Map::new();
    object.insert("schemaVersion".to_owned(), Value::from(1));
    object.insert(key.to_owned(), value);
    Value::Object(object)
}

fn print_json(value: Value) -> Result<()> {
    println!("{}", serde_json::to_string(&value)?);
    Ok(())
}

fn print_human(value: &Value) {
    if let Some(task) = value.get("task") {
        print_human(task);
        if let Some(turn_id) = value.get("turnId").and_then(Value::as_str) {
            println!("turn: {turn_id}");
        }
        return;
    }
    if let Some(object) = value.as_object() {
        for key in [
            "id",
            "name",
            "phase",
            "rootPath",
            "worktreePath",
            "branchName",
            "baseSha",
            "codexThreadId",
            "profile",
        ] {
            if let Some(entry) = object.get(key) {
                println!("{}: {}", human_key(key), compact(entry));
            }
        }
    } else {
        println!("{}", compact(value));
    }
}

fn print_task_list(value: &Value) {
    let Some(tasks) = value.as_array() else {
        println!("No tasks.");
        return;
    };
    if tasks.is_empty() {
        println!("No tasks.");
        return;
    }
    println!("ID\tNAME\tPHASE\tBRANCH");
    for task in tasks {
        println!(
            "{}\t{}\t{}\t{}",
            text(task, "id"),
            text(task, "name"),
            text(task, "phase"),
            text(task, "branchName")
        );
    }
}

fn print_status(value: &Value) {
    let task = value.get("task").unwrap_or(value);
    let name = task.get("name").and_then(Value::as_str).unwrap_or("task");
    let phase = task
        .get("phase")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    println!("{name}: {}", phase_label(phase));
    if let Some(worktree) = task.get("worktreePath").and_then(Value::as_str) {
        println!("worktree: {worktree}");
    }
    if let Some(message) = task.get("lastErrorMessage").and_then(Value::as_str) {
        println!("error: {message}");
    }
}

fn print_diff(value: &Value) {
    let patch = value.get("patch").and_then(Value::as_str).unwrap_or("");
    if !patch.is_empty() {
        print!("{patch}");
        if !patch.ends_with('\n') {
            println!();
        }
    }
    let untracked = value
        .get("untrackedPaths")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    if !untracked.is_empty() {
        println!("Untracked:");
        for path in untracked {
            println!("{}", compact(path));
        }
    }
    if patch.is_empty() && untracked.is_empty() {
        println!("No changes.");
    }
}

fn text(value: &Value, key: &str) -> String {
    value.get(key).map(compact).unwrap_or_else(|| "-".into())
}

fn compact(value: &Value) -> String {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| value.to_string())
}

fn human_key(value: &str) -> String {
    let mut output = String::new();
    for (index, character) in value.chars().enumerate() {
        if index > 0 && character.is_uppercase() {
            output.push('_');
        }
        output.extend(character.to_lowercase());
    }
    output
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use clap::Parser;

    use super::*;

    #[test]
    fn parses_task_preparation_with_an_optional_profile() {
        let minimal = Cli::try_parse_from(["coco", "new", "auth"]);
        assert!(minimal.is_ok());

        let configured =
            Cli::try_parse_from(["coco", "new", "auth", "--base", "main", "--profile", "dev"]);
        assert!(configured.is_ok());

        assert!(Cli::try_parse_from(["coco", "new", "auth", "--goal", "work"]).is_err());
        assert!(Cli::try_parse_from(["coco", "new", "auth", "--context", "fresh"]).is_err());
    }

    #[test]
    fn repository_registration_is_a_nested_repo_command() {
        assert!(Cli::try_parse_from(["coco", "repo", "add"]).is_ok());
        assert!(Cli::try_parse_from(["coco", "repo", "add", "../source"]).is_ok());
        assert!(Cli::try_parse_from(["coco", "init"]).is_err());
    }

    #[test]
    fn exposes_status_follow_and_jump_without_the_old_overlapping_commands() {
        assert!(Cli::try_parse_from(["coco", "status", "auth"]).is_ok());
        assert!(Cli::try_parse_from(["coco", "status", "auth", "--follow"]).is_ok());
        assert!(Cli::try_parse_from(["coco", "status", "auth", "--json"]).is_ok());
        assert!(Cli::try_parse_from(["coco", "status", "auth", "--follow", "--json"]).is_err());
        assert!(Cli::try_parse_from(["coco", "jump", "auth"]).is_ok());
        assert!(Cli::try_parse_from(["coco", "show", "auth"]).is_err());
        assert!(Cli::try_parse_from(["coco", "watch", "auth"]).is_err());
    }

    #[test]
    fn presents_stable_user_facing_task_states() {
        assert_eq!(phase_label("provisioning"), "Preparing worktree");
        assert_eq!(phase_label("active"), "Working");
        assert_eq!(phase_label("waiting_for_approval"), "Waiting for approval");
        assert_eq!(phase_label("idle"), "Ready");
        assert!(follow_stops_at("waiting_for_input"));
        assert!(!follow_stops_at("active"));
    }

    #[tokio::test]
    async fn builds_an_authenticated_jump_into_the_managed_worktree() {
        let directory = tempfile::tempdir().unwrap();
        let worktree = directory.path().join("worktree");
        std::fs::create_dir(&worktree).unwrap();
        let paths = CocoPaths {
            data_dir: directory.path().join("data"),
            database_path: directory.path().join("coco.db"),
            socket_path: directory.path().join("cocod.sock"),
            codex_endpoint_path: directory.path().join("codex-app-server.json"),
            codex_token_path: directory.path().join("codex-app-server.token"),
            worktrees_dir: directory.path().join("worktrees"),
            codex_home: directory.path().join("codex-home"),
        };
        std::fs::write(
            &paths.codex_endpoint_path,
            r#"{"schemaVersion":1,"url":"ws://127.0.0.1:45123"}"#,
        )
        .unwrap();
        std::fs::write(&paths.codex_token_path, "test-capability\n").unwrap();
        let response = json!({
            "task": {
                "worktreePath": worktree,
                "codexThreadId": "thread-123"
            }
        });

        let target = load_jump_target(&paths, &response).await.unwrap();
        let command = jump_command(&target, PathBuf::from("/opt/codex"));
        let command = command.as_std();
        let arguments = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(command.get_program(), OsStr::new("/opt/codex"));
        assert_eq!(
            arguments,
            [
                "resume",
                "thread-123",
                "--remote",
                "ws://127.0.0.1:45123",
                "--remote-auth-token-env",
                "COCO_CODEX_REMOTE_CAPABILITY_TOKEN",
                "-C",
                target.worktree.to_str().unwrap(),
            ]
        );
        assert_eq!(command.get_current_dir(), Some(target.worktree.as_path()));
        assert!(command.get_envs().any(|(name, value)| {
            name == OsStr::new("COCO_CODEX_REMOTE_CAPABILITY_TOKEN")
                && value == Some(OsStr::new("test-capability"))
        }));
    }
}
