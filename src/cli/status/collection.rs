use anyhow::Result;

use crate::protocol::{
    AccountQuotaResult, RepositoryScope, WorkspaceListItem, WorkspaceListParams,
    WorkspaceUsageGetParams, WorkspaceUsageItem,
};
use crate::rpc::{RpcClient, RpcClientError};

use super::filter::{StateFilter, StateSelector};
use super::sorting::{SortKey, SortPlan};
use super::{StatusProjection, account_quota, workspace_usage_collection};

#[derive(Debug)]
pub(in crate::cli) struct CollectionQuery {
    sort: SortPlan,
    filter: StateFilter,
}

pub(in crate::cli) struct CollectionSnapshot {
    pub(in crate::cli) workspaces: Vec<WorkspaceListItem>,
    pub(in crate::cli) usage: Option<Vec<WorkspaceUsageItem>>,
    pub(in crate::cli) quota: Option<AccountQuotaResult>,
}

impl CollectionQuery {
    pub(in crate::cli) fn new(
        keys: &[SortKey],
        include: &[StateSelector],
        exclude: &[StateSelector],
        projection: StatusProjection,
    ) -> Result<Self> {
        Ok(Self {
            sort: SortPlan::new(keys, projection.resources, projection.usage)?,
            filter: StateFilter::new(include, exclude)?,
        })
    }

    pub(in crate::cli) fn filtered(&self) -> bool {
        self.filter.is_active()
    }

    pub(in crate::cli) async fn read(
        &self,
        client: &RpcClient,
        scope: RepositoryScope,
        projection: StatusProjection,
        json: bool,
    ) -> Result<CollectionSnapshot> {
        let mut workspaces = client
            .request(WorkspaceListParams {
                scope: scope.clone(),
                phases: self.filter.phases(),
                include_resources: projection.resources || json,
                include_activity: true,
                include_clients: projection.clients || json,
            })
            .await?;
        self.filter.apply(&mut workspaces);
        let usage = if self.filtered() || workspaces.is_empty() {
            selected_usage(client, &workspaces, projection.usage).await?
        } else {
            workspace_usage_collection(client, scope, projection.usage).await?
        };
        self.sort.apply(&mut workspaces, usage.as_deref());
        let quota = account_quota(client, projection.quota).await?;
        Ok(CollectionSnapshot {
            workspaces,
            usage,
            quota,
        })
    }
}

async fn selected_usage(
    client: &RpcClient,
    rows: &[WorkspaceListItem],
    enabled: bool,
) -> Result<Option<Vec<WorkspaceUsageItem>>> {
    if !enabled {
        return Ok(None);
    }
    let mut usage = Vec::with_capacity(rows.len());
    for row in rows {
        match client
            .request(WorkspaceUsageGetParams {
                scope: RepositoryScope::AllRepositories,
                workspace: row.workspace.id.clone(),
            })
            .await
        {
            Ok(item) => usage.push(item),
            // A concurrent delete is reflected by the next collection refresh;
            // never replace live state with the usage endpoint's stored copy.
            Err(RpcClientError::Remote(error)) if error.code == "WORKSPACE_NOT_FOUND" => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(Some(usage))
}
