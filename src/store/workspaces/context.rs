use super::*;

impl Store {
    /// Atomically finishes compaction and clears its durable pending marker.
    pub(crate) fn complete_context_compaction(
        &self,
        workspace_id: &str,
        mut event: EventDraft,
    ) -> Result<(Workspace, NormalizedEvent), StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        assert_workspace_lifecycle(&transaction, workspace_id, &[WorkspaceLifecycle::Starting])?;
        let mut workspace = require_workspace(&transaction, workspace_id)?;
        let context = workspace
            .context
            .pointer_mut("/resolved/context")
            .and_then(serde_json::Value::as_object_mut)
            .ok_or_else(|| StoreError::InvalidWorkspaceTransition {
                workspace_id: workspace_id.to_owned(),
                expected: "stored compaction context".to_owned(),
                actual: "missing context".to_owned(),
            })?;
        context.insert("compactionPending".to_owned(), json!(false));
        let serialized = serde_json::to_string(&workspace.context).map_err(json_to_sql_error)?;
        transaction.execute(
            "UPDATE workspaces SET lifecycle = 'ready', context_json = ?1,
                updated_at_ms = ?2 WHERE id = ?3",
            params![serialized, now_ms(), workspace_id],
        )?;
        event.workspace_id = Some(workspace_id.to_owned());
        let event = insert_event(&transaction, event)?;
        let workspace = require_workspace(&transaction, workspace_id)?;
        transaction.commit()?;
        Ok((workspace, event))
    }
}
