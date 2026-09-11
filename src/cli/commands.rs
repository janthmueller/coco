use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use uuid::Uuid;

use crate::paths::CocoPaths;
use crate::protocol::{
    ModelListParams, RepositoryListParams, RepositoryRegisterParams, RepositoryResolveParams,
    RepositoryScope, RepositorySummary, ResourcePolicyUpdate, TurnStartParams,
    WorkspaceAttachParams, WorkspaceBaseRequest, WorkspaceChangesRequest, WorkspaceCloseParams,
    WorkspaceContextRequest, WorkspaceContextSource, WorkspaceCreateParams, WorkspaceDeleteParams,
    WorkspaceDiffParams, WorkspaceGetParams, WorkspaceLimitsGetParams, WorkspaceLimitsResetParams,
    WorkspaceLimitsResult, WorkspaceLimitsSetParams, WorkspaceListItem, WorkspaceListParams,
    WorkspaceReopenParams, WorkspaceResourcePolicyPatch, WorkspaceResult, WorkspaceWorktreeRequest,
};
use crate::rpc::{RpcClient, RpcClientError};

use super::args::{
    Cli, CloseArgs, Command, CreateArgs, DeleteArgs, LimitField, LimitsCommand, LimitsSetArgs,
    LimitsTargetArgs, McpCommand, ModelCommand, RepoCommand, StatusArgs,
};
use super::decision::decide;
use super::jump::jump;
use super::output::{
    print_diff, print_json, print_model_list, print_repository_list, print_repository_registered,
    print_retirement_plan, print_source_changes_omitted_warning, print_status, print_turn_started,
    print_workspace_closed, print_workspace_created, print_workspace_deleted,
    print_workspace_limits, print_workspace_list, print_workspace_reopened, versioned,
    versioned_array,
};
use super::prompt::{Choice, Interaction, TerminalInteraction};
use super::status::{follow_status, follow_status_collection};

#[cfg(test)]
mod resources_tests;
#[cfg(test)]
mod retirement_tests;
use super::turn::wait_for_turn;

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
        Command::Signal { command } => {
            super::signals::run(command, &paths, repository_path, all_repos, global).await
        }
        Command::Mcp { command } => {
            reject_top_level_scope(has_explicit_scope, "mcp")?;
            run_mcp(command, paths).await
        }
        Command::Hook { command } => {
            reject_top_level_scope(has_explicit_scope, "hook")?;
            super::hooks::run(command, &paths).await
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
        command @ (Command::Close(_)
        | Command::Reopen(_)
        | Command::Delete(_)
        | Command::Send { .. }
        | Command::Jump { .. }
        | Command::Diff { .. }) => {
            run_targeted_scoped(
                command,
                &paths,
                &repository_path,
                has_scope_path,
                all_repos,
                global,
                interaction,
            )
            .await
        }
        Command::List { json, closed, .. } => {
            if global {
                bail!("coco list accepts --all-repos for an overview, not --global");
            }
            let scope = overview_scope(RepositoryScope::repository(repository_path), all_repos);
            list_workspaces(&paths, scope, json, closed).await
        }
        Command::Status(args) => run_status(&paths, repository_path, args, all_repos, global).await,
        Command::Usage(args) => {
            super::usage::run(&paths, repository_path, args, all_repos, global).await
        }
        Command::Limits { command } => {
            run_limits_scoped(
                command,
                &paths,
                &repository_path,
                has_scope_path,
                all_repos,
                global,
                interaction,
            )
            .await
        }
        Command::Decide { decision, choice } => {
            reject_top_level_scope(has_explicit_scope, "decide")?;
            decide(&paths, decision, choice, interaction).await
        }
    }
}

