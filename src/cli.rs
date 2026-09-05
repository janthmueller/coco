use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use serde_json::{Value, json};
use uuid::Uuid;

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
    /// Create a fresh Codex task in an isolated worktree.
    New(NewArgs),
    /// List tasks in the current repository.
    Ls {
        #[arg(long)]
        json: bool,
    },
    /// Show one task by repository-local name or global ID.
    Show {
        task: String,
        #[arg(long)]
        json: bool,
    },
    /// Start another turn for an idle task.
    Send { task: String, message: String },
    /// Follow normalized task events until the current turn stops.
    Watch {
        task: String,
        #[arg(long)]
        json: bool,
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
    name: String,
    #[arg(long)]
    base: String,
    #[arg(long, value_parser = ["fresh"])]
    context: String,
    #[arg(long)]
    goal: String,
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
            if args.goal.trim().is_empty() {
                bail!("--goal must not be empty");
            }
            let result = client
                .request(
                    "task.create",
                    json!({
                        "repositoryPath": repository_path,
                        "name": args.name,
                        "baseRef": args.base,
                        "contextMode": args.context,
                        "goal": args.goal,
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
        Command::Show {
            task,
            json: json_output,
        } => {
            let client = RpcClient::new(paths.socket_path);
            let result = client
                .request(
                    "task.get",
                    json!({ "repositoryPath": repository_path, "task": task }),
                )
                .await?;
            if json_output {
                print_json(versioned(result))?;
            } else {
                print_human(&result);
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
        Command::Watch { task, json } => {
            let client = RpcClient::new(paths.socket_path);
            watch(&client, &repository_path, &task, json).await?;
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

async fn watch(
    client: &RpcClient,
    repository: &std::path::Path,
    task: &str,
    json: bool,
) -> Result<()> {
    let mut after_sequence = 0_i64;
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
            if json {
                print_json(versioned(event.clone()))?;
            } else {
                println!("{}", format_event(event));
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
        if matches!(phase, "idle" | "failed" | "interrupted" | "completed") {
            return Ok(());
        }
        tokio::select! {
            _ = tokio::signal::ctrl_c() => return Ok(()),
            _ = tokio::time::sleep(Duration::from_millis(500)) => {}
        }
    }
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

fn format_event(event: &Value) -> String {
    let sequence = event.get("sequence").and_then(Value::as_i64).unwrap_or(0);
    let kind = event.get("kind").and_then(Value::as_str).unwrap_or("event");
    let detail = event
        .pointer("/payload/text")
        .or_else(|| event.pointer("/payload/message"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if detail.is_empty() {
        format!("[{sequence}] {kind}")
    } else {
        format!("[{sequence}] {kind}: {detail}")
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
    use clap::Parser;

    use super::*;

    #[test]
    fn parses_named_profile_and_rejects_non_fresh_context() {
        let cli = Cli::try_parse_from([
            "coco",
            "new",
            "auth",
            "--base",
            "main",
            "--context",
            "fresh",
            "--goal",
            "Implement auth",
            "--profile",
            "dev",
        ]);
        assert!(cli.is_ok());

        let invalid = Cli::try_parse_from([
            "coco",
            "new",
            "auth",
            "--base",
            "main",
            "--context",
            "fork",
            "--goal",
            "Implement auth",
        ]);
        assert!(invalid.is_err());
    }

    #[test]
    fn repository_registration_is_a_nested_repo_command() {
        assert!(Cli::try_parse_from(["coco", "repo", "add"]).is_ok());
        assert!(Cli::try_parse_from(["coco", "repo", "add", "../source"]).is_ok());
        assert!(Cli::try_parse_from(["coco", "init"]).is_err());
    }
}
