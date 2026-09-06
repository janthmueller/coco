use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use uuid::Uuid;

use crate::domain::ContextMode;
use crate::paths::CocoPaths;
use crate::protocol::{
    RepositoryRegisterParams, TaskCreateParams, TaskDiffParams, TaskGetParams, TaskListParams,
    TurnStartParams,
};
use crate::rpc::RpcClient;

use super::args::{Cli, Command, McpCommand, NewArgs, RepoCommand};
use super::jump::jump;
use super::output::{
    print_diff, print_human, print_json, print_status, print_task_list, versioned, versioned_array,
};
use super::status::follow_status;

pub(super) async fn run(cli: Cli) -> Result<()> {
    let paths = CocoPaths::from_env()?;
    let repository_path =
        std::env::current_dir().context("could not determine current directory")?;

    match cli.command {
        Command::Mcp { command } => run_mcp(command, paths).await,
        Command::Repo { command } => run_repo(command, &paths, &repository_path).await,
        Command::New(args) => prepare_task(&paths, repository_path, args).await,
        Command::Ls { json } => list_tasks(&paths, repository_path, json).await,
        Command::Status { task, follow, json } => {
            show_status(&paths, repository_path, task, follow, json).await
        }
        Command::Send { task, message } => send(&paths, repository_path, task, message).await,
        Command::Jump { task } => jump_to_task(&paths, repository_path, task).await,
        Command::Diff { task } => show_diff(&paths, repository_path, task).await,
    }
}

async fn run_mcp(command: McpCommand, paths: CocoPaths) -> Result<()> {
    let McpCommand::Serve {
        repository,
        allow_send,
    } = command;
    crate::mcp::serve(repository, allow_send, paths.socket_path).await
}

async fn run_repo(command: RepoCommand, paths: &CocoPaths, cwd: &Path) -> Result<()> {
    let RepoCommand::Add { path } = command;
    let path = if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    };
    let result = RpcClient::new(paths.socket_path.clone())
        .request(RepositoryRegisterParams { path })
        .await?;
    print_human(&serde_json::to_value(result)?);
    Ok(())
}

async fn prepare_task(paths: &CocoPaths, cwd: PathBuf, args: NewArgs) -> Result<()> {
    let result = RpcClient::new(paths.socket_path.clone())
        .request(TaskCreateParams {
            repository_path: cwd,
            name: args.name,
            base_ref: args.base,
            context_mode: ContextMode::Fresh,
            profile: args.profile,
            operation_id: Uuid::new_v4().to_string(),
        })
        .await?;
    print_human(&serde_json::to_value(result)?);
    Ok(())
}

async fn list_tasks(paths: &CocoPaths, cwd: PathBuf, json_output: bool) -> Result<()> {
    let result = RpcClient::new(paths.socket_path.clone())
        .request(TaskListParams {
            repository_path: cwd,
            phases: None,
        })
        .await?;
    let result = serde_json::to_value(result)?;
    if json_output {
        print_json(versioned_array("tasks", result))
    } else {
        print_task_list(&result);
        Ok(())
    }
}

async fn show_status(
    paths: &CocoPaths,
    cwd: PathBuf,
    task: String,
    follow: bool,
    json_output: bool,
) -> Result<()> {
    let client = RpcClient::new(paths.socket_path.clone());
    if follow {
        return follow_status(&client, &cwd, &task).await;
    }
    let result = client
        .request(TaskGetParams {
            repository_path: cwd,
            task,
        })
        .await?;
    let result = serde_json::to_value(result)?;
    if json_output {
        print_json(versioned(result))
    } else {
        print_status(&result);
        Ok(())
    }
}

async fn send(paths: &CocoPaths, cwd: PathBuf, task: String, message: String) -> Result<()> {
    if message.trim().is_empty() {
        bail!("message must not be empty");
    }
    let result = RpcClient::new(paths.socket_path.clone())
        .request(TurnStartParams {
            repository_path: cwd,
            task,
            message,
            operation_id: Uuid::new_v4().to_string(),
        })
        .await?;
    print_human(&serde_json::to_value(result)?);
    Ok(())
}

async fn jump_to_task(paths: &CocoPaths, cwd: PathBuf, task: String) -> Result<()> {
    let result = RpcClient::new(paths.socket_path.clone())
        .request(TaskGetParams {
            repository_path: cwd,
            task,
        })
        .await?;
    jump(paths, &serde_json::to_value(result)?).await
}

async fn show_diff(paths: &CocoPaths, cwd: PathBuf, task: String) -> Result<()> {
    let result = RpcClient::new(paths.socket_path.clone())
        .request(TaskDiffParams {
            repository_path: cwd,
            task,
            max_bytes: None,
        })
        .await?;
    print_diff(&serde_json::to_value(result)?);
    Ok(())
}
