use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use uuid::Uuid;

use crate::paths::CocoPaths;
use crate::protocol::{
    ModelListParams, RepositoryListParams, RepositoryRegisterParams, RepositoryResolveParams,
    RepositoryScope, RepositorySummary, TurnStartParams, WorkspaceAttachParams,
    WorkspaceBaseRequest, WorkspaceChangesRequest, WorkspaceContextRequest, WorkspaceContextSource,
    WorkspaceCreateParams, WorkspaceDiffParams, WorkspaceGetParams, WorkspaceListItem,
    WorkspaceListParams, WorkspaceResult, WorkspaceWorktreeRequest,
};
use crate::rpc::{RpcClient, RpcClientError};

use super::args::{Cli, Command, CreateArgs, McpCommand, ModelCommand, RepoCommand, StatusArgs};
use super::decision::decide;
use super::jump::jump;
use super::output::{
    print_diff, print_human, print_json, print_model_list, print_repository_list, print_status,
    print_workspace_list, versioned, versioned_array,
};
use super::prompt::{Choice, Interaction, TerminalInteraction};
use super::status::follow_status;

pub(super) async fn run(cli: Cli) -> Result<()> {
    let mut interaction = TerminalInteraction::detect(cli.no_input);
    run_with_interaction(cli, &mut interaction).await
}

async fn run_with_interaction(cli: Cli, interaction: &mut dyn Interaction) -> Result<()> {
    let paths = CocoPaths::from_env()?;
    let cwd = std::env::current_dir().context("could not determine current directory")?;
    let all_repos = cli.requests_all_repositories();
    let global = cli.requests_global_search();
    let Cli {
        scope_path,
        command,
        ..
    } = cli;
    let has_scope_path = scope_path.is_some();
    validate_scope_selection(scope_path.is_some(), all_repos, global)?;
    let has_explicit_scope = scope_path.is_some() || all_repos || global;
    let repository_path = resolve_repository_path(&cwd, scope_path);

    match command {
        Command::Mcp { command } => {
            reject_top_level_scope(has_explicit_scope, "mcp")?;
            run_mcp(command, paths).await
        }
        Command::Repo { command } => {
            reject_top_level_scope(has_explicit_scope, "repo")?;
            run_repo(command, &paths, &cwd).await
        }
        Command::Model { command } => {
            reject_top_level_scope(has_explicit_scope, "model")?;
            run_model(command, &paths).await
        }
        Command::Models { json } => {
            reject_top_level_scope(has_explicit_scope, "models")?;
            list_models(&paths, json).await
        }
        Command::Create(args) => {
            run_create(
                &paths,
                repository_path,
                has_scope_path,
                all_repos || global,
                args,
                interaction,
            )
            .await
        }
        Command::List { json, .. } => {
            if global {
                bail!("coco list accepts --all-repos for an overview, not --global");
            }
            let scope = overview_scope(RepositoryScope::repository(repository_path), all_repos);
            list_workspaces(&paths, scope, json).await
        }
        Command::Status(args) => {
            reject_all_repos_for_reference(all_repos, "status")?;
            run_status(
                &paths,
                args,
                workspace_selection(&repository_path, has_scope_path, None, global),
                interaction,
            )
            .await
        }
        Command::Send {
            workspace,
            message,
            operation_id,
            ..
        } => {
            reject_all_repos_for_reference(all_repos, "send")?;
            run_send(
                &paths,
                workspace_selection(&repository_path, has_scope_path, workspace, global),
                message,
                operation_id,
                interaction,
            )
            .await
        }
        Command::Jump { workspace, .. } => {
            reject_all_repos_for_reference(all_repos, "jump")?;
            run_jump(
                &paths,
                workspace_selection(&repository_path, has_scope_path, workspace, global),
                interaction,
            )
            .await
        }
        Command::Decide { decision, choice } => {
            reject_top_level_scope(has_explicit_scope, "decide")?;
            decide(&paths, decision, choice, interaction).await
        }
        Command::Diff { workspace, .. } => {
            reject_all_repos_for_reference(all_repos, "diff")?;
            run_diff(
                &paths,
                workspace_selection(&repository_path, has_scope_path, workspace, global),
                interaction,
            )
            .await
        }
    }
}

async fn run_model(command: ModelCommand, paths: &CocoPaths) -> Result<()> {
    let ModelCommand::List { json } = command;
    list_models(paths, json).await
}

async fn run_create(
    paths: &CocoPaths,
    repository_path: PathBuf,
    has_explicit_path: bool,
    has_collection_scope: bool,
    args: CreateArgs,
    interaction: &mut dyn Interaction,
) -> Result<()> {
    if has_collection_scope {
        bail!("coco create requires one repository; omit --all-repos and --global");
    }
    if args.name.is_none() {
        require_interactive(interaction, "workspace name")?;
    }
    let repository_path =
        resolve_interactive_repository(paths, repository_path, has_explicit_path, interaction)
            .await?;
    create_workspace(paths, repository_path, args, interaction).await
}

