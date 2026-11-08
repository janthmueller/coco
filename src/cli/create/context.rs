use anyhow::Result;

use crate::protocol::{RepositoryScope, WorkspaceListItem, WorkspaceListParams};
use crate::rpc::RpcClient;

use super::super::args::WorkspaceSort;
use super::super::prompt::Interaction;
use super::super::workspace_picker;
use super::reusable_context_phases;

pub(super) async fn list_workspaces(client: &RpcClient) -> Result<Vec<WorkspaceListItem>> {
    Ok(client
        .request(WorkspaceListParams {
            scope: RepositoryScope::AllRepositories,
            phases: Some(reusable_context_phases()),
            include_resources: false,
            include_activity: false,
            include_clients: false,
        })
        .await?)
}

pub(super) fn select_workspace<'a>(
    interaction: &mut dyn Interaction,
    workspaces: &'a mut [WorkspaceListItem],
    sort: WorkspaceSort,
) -> Result<&'a WorkspaceListItem> {
    let choices = workspace_picker::choices(
        workspaces,
        sort,
        true,
        chrono::Utc::now().timestamp_millis(),
    );
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
