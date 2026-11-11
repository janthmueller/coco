use super::*;
use crate::domain::WorkspaceAvailability;
use crate::domain::retirement::WorkspaceDeletionState;
use crate::protocol::{
    WorkspaceCloseParams, WorkspaceCloseResult, WorkspaceDeleteOperationParams,
    WorkspaceDeleteRequest, WorkspaceDeletionGetParams, WorkspaceDeletionReceipt,
    WorkspaceReopenParams, WorkspaceRetirementPlan,
};

mod dispatch;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeleteInput {
    /// Caller-selected key; reuse only for identical retries, even after deletion.
    operation_id: String,
    /// Workspace name or ID in this server's repository.
    workspace: String,
    #[serde(default)]
    keep_thread: bool,
    #[serde(default)]
    keep_branch: bool,
    /// Explicitly permit loss of tracked, untracked and ignored local files.
    #[serde(default)]
    discard_changes: bool,
    /// Explicitly permit commits to lose their last retaining Git reference.
    #[serde(default)]
    discard_unretained_commits: bool,
    /// Return a checked preview without reserving the key or deleting anything.
    #[serde(default)]
    dry_run: bool,
    /// Optional planSha256 from a prior preview; changed plans are rejected.
    expected_plan_sha256: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CloseInput {
    workspace: String,
    #[serde(default)]
    archive_thread: bool,
    #[serde(default)]
    discard_changes: bool,
    #[serde(default)]
    dry_run: bool,
    expected_plan_sha256: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeletionGetInput {
    operation_id: String,
}

#[tool_router(router = lifecycle_tool_router, vis = "pub(super)")]
impl McpServer {
    #[tool(
        name = "workspaces.delete",
        description = "Permanently delete an open or closed workspace and, by default, its thread and CoCo-created branch. Requires --allow-delete. Dirty files and unretained commits block unless their separate discard policies are explicit. dryRun returns a checked preview without claiming operationId. Reuse identical operationId after lost replies; inspect workspaceDeletions.get, never retry an uncertain deletion under a new key. Keeping the thread does not retain CoCo turn receipts.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    async fn workspaces_delete(
        &self,
        Parameters(mut input): Parameters<DeleteInput>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        if !self.confirm_changes || input.dry_run {
            return result(
                self.dispatcher
                    .delete_operation(&input, input.dry_run)
                    .await,
            );
        }
        let preview = match self.dispatcher.delete_operation(&input, true).await {
            Ok(preview) => preview,
            Err(error) => return failure(error),
        };
        if preview.state != WorkspaceDeletionState::Preview {
            return result(Ok(preview));
        }
        let Some(plan) = &preview.plan else {
            return failure(DaemonFailure::new(
                "INTERNAL",
                "Deletion preview is unavailable",
            ));
        };
        let digest = match checked_plan(plan, input.expected_plan_sha256.as_deref()) {
            Ok(digest) => digest,
            Err(error) => return failure(error),
        };
        let review = super::confirmation::retirement_review(
            "Delete",
            &self.dispatcher.repository,
            plan,
            input.discard_changes,
            input.discard_unretained_commits,
        );
        if let Err(error) = self.confirm_change(review, &context).await {
            return error;
        }
        input.expected_plan_sha256 = Some(digest);
        result(self.dispatcher.delete_operation(&input, false).await)
    }

    #[tool(
        name = "workspaceDeletions.get",
        description = "Read a retained deletion receipt by its original operationId in this repository, including after workspace deletion or daemon restart. No native/model call. unknown is not proof that no effects occurred; uncertain/preparing never grants redispatch.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn workspace_deletions_get(
        &self,
        Parameters(input): Parameters<DeletionGetInput>,
    ) -> CallToolResult {
        self.dispatcher
            .call(
                "workspaceDeletions.get",
                WorkspaceDeletionGetParams {
                    repository_path: self.dispatcher.repository.clone(),
                    operation_id: input.operation_id.clone(),
                },
                None,
                Some(input.operation_id),
            )
            .await
    }