async fn list_models(paths: &CocoPaths, json_output: bool) -> Result<()> {
    let models = RpcClient::new(paths.socket_path.clone())
        .request(ModelListParams {})
        .await?;
    let models = serde_json::to_value(models)?;
    if json_output {
        print_json(versioned_array("models", models))
    } else {
        print_model_list(&models);
        Ok(())
    }
}

pub(super) fn validate_scope_selection(
    has_path: bool,
    all_repos: bool,
    global: bool,
) -> Result<()> {
    if all_repos && global {
        bail!("--all-repos and --global cannot be used together");
    }
    if has_path && all_repos {
        bail!("a repository path and --all-repos cannot be used together");
    }
    if has_path && global {
        bail!("a repository path and --global cannot be used together");
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
        bail!("coco {command} does not accept a leading repository path, --all-repos, or --global");
    }
    Ok(())
}

fn overview_scope(scope: RepositoryScope, all_repos: bool) -> RepositoryScope {
    if all_repos {
        RepositoryScope::AllRepositories
    } else {
        scope
    }
}

fn scope_for_reference(scope: RepositoryScope, reference: &str, global: bool) -> RepositoryScope {
    if global || Uuid::parse_str(reference).is_ok() {
        RepositoryScope::AllRepositories
    } else {
        scope
    }
}

fn reject_all_repos_for_reference(all_repos: bool, command: &str) -> Result<()> {
    if all_repos {
        bail!(
            "coco {command} targets one workspace; use --global to resolve its name across repositories"
        );
    }
    Ok(())
}

struct WorkspaceSelection {
    repository_scope: RepositoryScope,
    has_explicit_path: bool,
    workspace: Option<String>,
    global: bool,
}

struct ResolvedWorkspaceTarget {
    scope: RepositoryScope,
    workspace: String,
}

fn workspace_selection(
    repository_path: &Path,
    has_explicit_path: bool,
    workspace: Option<String>,
    global: bool,
) -> WorkspaceSelection {
    WorkspaceSelection {
        repository_scope: RepositoryScope::repository(repository_path.to_path_buf()),
        has_explicit_path,
        workspace,
        global,
    }
}

async fn resolve_interactive_repository(
    paths: &CocoPaths,
    requested_path: PathBuf,
    has_explicit_path: bool,
    interaction: &mut dyn Interaction,
) -> Result<PathBuf> {
    let client = RpcClient::new(paths.socket_path.clone());
    match client
        .request(RepositoryResolveParams {
            path: requested_path.clone(),
        })
        .await
    {
        Ok(_) => Ok(requested_path),
        Err(error)
            if !has_explicit_path
                && interaction.is_interactive()
                && is_unresolved_repository(&error) =>
        {
            choose_repository(&client, interaction).await
        }
        Err(error) => Err(error.into()),
    }
}

fn is_unresolved_repository(error: &RpcClientError) -> bool {
    matches!(
        error,
        RpcClientError::Remote(payload)
            if matches!(
                payload.code.as_str(),
                "REPOSITORY_NOT_REGISTERED" | "NOT_A_GIT_REPOSITORY"
            )
    )
}

async fn choose_repository(
    client: &RpcClient,
    interaction: &mut dyn Interaction,
) -> Result<PathBuf> {
    let repositories = client.request(RepositoryListParams {}).await?;
    if repositories.is_empty() {
        bail!("no repositories are registered; run `coco repo add <PATH>` first");
    }
    let choices = repositories
        .iter()
        .map(repository_choice)
        .collect::<Vec<_>>();
    let selected = interaction.select("Choose a repository", &choices)?;
    Ok(repositories[selected].root_path.clone())
}

fn repository_choice(repository: &RepositorySummary) -> Choice {
    Choice::new(
        repository.display_name.clone(),
        Some(repository.root_path.display().to_string()),
    )
}

