use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use uuid::Uuid;

use crate::domain::ContextMode;
use crate::paths::CocoPaths;
use crate::protocol::{
    RepositoryRegisterParams, TurnStartParams, WorkspaceCreateParams, WorkspaceDiffParams,
    WorkspaceGetParams, WorkspaceListParams, WorkspaceResult,
};
use crate::rpc::RpcClient;

use super::args::{Cli, Command, CreateArgs, McpCommand, RepoCommand};
use super::jump::jump;
use super::output::{
    print_diff, print_human, print_json, print_status, print_workspace_list, versioned,
    versioned_array,
};
use super::status::follow_status;

pub(super) async fn run(cli: Cli) -> Result<()> {
    let paths = CocoPaths::from_env()?;
    let repository_path =
        std::env::current_dir().context("could not determine current directory")?;

    match cli.command {
        Command::Mcp { command } => run_mcp(command, paths).await,
        Command::Repo { command } => run_repo(command, &paths, &repository_path).await,
        Command::Create(args) => create_workspace(&paths, repository_path, args).await,
        Command::Ls { json } => list_workspaces(&paths, repository_path, json).await,
        Command::Status {
            workspace,
            follow,
            json,
        } => show_status(&paths, repository_path, workspace, follow, json).await,
        Command::Send { workspace, message } => {
            send(&paths, repository_path, workspace, message).await
        }
        Command::Jump { workspace } => jump_to_workspace(&paths, repository_path, workspace).await,
        Command::Diff { workspace } => show_diff(&paths, repository_path, workspace).await,
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
    print_human(&serde_json::to_value(&result)?);
    Ok(())
}

async fn create_workspace(paths: &CocoPaths, cwd: PathBuf, args: CreateArgs) -> Result<()> {
    let CreateArgs {
        name,
        base,
        profile,
        send: initial_message,
        jump: should_jump,
    } = args;
    let client = RpcClient::new(paths.socket_path.clone());
    let mut result: WorkspaceResult = client
        .request(WorkspaceCreateParams {
            repository_path: cwd.clone(),
            name: name.clone(),
            base_ref: base,
            context_mode: ContextMode::Fresh,
            profile,
            operation_id: Uuid::new_v4().to_string(),
        })
        .await?;
    let workspace_id = result.workspace.id.clone();
    let sent = initial_message.is_some();
    if let Some(message) = initial_message {
        result = request_turn(&client, &cwd, workspace_id, message)
            .await
            .with_context(|| {
                format!("workspace {name:?} was created, but its initial message was not accepted")
            })?;
    }
    print_human(&serde_json::to_value(&result)?);
    if should_jump {
        let result = serde_json::to_value(&result)?;
        jump(paths, &result).await.with_context(|| {
            if sent {
                format!(
                    "workspace {name:?} was created and its initial turn was accepted, but the Codex terminal UI did not open"
                )
            } else {
                format!(
                    "workspace {name:?} was created, but the Codex terminal UI did not open"
                )
            }
        })?;
    }
    Ok(())
}

async fn list_workspaces(paths: &CocoPaths, cwd: PathBuf, json_output: bool) -> Result<()> {
    let result = RpcClient::new(paths.socket_path.clone())
        .request(WorkspaceListParams {
            repository_path: cwd,
            phases: None,
        })
        .await?;
    let result = serde_json::to_value(result)?;
    if json_output {
        print_json(versioned_array("workspaces", result))
    } else {
        print_workspace_list(&result);
        Ok(())
    }
}

async fn show_status(
    paths: &CocoPaths,
    cwd: PathBuf,
    workspace: String,
    follow: bool,
    json_output: bool,
) -> Result<()> {
    let client = RpcClient::new(paths.socket_path.clone());
    if follow {
        return follow_status(&client, &cwd, &workspace).await;
    }
    let result = client
        .request(WorkspaceGetParams {
            repository_path: cwd,
            workspace,
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

async fn send(paths: &CocoPaths, cwd: PathBuf, workspace: String, message: String) -> Result<()> {
    let result = request_turn(
        &RpcClient::new(paths.socket_path.clone()),
        &cwd,
        workspace,
        message,
    )
    .await?;
    print_human(&serde_json::to_value(result)?);
    Ok(())
}

async fn request_turn(
    client: &RpcClient,
    cwd: &Path,
    workspace: String,
    message: String,
) -> Result<WorkspaceResult> {
    if message.trim().is_empty() {
        bail!("message must not be empty");
    }
    Ok(client
        .request(TurnStartParams {
            repository_path: cwd.to_path_buf(),
            workspace,
            message,
            operation_id: Uuid::new_v4().to_string(),
        })
        .await?)
}

async fn jump_to_workspace(paths: &CocoPaths, cwd: PathBuf, workspace: String) -> Result<()> {
    let result = RpcClient::new(paths.socket_path.clone())
        .request(WorkspaceGetParams {
            repository_path: cwd,
            workspace,
        })
        .await?;
    jump(paths, &serde_json::to_value(result)?).await
}

async fn show_diff(paths: &CocoPaths, cwd: PathBuf, workspace: String) -> Result<()> {
    let result = RpcClient::new(paths.socket_path.clone())
        .request(WorkspaceDiffParams {
            repository_path: cwd,
            workspace,
            max_bytes: None,
        })
        .await?;
    print_diff(&serde_json::to_value(result)?);
    Ok(())
}