async fn run_limits_scoped(
    command: LimitsCommand,
    paths: &CocoPaths,
    repository_path: &Path,
    has_scope_path: bool,
    all_repos: bool,
    global: bool,
    interaction: &mut dyn Interaction,
) -> Result<()> {
    reject_all_repos_for_reference(all_repos, "limits")?;
    match command {
        LimitsCommand::Show(args) => {
            let target = resolve_limits_target(
                paths,
                repository_path,
                has_scope_path,
                global,
                &args,
                "inspect",
                interaction,
            )
            .await?;
            let result = RpcClient::new(paths.socket_path.clone())
                .request(WorkspaceLimitsGetParams {
                    scope: target.scope,
                    workspace: target.workspace,
                })
                .await?;
            print_limits_result(&result, args.json)
        }
        LimitsCommand::Set(args) => {
            let target_args = LimitsTargetArgs {
                workspace: args.workspace.clone(),
                global: args.global,
                json: args.json,
            };
            let target = resolve_limits_target(
                paths,
                repository_path,
                has_scope_path,
                global,
                &target_args,
                "configure",
                interaction,
            )
            .await?;
            let patch = limits_patch(&args)?;
            let result = RpcClient::new(paths.socket_path.clone())
                .request(WorkspaceLimitsSetParams {
                    scope: target.scope,
                    workspace: target.workspace,
                    patch,
                })
                .await?;
            print_limits_result(&result, args.json)
        }
        LimitsCommand::Reset(args) => {
            let target = resolve_limits_target(
                paths,
                repository_path,
                has_scope_path,
                global,
                &args,
                "reset",
                interaction,
            )
            .await?;
            let result = RpcClient::new(paths.socket_path.clone())
                .request(WorkspaceLimitsResetParams {
                    scope: target.scope,
                    workspace: target.workspace,
                })
                .await?;
            print_limits_result(&result, args.json)
        }
    }
}