async fn resolve_workspace_input(
    paths: &CocoPaths,
    selection: WorkspaceSelection,
    title: &str,
    interaction: &mut dyn Interaction,
) -> Result<ResolvedWorkspaceTarget> {
    let WorkspaceSelection {
        repository_scope,
        has_explicit_path,
        workspace,
        global,
    } = selection;
    if let Some(workspace) = workspace {
        return Ok(ResolvedWorkspaceTarget {
            scope: scope_for_reference(repository_scope, &workspace, global),
            workspace,
        });
    }
    require_interactive(interaction, "workspace")?;
    let client = RpcClient::new(paths.socket_path.clone());
    let scope = if global {
        RepositoryScope::AllRepositories
    } else {
        let RepositoryScope::Repository { path } = repository_scope else {
            unreachable!("a non-global workspace picker always begins with a repository scope")
        };
        RepositoryScope::repository(
            resolve_interactive_repository(paths, path, has_explicit_path, interaction).await?,
        )
    };
    let workspaces = client
        .request(WorkspaceListParams {
            scope,
            phases: None,
        })
        .await?;
    if workspaces.is_empty() {
        let scope_hint = if global {
            "across registered repositories"
        } else {
            "in the selected repository; use --global to choose across repositories"
        };
        bail!("no workspaces are available {scope_hint}; create one with `coco create <NAME>`");
    }
    let choices = workspaces.iter().map(workspace_choice).collect::<Vec<_>>();
    let selected = interaction.select(title, &choices)?;
    Ok(ResolvedWorkspaceTarget {
        scope: RepositoryScope::AllRepositories,
        workspace: workspaces[selected].workspace.id.clone(),
    })
}

fn workspace_choice(item: &WorkspaceListItem) -> Choice {
    let branch = item.workspace.branch_name.as_deref().unwrap_or("detached");
    Choice::new(
        item.workspace.name.clone(),
        Some(format!(
            "{} | {} | {branch}",
            item.repository.root_path.display(),
            super::output::phase_label(item.workspace.phase.as_str()),
        )),
    )
}

fn require_interactive(interaction: &dyn Interaction, field: &str) -> Result<()> {
    if interaction.is_interactive() {
        Ok(())
    } else {
        bail!(
            "{field} is required without interactive input; pass it explicitly or run from a terminal without --no-input"
        )
    }
}

