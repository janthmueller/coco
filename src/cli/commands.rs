use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use uuid::Uuid;

use crate::domain::ContextMode;
use crate::paths::CocoPaths;
use crate::protocol::{
    RepositoryListParams, RepositoryRegisterParams, RepositoryScope, TurnStartParams,
    WorkspaceCreateParams, WorkspaceDiffParams, WorkspaceGetParams, WorkspaceListParams,
    WorkspaceResult,
};
use crate::rpc::RpcClient;

use super::args::{Cli, Command, CreateArgs, McpCommand, RepoCommand};
use super::jump::jump;
use super::output::{
    print_diff, print_human, print_json, print_repository_list, print_status, print_workspace_list,
    versioned, versioned_array,
};
use super::status::follow_status;

pub(super) async fn run(cli: Cli) -> Result<()> {
    let paths = CocoPaths::from_env()?;
    let cwd = std::env::current_dir().context("could not determine current directory")?;
    let Cli {
        scope_path,
        all_repos,
        command,
    } = cli;
    validate_scope_selection(scope_path.is_some(), all_repos)?;
    let has_explicit_scope = scope_path.is_some() || all_repos;
    let repository_path = resolve_repository_path(&cwd, scope_path);
    let scope = if all_repos {
        RepositoryScope::AllRepositories
    } else {
        RepositoryScope::repository(repository_path.clone())
    };

    match command {
        Command::Mcp { command } => {
            reject_top_level_scope(has_explicit_scope, "mcp")?;
            run_mcp(command, paths).await
        }
        Command::Repo { command } => {
            reject_top_level_scope(has_explicit_scope, "repo")?;
            run_repo(command, &paths, &cwd).await
        }
        Command::Create(args) => {
            if all_repos {
                bail!("coco create requires one repository; omit --all-repos");
            }
            create_workspace(&paths, repository_path, args).await
        }
        Command::Ls { json } => list_workspaces(&paths, scope, json).await,
        Command::Status {
            workspace,
            follow,
            json,
        } => {
            show_status(
                &paths,
                scope_for_reference(scope, &workspace),
                workspace,
                follow,
                json,
            )
            .await
        }
        Command::Send { workspace, message } => {
            send(
                &paths,
                scope_for_reference(scope, &workspace),
                workspace,
                message,
            )
            .await
        }
        Command::Jump { workspace } => {
            jump_to_workspace(&paths, scope_for_reference(scope, &workspace), workspace).await
        }
        Command::Diff { workspace } => {
            show_diff(&paths, scope_for_reference(scope, &workspace), workspace).await
        }
    }
}

pub(super) fn validate_scope_selection(has_path: bool, all_repos: bool) -> Result<()> {
    if has_path && all_repos {
        bail!("a repository path and --all-repos cannot be used together");
    }
    Ok(())
}

fn resolve_repository_path(cwd: &Path, path: Option<PathBuf>) -> PathBuf {
    match path {
        Some(path) if path.is_absolute() => path,
        Some(path) => cwd.join(path),
        None => cwd.to_path_buf(),
    }
}

fn reject_top_level_scope(has_explicit_scope: bool, command: &str) -> Result<()> {
    if has_explicit_scope {
        bail!("coco {command} does not accept a leading repository path or --all-repos");
    }
    Ok(())
}

fn scope_for_reference(scope: RepositoryScope, reference: &str) -> RepositoryScope {
    if Uuid::parse_str(reference).is_ok() {
        RepositoryScope::AllRepositories
    } else {
        scope
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
    let client = RpcClient::new(paths.socket_path.clone());
    match command {
        RepoCommand::Add { path } => {
            let path = resolve_repository_path(cwd, Some(path));
            let result = client.request(RepositoryRegisterParams { path }).await?;
            print_human(&serde_json::to_value(&result)?);
            Ok(())
        }
        RepoCommand::List { json } => {
            let result = client.request(RepositoryListParams {}).await?;
            let result = serde_json::to_value(result)?;
            if json {
                print_json(versioned_array("repositories", result))
            } else {
                print_repository_list(&result);
                Ok(())
            }
        }
    }
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
        result = request_turn(
            &client,
            RepositoryScope::repository(cwd.clone()),
            workspace_id,
            message,
        )
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

async fn list_workspaces(
    paths: &CocoPaths,
    scope: RepositoryScope,
    json_output: bool,
) -> Result<()> {
    let include_repository = matches!(scope, RepositoryScope::AllRepositories);
    let result = RpcClient::new(paths.socket_path.clone())
        .request(WorkspaceListParams {
            scope,
            phases: None,
        })
        .await?;
    let result = serde_json::to_value(result)?;
    if json_output {
        print_json(versioned_array("workspaces", result))
    } else {
        print_workspace_list(&result, include_repository);
        Ok(())
    }
}

async fn show_status(
    paths: &CocoPaths,
    scope: RepositoryScope,
    workspace: String,
    follow: bool,
    json_output: bool,
) -> Result<()> {
    let client = RpcClient::new(paths.socket_path.clone());
    if follow {
        return follow_status(&client, scope, &workspace).await;
    }
    let result = client
        .request(WorkspaceGetParams { scope, workspace })
        .await?;
    let result = serde_json::to_value(result)?;
    if json_output {
        print_json(versioned(result))
    } else {
        print_status(&result);
        Ok(())
    }
}

async fn send(
    paths: &CocoPaths,
    scope: RepositoryScope,
    workspace: String,
    message: String,
) -> Result<()> {
    let result = request_turn(
        &RpcClient::new(paths.socket_path.clone()),
        scope,
        workspace,
        message,
    )
    .await?;
    print_human(&serde_json::to_value(result)?);
    Ok(())
}

async fn request_turn(
    client: &RpcClient,
    scope: RepositoryScope,
    workspace: String,
    message: String,
) -> Result<WorkspaceResult> {
    if message.trim().is_empty() {
        bail!("message must not be empty");
    }
    Ok(client
        .request(TurnStartParams {
            scope,
            workspace,
            message,
            operation_id: Uuid::new_v4().to_string(),
        })
        .await?)
}

async fn jump_to_workspace(
    paths: &CocoPaths,
    scope: RepositoryScope,
    workspace: String,
) -> Result<()> {
    let result = RpcClient::new(paths.socket_path.clone())
        .request(WorkspaceGetParams { scope, workspace })
        .await?;
    jump(paths, &serde_json::to_value(result)?).await
}

async fn show_diff(paths: &CocoPaths, scope: RepositoryScope, workspace: String) -> Result<()> {
    let result = RpcClient::new(paths.socket_path.clone())
        .request(WorkspaceDiffParams {
            scope,
            workspace,
            max_bytes: None,
        })
        .await?;
    print_diff(&serde_json::to_value(result)?);
    Ok(())
}