async fn resolve_limits_target(
    paths: &CocoPaths,
    repository_path: &Path,
    has_scope_path: bool,
    global: bool,
    args: &LimitsTargetArgs,
    action: &str,
    interaction: &mut dyn Interaction,
) -> Result<ResolvedWorkspaceTarget> {
    resolve_workspace_input_with_phases(
        paths,
        workspace_selection(
            repository_path,
            has_scope_path,
            args.workspace.clone(),
            global,
        ),
        &format!("Choose a workspace to {action} limits"),
        Some(
            [
                "prepared",
                "active",
                "waiting_for_approval",
                "waiting_for_input",
                "idle",
                "not_loaded",
                "system_error",
                "unavailable",
                "completed",
                "failed",
                "closed",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        ),
        interaction,
    )
    .await
}

fn limits_patch(args: &LimitsSetArgs) -> Result<WorkspaceResourcePolicyPatch> {
    let clear = args.clear.iter().copied().collect::<HashSet<_>>();
    let patch = WorkspaceResourcePolicyPatch {
        memory_high_bytes: limit_update(
            args.memory_high,
            LimitField::MemoryHigh,
            &clear,
            "memory-high",
        )?,
        memory_max_bytes: limit_update(
            args.memory_max,
            LimitField::MemoryMax,
            &clear,
            "memory-max",
        )?,
        cpu_max_millicores: limit_update(args.cpu_max, LimitField::CpuMax, &clear, "cpu-max")?,
        cpu_weight: limit_update(args.cpu_weight, LimitField::CpuWeight, &clear, "cpu-weight")?,
        tasks_max: limit_update(args.tasks_max, LimitField::TasksMax, &clear, "tasks-max")?,
    };
    if patch.is_empty() {
        bail!("set at least one limit or use --clear <FIELD>");
    }
    Ok(patch)
}

fn limit_update<T>(
    value: Option<T>,
    field: LimitField,
    clear: &HashSet<LimitField>,
    name: &str,
) -> Result<Option<ResourcePolicyUpdate<T>>> {
    if value.is_some() && clear.contains(&field) {
        bail!("--{name} conflicts with --clear {name}");
    }
    Ok(match value {
        Some(value) => Some(ResourcePolicyUpdate::Set(value)),
        None if clear.contains(&field) => Some(ResourcePolicyUpdate::Clear),
        None => None,
    })
}

fn print_limits_result(result: &WorkspaceLimitsResult, json_output: bool) -> Result<()> {
    if json_output {
        print_json(versioned(serde_json::to_value(result)?))
    } else {
        print_workspace_limits(result);
        Ok(())
    }
}

async fn run_targeted_scoped(
    command: Command,
    paths: &CocoPaths,
    repository_path: &Path,
    has_scope_path: bool,
    all_repos: bool,
    global: bool,
    interaction: &mut dyn Interaction,
) -> Result<()> {
    match command {
        Command::Close(args) => {
            run_close_scoped(
                paths,
                repository_path,
                has_scope_path,
                all_repos,
                global,
                args,
                interaction,
            )
            .await
        }
        Command::Reopen(args) => {
            run_reopen_scoped(
                paths,
                repository_path,
                has_scope_path,
                all_repos,
                global,
                args,
                interaction,
            )
            .await
        }
        Command::Delete(args) => {
            run_delete_scoped(
                paths,
                repository_path,
                has_scope_path,
                all_repos,
                global,
                args,
                interaction,
            )
            .await
        }
        Command::Send {
            workspace,
            message,
            operation_id,
            wait,
            ..
        } => {
            run_send_scoped(
                paths,
                repository_path,
                has_scope_path,
                all_repos,
                global,
                workspace,
                message,
                operation_id,
                wait,
                interaction,
            )
            .await
        }
        Command::Jump { workspace, .. } => {
            run_jump_scoped(
                paths,
                repository_path,
                has_scope_path,
                all_repos,
                global,
                workspace,
                interaction,
            )
            .await
        }
        Command::Diff { workspace, .. } => {
            run_diff_scoped(
                paths,
                repository_path,
                has_scope_path,
                all_repos,
                global,
                workspace,
                interaction,
            )
            .await
        }
        _ => unreachable!("non-targeted command routed to targeted dispatcher"),
    }
}

async fn run_close_scoped(
    paths: &CocoPaths,
    repository_path: &Path,
    has_scope_path: bool,
    all_repos: bool,
    global: bool,
    args: CloseArgs,
    interaction: &mut dyn Interaction,
) -> Result<()> {
    reject_all_repos_for_reference(all_repos, "close")?;
    let selection = workspace_selection(
        repository_path,
        has_scope_path,
        args.workspace.clone(),
        global,
    );
    run_close(paths, selection, args, interaction).await
}

async fn run_reopen_scoped(
    paths: &CocoPaths,
    repository_path: &Path,
    has_scope_path: bool,
    all_repos: bool,
    global: bool,
    args: super::args::ReopenArgs,
    interaction: &mut dyn Interaction,
) -> Result<()> {
    reject_all_repos_for_reference(all_repos, "reopen")?;
    let selection = workspace_selection(repository_path, has_scope_path, args.workspace, global);
    run_reopen(paths, selection, interaction).await
}

async fn run_delete_scoped(
    paths: &CocoPaths,
    repository_path: &Path,
    has_scope_path: bool,
    all_repos: bool,
    global: bool,
    args: DeleteArgs,
    interaction: &mut dyn Interaction,
) -> Result<()> {
    reject_all_repos_for_reference(all_repos, "delete")?;
    let selection = workspace_selection(
        repository_path,
        has_scope_path,
        args.workspace.clone(),
        global,
    );
    run_delete(paths, selection, args, interaction).await
}

#[expect(
    clippy::too_many_arguments,
    reason = "the scoped send adapter keeps each independent CLI input explicit"
)]
async fn run_send_scoped(
    paths: &CocoPaths,
    repository_path: &Path,
    has_scope_path: bool,
    all_repos: bool,
    global: bool,
    workspace: Option<String>,
    message: Option<String>,
    operation_id: Option<String>,
    wait: bool,
    interaction: &mut dyn Interaction,
) -> Result<()> {
    reject_all_repos_for_reference(all_repos, "send")?;
    let selection = workspace_selection(repository_path, has_scope_path, workspace, global);
    run_send(paths, selection, message, operation_id, wait, interaction).await
}