    #[tool(
        name = "workspaces.close",
        description = "Remove a checked workspace worktree while retaining its identity, branch, conversation and turn receipts for reopen. Requires --allow-close; blocks active work, attached TUIs, unretained detached commits and local files without explicit discardChanges. dryRun returns a plan and planSha256; expectedPlanSha256 can pin it. State-sensitive, not an operation-keyed retry API.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn workspaces_close(
        &self,
        Parameters(input): Parameters<CloseInput>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        let preview = match self.dispatcher.close_preview(&input).await {
            Ok(preview) => preview,
            Err(error) => return failure(error),
        };
        if input.dry_run {
            return close_result(Ok(preview));
        }
        if let Err(error) = checked_plan(&preview.plan, input.expected_plan_sha256.as_deref()) {
            return failure(error);
        }
        let review = super::confirmation::retirement_review(
            "Close",
            &self.dispatcher.repository,
            &preview.plan,
            input.discard_changes,
            false,
        );
        if let Err(error) = self.confirm_change(review, &context).await {
            return error;
        }
        close_result(self.dispatcher.close_apply(&input, preview.plan).await)
    }

    #[tool(
        name = "workspaces.reopen",
        description = "Restore a closed workspace's exact checked worktree and retained conversation binding without starting a turn. Requires --allow-reopen. State-sensitive: inspect status after a lost response; an already open workspace is not reopened again.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn workspaces_reopen(
        &self,
        Parameters(input): Parameters<WorkspaceStatusInput>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        let status = match self
            .dispatcher
            .lifecycle_request(
                "workspaces.reopen.preview",
                WorkspaceGetParams {
                    scope: RepositoryScope::repository(self.dispatcher.repository.clone()),
                    workspace: input.workspace.clone(),
                    include_clients: false,
                    include_resources: false,
                },
                Some(&input.workspace),
                None,
            )
            .await
        {
            Ok(status) => status,
            Err(error) => return failure(error),
        };
        if status.workspace.availability != WorkspaceAvailability::Closed {
            return failure(DaemonFailure::new(
                "INVALID_WORKSPACE_STATE",
                "Workspace must be closed before reopening",
            ));
        }
        let review =
            super::confirmation::reopen_review(&self.dispatcher.repository, &status.workspace);
        if let Err(error) = self.confirm_change(review, &context).await {
            return error;
        }
        result(
            self.dispatcher
                .lifecycle_request(
                    "workspaces.reopen",
                    WorkspaceReopenParams {
                        scope: RepositoryScope::repository(self.dispatcher.repository.clone()),
                        workspace: status.workspace.id.clone(),
                    },
                    Some(&status.workspace.id),
                    None,
                )
                .await,
        )
    }
}

fn checked_plan(
    plan: &WorkspaceRetirementPlan,
    expected: Option<&str>,
) -> Result<String, DaemonFailure> {
    let digest = plan
        .sha256()
        .map_err(|_| DaemonFailure::new("INTERNAL", "Could not encode retirement plan"))?;
    if expected.is_some_and(|expected| expected != digest) || !plan.blockers.is_empty() {
        return Err(DaemonFailure::new(
            "WORKSPACE_RETIREMENT_BLOCKED",
            "Workspace retirement is blocked or the reviewed plan changed; inspect a fresh dryRun",
        ));
    }
    Ok(digest)
}

fn failure(error: DaemonFailure) -> CallToolResult {
    CallToolResult::structured_error(error.as_json())
}

fn result<T: serde::Serialize>(response: Result<T, DaemonFailure>) -> CallToolResult {
    match response.and_then(|response| {
        serde_json::to_value(response)
            .map_err(|_| DaemonFailure::new("INTERNAL", "Could not encode lifecycle result"))
    }) {
        Ok(value) => CallToolResult::structured(value),
        Err(error) => failure(error),
    }
}

fn close_result(response: Result<WorkspaceCloseResult, DaemonFailure>) -> CallToolResult {
    result(response.and_then(|response| {
        let digest = response
            .plan
            .sha256()
            .map_err(|_| DaemonFailure::new("INTERNAL", "Could not encode retirement plan"))?;
        let mut value = serde_json::to_value(response)
            .map_err(|_| DaemonFailure::new("INTERNAL", "Could not encode lifecycle result"))?;
        value["planSha256"] = json!(digest);
        Ok(value)
    }))
}

#[cfg(test)]
mod tests;
