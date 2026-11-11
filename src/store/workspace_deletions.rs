use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use super::{Store, StoreError, json_to_sql_error, now_ms};
use crate::domain::Workspace;
use crate::domain::retirement::{WorkspaceDeletionReceipt, WorkspaceDeletionState};

impl Store {
    pub(crate) fn workspace_deletion(
        &self,
        operation: &str,
    ) -> Result<Option<WorkspaceDeletionReceipt>, StoreError> {
        deletion(&*self.lock()?, operation)
    }

    /// Only the inserting caller may dispatch. Neither restart nor polling
    /// grants another destructive attempt under an already retained key.
    pub(crate) fn claim_workspace_deletion(
        &self,
        receipt: &WorkspaceDeletionReceipt,
    ) -> Result<(WorkspaceDeletionReceipt, bool), StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(existing) = deletion(&transaction, &receipt.operation_id)? {
            return Ok((existing, false));
        }
        let plan = receipt
            .plan
            .as_ref()
            .ok_or(StoreError::DeletionClaimMismatch)?;
        let workspace = super::rows::require_workspace(&transaction, &plan.workspace_id)?;
        if receipt.state != WorkspaceDeletionState::Preparing
            || receipt.request_sha256.is_none()
            || workspace.repository_id != receipt.repository_id
        {
            return Err(StoreError::DeletionClaimMismatch);
        }
        write(&transaction, receipt)?;
        transaction.commit()?;
        Ok((receipt.clone(), true))
    }

    pub(crate) fn fail_workspace_deletion(
        &self,
        operation: &str,
        code: &str,
    ) -> Result<(), StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(mut receipt) = deletion(&transaction, operation)? {
            match receipt.state {
                WorkspaceDeletionState::Preparing => {
                    receipt.state = WorkspaceDeletionState::Rejected
                }
                WorkspaceDeletionState::Deleting => {
                    receipt.state = WorkspaceDeletionState::Uncertain
                }
                WorkspaceDeletionState::Uncertain => {}
                _ => return Ok(()),
            }
            receipt.error_code = Some(code.into());
            receipt.updated_at_ms = Some(now_ms());
            write(&transaction, &receipt)?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Startup only: a pre-saga claim is not permission to resume file discard.
    pub(crate) fn recover_unassociated_deletions(&self) -> Result<(), StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let receipts = {
            let mut statement = transaction.prepare(
                "SELECT receipt_json FROM workspace_deletions
                 WHERE associated = 0 AND json_extract(receipt_json, '$.state') = 'preparing'",
            )?;
            statement
                .query_map([], read)?
                .collect::<Result<Vec<_>, _>>()?
        };
        for mut receipt in receipts {
            if receipt.state == WorkspaceDeletionState::Preparing {
                receipt.state = WorkspaceDeletionState::Uncertain;
                receipt.error_code = Some("DELETION_DISPATCH_UNCONFIRMED".into());
                receipt.updated_at_ms = Some(now_ms());
                write(&transaction, &receipt)?;
            }
        }
        transaction.commit()?;
        Ok(())
    }
}

/// Called in the same transaction that accepts the core deletion intent.
pub(super) fn associate(
    connection: &Connection,
    workspace: &Workspace,
    operation: Option<&str>,
) -> Result<(), StoreError> {
    let Some(operation) = operation else {
        return Ok(());
    };
    let mut receipt = deletion(connection, operation)?.ok_or(StoreError::DeletionClaimMismatch)?;
    if receipt.state != WorkspaceDeletionState::Preparing
        || receipt.repository_id != workspace.repository_id
        || receipt.plan.as_ref().map(|p| p.workspace_id.as_str()) != Some(workspace.id.as_str())
    {
        return Err(StoreError::DeletionClaimMismatch);
    }
    receipt.state = WorkspaceDeletionState::Deleting;
    receipt.updated_at_ms = Some(now_ms());
    write(connection, &receipt)?;
    connection.execute(
        "UPDATE workspace_deletions SET associated = 1 WHERE operation_id = ?1 AND associated = 0",
        [operation],
    )?;
    Ok(())
}

/// Only the operation attached to this saga receives its completion/cancellation.
/// An unrelated later CLI deletion must not repair an older uncertain attempt.
pub(super) fn finish(
    connection: &Connection,
    workspace: &Workspace,
    deleted: bool,
) -> Result<(), StoreError> {
    let receipt = connection.query_row(
        "SELECT receipt_json FROM workspace_deletions WHERE workspace_id = ?1 AND associated = 1",
        [&workspace.id], read,
    ).optional()?;
    if let Some(mut receipt) = receipt {
        if receipt.repository_id != workspace.repository_id {
            return Err(StoreError::DeletionClaimMismatch);
        }
        receipt.state = if deleted {
            WorkspaceDeletionState::Deleted
        } else {
            WorkspaceDeletionState::Uncertain
        };
        receipt.error_code = (!deleted).then(|| "DELETION_INCOMPLETE".into());
        receipt.updated_at_ms = Some(now_ms());
        write(connection, &receipt)?;
        connection.execute(
            "UPDATE workspace_deletions SET associated = 0 WHERE operation_id = ?1",
            [&receipt.operation_id],
        )?;
    }
    Ok(())
}

fn deletion(
    connection: &Connection,
    operation: &str,
) -> Result<Option<WorkspaceDeletionReceipt>, StoreError> {
    connection
        .query_row(
            "SELECT receipt_json FROM workspace_deletions WHERE operation_id = ?1",
            [operation],
            read,
        )
        .optional()
        .map_err(StoreError::from)
}

fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkspaceDeletionReceipt> {
    serde_json::from_str(&row.get::<_, String>(0)?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
    })
}

fn write(connection: &Connection, receipt: &WorkspaceDeletionReceipt) -> Result<(), StoreError> {
    let plan = receipt
        .plan
        .as_ref()
        .ok_or(StoreError::DeletionClaimMismatch)?;
    let json = serde_json::to_string(receipt).map_err(json_to_sql_error)?;
    connection.execute(
        "INSERT INTO workspace_deletions(operation_id, repository_id, workspace_id, receipt_json)
         VALUES (?1, ?2, ?3, ?4) ON CONFLICT(operation_id) DO UPDATE SET receipt_json = excluded.receipt_json",
        params![receipt.operation_id, receipt.repository_id, plan.workspace_id, json],
    )?;
    Ok(())
}
