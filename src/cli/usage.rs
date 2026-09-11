use std::io;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Result, bail};

use crate::paths::CocoPaths;
use crate::protocol::{RepositoryScope, WorkspaceUsageGetParams, WorkspaceUsageListParams};
use crate::rpc::RpcClient;

use super::args::UsageArgs;
use super::commands::{overview_scope, scope_for_reference};
use super::follow::{FollowOutput, stdout_supports_live_updates, wait_or_interrupt};
use super::output::{
    print_json, print_workspace_usage, print_workspace_usage_list,
    render_workspace_usage_for_stdout, render_workspace_usage_list_for_stdout, versioned,
    versioned_array,
};

const USAGE_POLL_INTERVAL: Duration = Duration::from_secs(1);

pub(super) async fn run(
    paths: &CocoPaths,
    repository_path: PathBuf,
    args: UsageArgs,
    all_repos: bool,
    global: bool,
) -> Result<()> {
    let repository_scope = RepositoryScope::repository(repository_path);
    if let Some(workspace) = args.workspace {
        if all_repos {
            bail!(
                "coco usage WORKSPACE targets one workspace; use --global to resolve its name across repositories"
            );
        }
        let scope = scope_for_reference(repository_scope, &workspace, global);
        if args.follow {
            return follow_workspace(&RpcClient::new(paths.socket_path.clone()), scope, workspace)
                .await;
        }
        let result = RpcClient::new(paths.socket_path.clone())
            .request(WorkspaceUsageGetParams { scope, workspace })
            .await?;
        return if args.json {
            print_json(versioned(serde_json::to_value(result)?))
        } else {
            print_workspace_usage(&result);
            Ok(())
        };
    }
    if global {
        bail!(
            "coco usage --global requires a workspace name or ID; use --all-repos for an overview"
        );
    }
    let scope = overview_scope(repository_scope, all_repos);
    if args.follow {
        follow_collection(&RpcClient::new(paths.socket_path.clone()), scope).await
    } else {
        let include_repository = matches!(scope, RepositoryScope::AllRepositories);
        let result = RpcClient::new(paths.socket_path.clone())
            .request(WorkspaceUsageListParams { scope })
            .await?;
        if args.json {
            print_json(versioned_array("workspaces", serde_json::to_value(result)?))
        } else {
            print_workspace_usage_list(&result, include_repository);
            Ok(())
        }
    }
}

async fn follow_workspace(
    client: &RpcClient,
    scope: RepositoryScope,
    workspace: String,
) -> Result<()> {
    let interactive = stdout_supports_live_updates();
    let mut output = FollowOutput::new(io::stdout(), interactive)?;
    let mut previous: Option<String> = None;
    loop {
        let result = client
            .request(WorkspaceUsageGetParams {
                scope: scope.clone(),
                workspace: workspace.clone(),
            })
            .await?;
        let frame = render_workspace_usage_for_stdout(&result);
        if previous.as_deref() != Some(&frame) {
            output.write_frame(&frame)?;
            previous = Some(frame);
        }
        if wait_or_interrupt(USAGE_POLL_INTERVAL).await {
            return Ok(());
        }
    }
}

async fn follow_collection(client: &RpcClient, scope: RepositoryScope) -> Result<()> {
    let include_repository = matches!(scope, RepositoryScope::AllRepositories);
    let interactive = stdout_supports_live_updates();
    let mut output = FollowOutput::new(io::stdout(), interactive)?;
    let mut previous: Option<String> = None;
    loop {
        let result = client
            .request(WorkspaceUsageListParams {
                scope: scope.clone(),
            })
            .await?;
        let frame = render_workspace_usage_list_for_stdout(&result, include_repository);
        if previous.as_deref() != Some(&frame) {
            output.write_frame(&frame)?;
            previous = Some(frame);
        }
        if wait_or_interrupt(USAGE_POLL_INTERVAL).await {
            return Ok(());
        }
    }
}
