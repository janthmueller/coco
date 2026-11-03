use std::path::Path;

use anyhow::Result;

use crate::protocol::{
    RepositoryResolveParams, RepositoryScope, WorkspaceListItem, WorkspaceListParams,
};
use crate::rpc::{RpcClient, RpcClientError};

use super::super::output::phase_label;
use super::super::prompt::{Choice, Interaction};
use super::super::status::sort_workspace_collection;
use super::reusable_context_phases;

pub(super) async fn list_workspaces(
    client: &RpcClient,
    repository_path: &Path,
) -> Result<Vec<WorkspaceListItem>> {
    // Resolve identity through the daemon: the selected path can be a nested
    // directory or another worktree, not the repository's displayed root.
    let current_repository = match client
        .request(RepositoryResolveParams {
            path: repository_path.to_path_buf(),
        })
        .await
    {
        Ok(repository) => Some(repository.id),
        Err(RpcClientError::Remote(error)) if error.code == "REPOSITORY_NOT_REGISTERED" => None,
        Err(error) => return Err(error.into()),
    };
    let mut workspaces = client
        .request(WorkspaceListParams {
            scope: RepositoryScope::AllRepositories,
            phases: Some(reusable_context_phases()),
            include_resources: false,
            include_activity: false,
            include_clients: false,
        })
        .await?;
    sort_workspace_collection(&mut workspaces);
    // Stable partition: preserve natural repository/name ordering inside both
    // the current-repository group and the remaining repositories.
    workspaces.sort_by_key(|item| current_repository.as_deref() != Some(&item.repository.id));
    Ok(workspaces)
}

pub(super) fn select_workspace<'a>(
    interaction: &mut dyn Interaction,
    workspaces: &'a [WorkspaceListItem],
) -> Result<&'a WorkspaceListItem> {
    let choices = workspaces
        .iter()
        .map(|item| {
            Choice::new(
                item.workspace.name.clone(),
                Some(format!(
                    "{} · {}",
                    item.repository.root_path.display(),
                    phase_label(item.workspace.phase.as_str())
                )),
            )
        })
        .collect::<Vec<_>>();
    let selected = interaction.select("Context from workspace", &choices)?;
    Ok(&workspaces[selected])
}

pub(super) fn source_label(item: &WorkspaceListItem) -> String {
    format!(
        "workspace {} · {}",
        item.workspace.name,
        item.repository.root_path.display()
    )
}
