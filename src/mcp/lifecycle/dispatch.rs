use super::*;

impl<C: DaemonRpc> Dispatcher<C> {
    pub(super) async fn lifecycle_request<R>(
        &self,
        action: &'static str,
        request: R,
        workspace: Option<&str>,
        operation: Option<&str>,
    ) -> Result<R::Response, DaemonFailure>
    where
        R: DaemonRequest + Send,
        R::Response: Send,
    {
        let response = self.client.request(request).await;
        let audit = response.as_ref().map(|_| Value::Null).map_err(Clone::clone);
        self.audit(action, workspace, operation, &audit).await;
        response
    }

    pub(super) async fn delete_operation(
        &self,
        input: &DeleteInput,
        dry_run: bool,
    ) -> Result<WorkspaceDeletionReceipt, DaemonFailure> {
        self.lifecycle_request(
            if dry_run {
                "workspaces.delete.preview"
            } else {
                "workspaces.delete"
            },
            WorkspaceDeleteOperationParams {
                repository_path: self.repository.clone(),
                request: WorkspaceDeleteRequest {
                    operation_id: input.operation_id.clone(),
                    workspace: input.workspace.clone(),
                    keep_thread: input.keep_thread,
                    keep_branch: input.keep_branch,
                    discard_changes: input.discard_changes,
                    discard_unretained_commits: input.discard_unretained_commits,
                },
                dry_run,
                expected_plan_sha256: input.expected_plan_sha256.clone(),
            },
            Some(&input.workspace),
            Some(&input.operation_id),
        )
        .await
    }

    pub(super) async fn close_preview(
        &self,
        input: &CloseInput,
    ) -> Result<WorkspaceCloseResult, DaemonFailure> {
        self.lifecycle_request(
            "workspaces.close.preview",
            WorkspaceCloseParams {
                scope: RepositoryScope::repository(self.repository.clone()),
                workspace: input.workspace.clone(),
                archive_thread: input.archive_thread,
                discard_changes: input.discard_changes,
                dry_run: true,
                expected_plan: None,
            },
            Some(&input.workspace),
            None,
        )
        .await
    }

    pub(super) async fn close_apply(
        &self,
        input: &CloseInput,
        plan: WorkspaceRetirementPlan,
    ) -> Result<WorkspaceCloseResult, DaemonFailure> {
        let workspace = plan.workspace_id.clone();
        self.lifecycle_request(
            "workspaces.close",
            WorkspaceCloseParams {
                scope: RepositoryScope::repository(self.repository.clone()),
                workspace: workspace.clone(),
                archive_thread: input.archive_thread,
                discard_changes: input.discard_changes,
                dry_run: false,
                expected_plan: Some(plan),
            },
            Some(&workspace),
            None,
        )
        .await
    }
}