fn resolve_text_input(
    value: Option<String>,
    field: &str,
    prompt: &str,
    interaction: &mut dyn Interaction,
) -> Result<String> {
    if let Some(value) = value {
        return Ok(value);
    }
    require_interactive(interaction, field)?;
    interaction.text(prompt)
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

async fn create_workspace(
    paths: &CocoPaths,
    cwd: PathBuf,
    mut args: CreateArgs,
    interaction: &mut dyn Interaction,
) -> Result<()> {
    args.name = Some(resolve_text_input(
        args.name.take(),
        "workspace name",
        "Workspace name",
        interaction,
    )?);
    let (params, initial_message, should_jump) =
        normalize_create_args(cwd, args, Uuid::new_v4().to_string())?;
    let name = params.name.clone();
    let repository_path = params.repository_path.clone();
    let client = RpcClient::new(paths.socket_path.clone());
    let mut result: WorkspaceResult = client.request(params).await?;
    let workspace_id = result.workspace.id.clone();
    let sent = initial_message.is_some();
    if let Some(message) = initial_message {
        result = request_turn(
            &client,
            RepositoryScope::repository(repository_path.clone()),
            workspace_id.clone(),
            message,
            None,
        )
        .await
        .with_context(|| {
            format!("workspace {name:?} was created, but its initial message was not accepted")
        })?;
    }
    print_human(&serde_json::to_value(&result)?);
    if should_jump {
        attach_and_jump(
            paths,
            &client,
            RepositoryScope::repository(repository_path),
            workspace_id,
        )
        .await
        .with_context(|| {
            if sent {
                format!(
                    "workspace {name:?} was created and its initial turn was accepted, but the Codex terminal UI did not open"
                )
            } else {
                format!("workspace {name:?} was created, but the Codex terminal UI did not open")
            }
        })?;
    }
    Ok(())
}

pub(super) fn normalize_create_args(
    repository_path: PathBuf,
    args: CreateArgs,
    operation_id: String,
) -> Result<(WorkspaceCreateParams, Option<String>, bool)> {
    let CreateArgs {
        name,
        base,
        base_workspace,
        context_workspace,
        context_thread,
        fork_from,
        compact,
        branch,
        checkout,
        detached,
        carry_changes,
        carry_untracked,
        dirty,
        profile,
        model,
        send: initial_message,
        jump: should_jump,
    } = args;
    let name = name.context("workspace name must be resolved before creation")?;
    let (base, context) = if let Some(source) = fork_from {
        (
            WorkspaceBaseRequest::Workspace {
                workspace: source.clone(),
            },
            WorkspaceContextRequest::Fork {
                source: WorkspaceContextSource::Workspace { workspace: source },
                compact,
            },
        )
    } else {
        let base = match base_workspace {
            Some(workspace) => WorkspaceBaseRequest::Workspace { workspace },
            None => WorkspaceBaseRequest::Revision {
                revision: base.unwrap_or_else(|| "HEAD".to_owned()),
            },
        };
        let source = match (context_workspace, context_thread) {
            (Some(workspace), None) => Some(WorkspaceContextSource::Workspace { workspace }),
            (None, Some(thread_id)) => Some(WorkspaceContextSource::Thread { thread_id }),
            (None, None) => None,
            (Some(_), Some(_)) => unreachable!("clap rejects conflicting context sources"),
        };
        let context = match source {
            Some(source) => WorkspaceContextRequest::Fork { source, compact },
            None if compact => {
                bail!("--compact requires --context-workspace, --context-thread, or --fork-from")
            }
            None => WorkspaceContextRequest::Fresh,
        };
        (base, context)
    };
    let worktree = if let Some(branch) = checkout {
        WorkspaceWorktreeRequest::ExistingBranch { branch }
    } else if detached {
        WorkspaceWorktreeRequest::Detached { base }
    } else {
        WorkspaceWorktreeRequest::NewBranch { branch, base }
    };
    if carry_untracked && !carry_changes && !dirty {
        bail!("--carry-untracked requires --carry-changes or --dirty");
    }
    let changes = if dirty || carry_untracked {
        WorkspaceChangesRequest::CarryTrackedAndUntracked
    } else if carry_changes {
        WorkspaceChangesRequest::CarryTracked
    } else {
        WorkspaceChangesRequest::Reject
    };
    Ok((
        WorkspaceCreateParams {
            repository_path,
            name: name.clone(),
            context,
            worktree,
            changes,
            profile,
            model,
            operation_id,
        },
        initial_message,
        should_jump,
    ))
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

async fn run_status(
    paths: &CocoPaths,
    args: StatusArgs,
    mut selection: WorkspaceSelection,
    interaction: &mut dyn Interaction,
) -> Result<()> {
    if args.workspace.is_none() && args.json {
        bail!("coco status --json requires a workspace; use `coco list --json` for an overview");
    }
    selection.workspace = args.workspace;
    let target = resolve_workspace_input(
        paths,
        selection,
        "Choose a workspace for status",
        interaction,
    )
    .await?;
    show_status(
        paths,
        target.scope,
        target.workspace,
        args.follow,
        args.json,
    )
    .await
}

async fn run_send(
    paths: &CocoPaths,
    selection: WorkspaceSelection,
    message: Option<String>,
    operation_id: Option<String>,
    interaction: &mut dyn Interaction,
) -> Result<()> {
    let target = resolve_workspace_input(
        paths,
        selection,
        "Choose a workspace to send to",
        interaction,
    )
    .await?;
    let message = resolve_text_input(message, "message", "Message", interaction)?;
    send(paths, target.scope, target.workspace, message, operation_id).await
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
    operation_id: Option<String>,
) -> Result<()> {
    let result = request_turn(
        &RpcClient::new(paths.socket_path.clone()),
        scope,
        workspace,
        message,
        operation_id,
    )
    .await?;
    print_human(&serde_json::to_value(result)?);
    Ok(())
}

async fn run_jump(
    paths: &CocoPaths,
    selection: WorkspaceSelection,
    interaction: &mut dyn Interaction,
) -> Result<()> {
    let target =
        resolve_workspace_input(paths, selection, "Choose a workspace to open", interaction)
            .await?;
    jump_to_workspace(paths, target.scope, target.workspace).await
}

async fn request_turn(
    client: &RpcClient,
    scope: RepositoryScope,
    workspace: String,
    message: String,
    operation_id: Option<String>,
) -> Result<WorkspaceResult> {
    if message.trim().is_empty() {
        bail!("message must not be empty");
    }
    let operation_id = operation_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    client
        .request(TurnStartParams {
            scope,
            workspace,
            message,
            operation_id: operation_id.clone(),
        })
        .await
        .with_context(|| {
            format!(
                "turn operation ID: {operation_id}; reuse it with --operation-id only when retrying this exact send"
            )
        })
}

async fn jump_to_workspace(
    paths: &CocoPaths,
    scope: RepositoryScope,
    workspace: String,
) -> Result<()> {
    attach_and_jump(
        paths,
        &RpcClient::new(paths.socket_path.clone()),
        scope,
        workspace,
    )
    .await
}

async fn attach_and_jump(
    paths: &CocoPaths,
    client: &RpcClient,
    scope: RepositoryScope,
    workspace: String,
) -> Result<()> {
    let result = client
        .request(WorkspaceAttachParams { scope, workspace })
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

async fn run_diff(
    paths: &CocoPaths,
    selection: WorkspaceSelection,
    interaction: &mut dyn Interaction,
) -> Result<()> {
    let target = resolve_workspace_input(
        paths,
        selection,
        "Choose a workspace to inspect",
        interaction,
    )
    .await?;
    show_diff(paths, target.scope, target.workspace).await
}