async fn run_jump_scoped(
    paths: &CocoPaths,
    repository_path: &Path,
    has_scope_path: bool,
    all_repos: bool,
    global: bool,
    workspace: Option<String>,
    interaction: &mut dyn Interaction,
) -> Result<()> {
    reject_all_repos_for_reference(all_repos, "jump")?;
    let selection = workspace_selection(repository_path, has_scope_path, workspace, global);
    run_jump(paths, selection, interaction).await
}

async fn run_diff_scoped(
    paths: &CocoPaths,
    repository_path: &Path,
    has_scope_path: bool,
    all_repos: bool,
    global: bool,
    workspace: Option<String>,
    interaction: &mut dyn Interaction,
) -> Result<()> {
    reject_all_repos_for_reference(all_repos, "diff")?;
    let selection = workspace_selection(repository_path, has_scope_path, workspace, global);
    run_diff(paths, selection, interaction).await
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
    if json_output {
        print_json(versioned_array("models", serde_json::to_value(models)?))
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

pub(super) fn overview_scope(scope: RepositoryScope, all_repos: bool) -> RepositoryScope {
    if all_repos {
        RepositoryScope::AllRepositories
    } else {
        scope
    }
}

pub(super) fn scope_for_reference(
    scope: RepositoryScope,
    reference: &str,
    global: bool,
) -> RepositoryScope {
    if global || Uuid::parse_str(reference).is_ok() {
        RepositoryScope::AllRepositories
    } else {
        scope
    }
}

pub(super) fn reject_all_repos_for_reference(all_repos: bool, command: &str) -> Result<()> {
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
    verified: bool,
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

fn is_dirty_source(error: &RpcClientError) -> bool {
    matches!(
        error,
        RpcClientError::Remote(payload) if payload.code == "DIRTY_SOURCE"
    )
}

fn dirty_source_path(error: &RpcClientError) -> Option<&str> {
    let RpcClientError::Remote(payload) = error else {
        return None;
    };
    payload.data.as_ref()?.get("repositoryPath")?.as_str()
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
    resolve_workspace_input_with_phases(paths, selection, title, None, interaction).await
}

async fn resolve_workspace_input_with_phases(
    paths: &CocoPaths,
    selection: WorkspaceSelection,
    title: &str,
    phases: Option<Vec<String>>,
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
            verified: false,
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
            phases,
            include_resources: false,
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
    let choices = workspaces
        .iter()
        .map(|workspace| workspace_choice(workspace, global))
        .collect::<Vec<_>>();
    let selected = interaction.select(title, &choices)?;
    Ok(ResolvedWorkspaceTarget {
        scope: RepositoryScope::AllRepositories,
        workspace: workspaces[selected].workspace.id.clone(),
        verified: true,
    })
}

fn workspace_choice(item: &WorkspaceListItem, include_repository: bool) -> Choice {
    let branch = item.workspace.branch_name.as_deref().unwrap_or("detached");
    let mut detail = Vec::with_capacity(3);
    if include_repository {
        detail.push(item.repository.root_path.display().to_string());
    }
    detail.push(super::output::phase_label(item.workspace.phase.as_str()).to_owned());
    detail.push(branch.to_owned());
    Choice::new(item.workspace.name.clone(), Some(detail.join(" · ")))
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
        allowed_signals,
        signal_catalog,
    } = command;
    crate::mcp::serve(
        repository,
        allow_send,
        paths.socket_path,
        allowed_signals,
        signal_catalog,
    )
    .await
}

async fn run_repo(command: RepoCommand, paths: &CocoPaths, cwd: &Path) -> Result<()> {
    let client = RpcClient::new(paths.socket_path.clone());
    match command {
        RepoCommand::Add { path } => {
            let path = resolve_repository_path(cwd, Some(path));
            let result = client.request(RepositoryRegisterParams { path }).await?;
            print_repository_registered(&result);
            Ok(())
        }
        RepoCommand::List { json } => {
            let result = client.request(RepositoryListParams {}).await?;
            if json {
                print_json(versioned_array(
                    "repositories",
                    serde_json::to_value(result)?,
                ))
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
    let (mut params, initial_message, should_jump) =
        normalize_create_args(cwd, args, Uuid::new_v4().to_string())?;
    let name = params.name.clone();
    let repository_path = params.repository_path.clone();
    let client = RpcClient::new(paths.socket_path.clone());
    let mut result: WorkspaceResult = match client.request(params.clone()).await {
        Ok(result) => result,
        Err(error)
            if params.changes == WorkspaceChangesRequest::Reject && is_dirty_source(&error) =>
        {
            let path = dirty_source_path(&error)
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| repository_path.display().to_string());
            print_source_changes_omitted_warning(&path);
            params.changes = WorkspaceChangesRequest::Ignore;
            client.request(params).await?
        }
        Err(error) => return Err(error.into()),
    };
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
    print_workspace_created(&result);
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
        context,
        fork_from,
        compact_context,
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
                compact: compact_context,
            },
        )
    } else {
        let base = match base_workspace {
            Some(workspace) => WorkspaceBaseRequest::Workspace { workspace },
            None => WorkspaceBaseRequest::Revision {
                revision: base.unwrap_or_else(|| "HEAD".to_owned()),
            },
        };
        let context = match context {
            Some(reference) => WorkspaceContextRequest::Fork {
                source: WorkspaceContextSource::Reference { reference },
                compact: compact_context,
            },
            None if compact_context => {
                bail!("--compact-context requires --context or --fork-from")
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
    closed: bool,
) -> Result<()> {
    let include_repository = matches!(scope, RepositoryScope::AllRepositories);
    let result = RpcClient::new(paths.socket_path.clone())
        .request(WorkspaceListParams {
            scope,
            phases: closed.then(|| vec!["closed".to_owned()]),
            include_resources: false,
        })
        .await?;
    if json_output {
        print_json(versioned_array("workspaces", serde_json::to_value(result)?))
    } else {
        print_workspace_list(&result, include_repository, false);
        Ok(())
    }
}

async fn run_status(
    paths: &CocoPaths,
    repository_path: PathBuf,
    args: StatusArgs,
    all_repos: bool,
    global: bool,
) -> Result<()> {
    let repository_scope = RepositoryScope::repository(repository_path);
    if let Some(workspace) = args.workspace {
        if all_repos {
            bail!(
                "coco status WORKSPACE targets one workspace; use --global to resolve its name across repositories"
            );
        }
        let scope = scope_for_reference(repository_scope, &workspace, global);
        return show_status(
            paths,
            scope,
            workspace,
            args.follow,
            args.resources,
            args.json,
        )
        .await;
    }
    if global {
        bail!(
            "coco status --global requires a workspace name or ID; use --all-repos for an overview"
        );
    }
    let scope = overview_scope(repository_scope, all_repos);
    if args.follow {
        follow_status_collection(
            &RpcClient::new(paths.socket_path.clone()),
            scope,
            args.resources,
        )
        .await
    } else {
        show_status_collection(paths, scope, args.resources, args.json).await
    }
}

async fn show_status_collection(
    paths: &CocoPaths,
    scope: RepositoryScope,
    resources: bool,
    json_output: bool,
) -> Result<()> {
    let include_repository = matches!(scope, RepositoryScope::AllRepositories);
    let result = RpcClient::new(paths.socket_path.clone())
        .request(WorkspaceListParams {
            scope,
            phases: None,
            include_resources: resources || json_output,
        })
        .await?;
    if json_output {
        print_json(versioned_array("workspaces", serde_json::to_value(result)?))
    } else {
        print_workspace_list(&result, include_repository, resources);
        Ok(())
    }
}

async fn run_close(
    paths: &CocoPaths,
    selection: WorkspaceSelection,
    args: CloseArgs,
    interaction: &mut dyn Interaction,
) -> Result<()> {
    let target =
        resolve_workspace_input(paths, selection, "Choose a workspace to close", interaction)
            .await?;
    let client = RpcClient::new(paths.socket_path.clone());
    let mut discard_changes = args.discard_changes;
    let mut preview = client
        .request(WorkspaceCloseParams {
            scope: target.scope.clone(),
            workspace: target.workspace.clone(),
            archive_thread: args.archive_thread,
            discard_changes,
            dry_run: true,
            expected_plan: None,
        })
        .await?;
    if args.dry_run {
        print_retirement_plan("Close", &preview.plan);
        return Ok(());
    }

    if preview.plan.has_local_changes() && !discard_changes {
        if args.yes || !interaction.is_interactive() {
            print_retirement_plan("Close", &preview.plan);
        }
        if args.yes {
            bail!(
                "--yes does not discard local changes; add --discard-changes after reviewing them"
            );
        }
        require_interactive(interaction, "confirmation")?;
        discard_changes = true;
        preview = client
            .request(WorkspaceCloseParams {
                scope: target.scope.clone(),
                workspace: preview.plan.workspace_id.clone(),
                archive_thread: args.archive_thread,
                discard_changes,
                dry_run: true,
                expected_plan: None,
            })
            .await?;
    }
    if !preview.plan.blockers.is_empty() {
        print_retirement_plan("Close", &preview.plan);
        bail!("workspace cannot be closed while the listed blockers remain");
    }
    if discard_changes && preview.plan.has_local_changes() && !args.yes {
        print_retirement_plan("Close", &preview.plan);
        require_interactive(interaction, "confirmation")?;
        if !interaction.confirm("Discard every listed local change and close this workspace?")? {
            bail!("workspace close cancelled");
        }
    }
    let result = client
        .request(WorkspaceCloseParams {
            scope: target.scope,
            workspace: preview.plan.workspace_id.clone(),
            archive_thread: args.archive_thread,
            discard_changes,
            dry_run: false,
            expected_plan: Some(preview.plan),
        })
        .await?;
    print_workspace_closed(&result);
    Ok(())
}

async fn run_reopen(
    paths: &CocoPaths,
    selection: WorkspaceSelection,
    interaction: &mut dyn Interaction,
) -> Result<()> {
    let target = resolve_workspace_input_with_phases(
        paths,
        selection,
        "Choose a workspace to reopen",
        Some(vec!["closed".to_owned()]),
        interaction,
    )
    .await?;
    let result = RpcClient::new(paths.socket_path.clone())
        .request(WorkspaceReopenParams {
            scope: target.scope,
            workspace: target.workspace,
        })
        .await?;
    print_workspace_reopened(&result);
    Ok(())
}

async fn run_delete(
    paths: &CocoPaths,
    selection: WorkspaceSelection,
    args: DeleteArgs,
    interaction: &mut dyn Interaction,
) -> Result<()> {
    let target = resolve_workspace_input_with_phases(
        paths,
        selection,
        "Choose a workspace to delete",
        Some(
            [
                "prepared",
                "active",
                "waiting_for_approval",
                "waiting_for_input",
                "idle",
                "not_loaded",
                "system_error",
                "unavailable",
                "provisioning",
                "starting",
                "completed",
                "failed",
                "closed",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        ),
        interaction,
    )
    .await?;
    let client = RpcClient::new(paths.socket_path.clone());
    let mut params = WorkspaceDeleteParams {
        scope: target.scope,
        workspace: target.workspace,
        delete_thread: !args.keep_thread,
        delete_branch: !args.keep_branch,
        discard_changes: args.discard_changes,
        discard_unretained_commits: args.discard_unretained_commits,
        dry_run: true,
        expected_plan: None,
    };
    let mut preview = client.request(params.clone()).await?;
    if args.dry_run {
        print_retirement_plan("Delete", &preview.plan);
        return Ok(());
    }
    params.workspace = preview.plan.workspace_id.clone();
    // Only the explicit final discard question may expand loss policy in an
    // interactive run. --yes merely skips that question for supplied policies.
    let needs_discard = (preview.plan.has_local_changes() && !params.discard_changes)
        || (preview.plan.unretained_commit_count > 0 && !params.discard_unretained_commits);
    if needs_discard && !args.yes && interaction.is_interactive() {
        params.discard_changes |= preview.plan.has_local_changes();
        params.discard_unretained_commits |= preview.plan.unretained_commit_count > 0;
        preview = client.request(params.clone()).await?;
    }
    print_retirement_plan("Delete", &preview.plan);
    if !preview.plan.blockers.is_empty() {
        bail!(
            "workspace cannot be deleted while the listed blockers remain; --yes does not authorize discarding changes or commits"
        );
    }
    if !args.yes {
        require_interactive(interaction, "confirmation")?;
        let question = deletion_confirmation(&preview.plan);
        if !interaction.confirm(question)? {
            bail!("workspace deletion cancelled");
        }
    }
    params.workspace = preview.plan.workspace_id.clone();
    params.dry_run = false;
    params.expected_plan = Some(preview.plan);
    let result = client.request(params).await?;
    print_workspace_deleted(&result);
    Ok(())
}

fn deletion_confirmation(plan: &crate::protocol::WorkspaceRetirementPlan) -> &'static str {
    match (plan.has_local_changes(), plan.unretained_commit_count > 0) {
        (true, true) => {
            "Discard the listed local changes and unretained commits, and permanently delete this workspace?"
        }
        (true, false) => "Discard the listed local changes and permanently delete this workspace?",
        (false, true) => {
            "Discard the listed unretained commits and permanently delete this workspace?"
        }
        (false, false) => "Permanently apply this deletion plan?",
    }
}

async fn run_send(
    paths: &CocoPaths,
    selection: WorkspaceSelection,
    message: Option<String>,
    operation_id: Option<String>,
    wait: bool,
    interaction: &mut dyn Interaction,
) -> Result<()> {
    let mut target = resolve_workspace_input(
        paths,
        selection,
        "Choose a workspace to send to",
        interaction,
    )
    .await?;
    if message.is_none() && !target.verified {
        let result = RpcClient::new(paths.socket_path.clone())
            .request(WorkspaceGetParams {
                scope: target.scope.clone(),
                workspace: target.workspace.clone(),
                include_resources: false,
            })
            .await?;
        target.workspace = result.workspace.id;
        target.verified = true;
    }
    let message = resolve_text_input(message, "message", "Message", interaction)?;
    send(
        paths,
        target.scope,
        target.workspace,
        message,
        operation_id,
        wait,
    )
    .await
}

async fn show_status(
    paths: &CocoPaths,
    scope: RepositoryScope,
    workspace: String,
    follow: bool,
    resources: bool,
    json_output: bool,
) -> Result<()> {
    let client = RpcClient::new(paths.socket_path.clone());
    if follow {
        return follow_status(&client, scope, &workspace, resources).await;
    }
    let result = client
        .request(WorkspaceGetParams {
            scope,
            workspace,
            include_resources: resources || json_output,
        })
        .await?;
    if json_output {
        print_json(versioned(serde_json::to_value(result)?))
    } else {
        print_status(&result, resources);
        Ok(())
    }
}

async fn send(
    paths: &CocoPaths,
    scope: RepositoryScope,
    workspace: String,
    message: String,
    operation_id: Option<String>,
    wait: bool,
) -> Result<()> {
    let client = RpcClient::new(paths.socket_path.clone());
    let operation_id = operation_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let result = request_turn(
        &client,
        scope,
        workspace,
        message,
        Some(operation_id.clone()),
    )
    .await?;
    if wait {
        wait_for_turn(&client, &operation_id).await
    } else {
        print_turn_started(&result);
        Ok(())
    }
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
    jump(paths, client, result).await
}

async fn show_diff(paths: &CocoPaths, scope: RepositoryScope, workspace: String) -> Result<()> {
    let result = RpcClient::new(paths.socket_path.clone())
        .request(WorkspaceDiffParams {
            scope,
            workspace,
            max_bytes: None,
        })
        .await?;
    print_diff(&result);
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
