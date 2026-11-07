use super::*;
use crate::domain::TemporaryWorkspacePolicy;

impl Store {
    /// Consume cleanup permission before any side effect. A crash or blocked
    /// deletion leaves a persistent workspace, never a delayed cleanup retry.
    pub(crate) fn claim_temporary_cleanup(
        &self,
        workspace_id: &str,
    ) -> Result<Option<TemporaryWorkspacePolicy>, StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let workspace = require_workspace(&transaction, workspace_id)?;
        if workspace.temporary.is_some() {
            transaction.execute(
                "UPDATE workspaces SET temporary_cleanup_json = NULL, updated_at_ms = ?2 WHERE id = ?1",
                params![workspace_id, now_ms()],
            )?;
        }
        transaction.commit()?;
        Ok(workspace.temporary)
    }
}
