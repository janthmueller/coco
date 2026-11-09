use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use super::{NewWorkspace, Store, StoreError, json_to_sql_error, now_ms};
use crate::domain::setup::{
    CreationProfileEvidence, WorkerMcpReference, WorkspaceCreationReceipt, WorkspaceCreationState,
};
use crate::domain::{Workspace, WorkspaceLifecycle};

mod threads;
pub(super) use threads::remember_worker_thread;

impl Store {
    pub(crate) fn workspace_creation(
        &self,
        operation: &str,
    ) -> Result<Option<WorkspaceCreationReceipt>, StoreError> {
        creation(&*self.lock()?, operation)
    }

    pub(crate) fn workspace_by_creation_claim(
        &self,
        operation: &str,
        request_sha256: Option<&str>,
    ) -> Result<Option<Workspace>, StoreError> {
        let connection = self.lock()?;
        let receipt = creation(&connection, operation)?;
        let workspace = connection
            .query_row(
                &format!(
                    "{} WHERE create_operation_id = ?1",
                    super::rows::WORKSPACE_SELECT
                ),
                [operation],
                super::rows::map_workspace,
            )
            .optional()?;
        match (receipt.as_ref(), request_sha256) {
            (None, None) => {}
            (Some(receipt), None) if receipt.request_sha256.is_none() && workspace.is_some() => {}
            (Some(receipt), Some(expected))
                if receipt.request_sha256.as_deref() == Some(expected)
                    && receipt.state == WorkspaceCreationState::Preparing
                    && receipt.workspace_id.as_deref()
                        == workspace.as_ref().map(|w| w.id.as_str()) => {}
            _ => return Err(StoreError::CreationKeyRetired),
        }
        Ok(workspace)
    }

    /// The winning caller alone may execute. A durable intent is never stolen
    /// after cancellation/restart, including when no workspace is linked yet.
    pub(crate) fn claim_workspace_creation(
        &self,
        receipt: &WorkspaceCreationReceipt,
    ) -> Result<(WorkspaceCreationReceipt, bool), StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(existing) = creation(&transaction, &receipt.operation_id)? {
            transaction.commit()?;
            return Ok((existing, false));
        }
        // A missing receipt must not let a public request adopt an existing
        // workspace or install a new grant under a previously used key.
        let existing = transaction
            .query_row(
                &format!(
                    "{} WHERE create_operation_id = ?1",
                    super::rows::WORKSPACE_SELECT
                ),
                [&receipt.operation_id],
                super::rows::map_workspace,
            )
            .optional()?;
        if let Some(workspace) = existing {
            observe_workspace(&transaction, &workspace, false)?;
            let existing = creation(&transaction, &receipt.operation_id)?
                .ok_or(StoreError::CreationKeyRetired)?;
            transaction.commit()?;
            return Ok((existing, false));
        }
        write_creation(&transaction, receipt)?;
        transaction.commit()?;
        Ok((receipt.clone(), true))
    }

    pub(crate) fn reject_workspace_creation(
        &self,
        operation: &str,
        code: &str,
    ) -> Result<(), StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(mut receipt) = creation(&transaction, operation)?
            && receipt.state == WorkspaceCreationState::Preparing
        {
            receipt.state = if receipt.workspace_id.is_some() {
                WorkspaceCreationState::Failed
            } else {
                WorkspaceCreationState::Rejected
            };
            receipt.error_code = Some(code.to_owned());
            receipt.updated_at_ms = Some(now_ms());
            write_creation(&transaction, &receipt)?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn worker_mcp_reference(
        &self,
        workspace_id: &str,
    ) -> Result<Option<WorkerMcpReference>, StoreError> {
        let connection = self.lock()?;
        let row: Option<(bool, Option<String>)> = connection
            .query_row(
                "SELECT w.worker_mcp_bound, c.receipt_json FROM workspaces w
                 LEFT JOIN workspace_creations c ON c.workspace_id = w.id WHERE w.id = ?1",
                [workspace_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((bound, json)) = row else {
            return Ok(None);
        };
        let reference = json
            .map(|json| {
                serde_json::from_str::<WorkspaceCreationReceipt>(&json)
                    .map(|r| r.worker_mcp)
                    .map_err(json_to_sql_error)
            })
            .transpose()
            .map(Option::flatten)?;
        if bound != reference.is_some() {
            return Err(StoreError::WorkerMcpBindingMissing);
        }
        Ok(reference)
    }

    pub(crate) fn thread_has_worker_mcp_binding(
        &self,
        thread_id: &str,
    ) -> Result<bool, StoreError> {
        Ok(self.lock()?.query_row(
            "SELECT EXISTS(SELECT 1 FROM worker_mcp_threads WHERE thread_id = ?1)
             OR EXISTS(SELECT 1 FROM workspaces WHERE codex_thread_id = ?1 AND worker_mcp_bound = 1)
             OR EXISTS(SELECT 1 FROM workspace_creations
              WHERE json_extract(receipt_json, '$.codexThreadId') = ?1
                AND (worker_mcp_bound = 1 OR json_type(receipt_json, '$.workerMcp') = 'object'))",
            [thread_id],
            |row| row.get(0),
        )?)
    }
}

/// Association must own the original public intent, or be a new private key.
/// Earlier coordinator checks do not protect against another entry point
/// claiming the key while a caller is waiting for the repository gate.
pub(super) fn assert_creation_claim(
    connection: &Connection,
    input: &NewWorkspace,
) -> Result<(), StoreError> {
    let Some(operation) = input.create_operation_id.as_deref() else {
        return if input.creation_request_sha256.is_none() {
            Ok(())
        } else {
            Err(StoreError::CreationKeyRetired)
        };
    };
    match (
        creation(connection, operation)?,
        input.creation_request_sha256.as_ref(),
    ) {
        (None, None) => Ok(()),
        (Some(receipt), Some(expected))
            if receipt.repository_id == input.repository_id
                && receipt.request_sha256.as_ref() == Some(expected)
                && receipt.workspace_id.is_none()
                && receipt.state == WorkspaceCreationState::Preparing =>
        {
            Ok(())
        }
        _ => Err(StoreError::CreationKeyRetired),
    }
}

pub(super) fn creation(
    connection: &Connection,
    operation: &str,
) -> Result<Option<WorkspaceCreationReceipt>, StoreError> {
    let json: Option<String> = connection
        .query_row(
            "SELECT receipt_json FROM workspace_creations WHERE operation_id = ?1",
            [operation],
            |row| row.get(0),
        )
        .optional()?;
    json.map(|value| serde_json::from_str(&value).map_err(json_to_sql_error))
        .transpose()
}

fn write_creation(
    connection: &Connection,
    receipt: &WorkspaceCreationReceipt,
) -> Result<(), StoreError> {
    let json = serde_json::to_string(receipt).map_err(json_to_sql_error)?;
    connection.execute(
        "INSERT INTO workspace_creations(operation_id, repository_id, request_sha256, workspace_id, receipt_json, worker_mcp_bound)
         VALUES (?1, ?2, ?3, ?4, ?5, MAX(json_type(?5, '$.workerMcp') = 'object',
            COALESCE((SELECT worker_mcp_bound FROM workspaces WHERE id = ?4), 0)))
         ON CONFLICT(operation_id) DO UPDATE SET
         workspace_id = excluded.workspace_id, receipt_json = excluded.receipt_json,
         worker_mcp_bound = MAX(workspace_creations.worker_mcp_bound, excluded.worker_mcp_bound)",
        params![receipt.operation_id, receipt.repository_id, receipt.request_sha256, receipt.workspace_id, json],
    )?;
    Ok(())
}

/// Called in the SAME transaction as workspace insert/transition/binding. The
/// minimal receipt survives deletion, but never mirrors conversation contents.
pub(super) fn observe_workspace(
    connection: &Connection,
    workspace: &Workspace,
    deleted: bool,
) -> Result<(), StoreError> {
    if let Some(thread) = workspace.codex_thread_id.as_deref() {
        remember_worker_thread(connection, &workspace.id, thread)?;
    }
    let Some(operation) = workspace.create_operation_id.as_deref() else {
        return Ok(());
    };
    let mut receipt = match creation(connection, operation)? {
        Some(receipt) => {
            if receipt.repository_id != workspace.repository_id
                || receipt
                    .workspace_id
                    .as_ref()
                    .is_some_and(|id| id != &workspace.id)
                || receipt.state == WorkspaceCreationState::Deleted
            {
                return Err(StoreError::CreationKeyRetired);
            }
            receipt
        }
        None => {
            let path: String = connection.query_row(
                "SELECT root_path FROM repositories WHERE id = ?1",
                [&workspace.repository_id],
                |row| row.get(0),
            )?;
            WorkspaceCreationReceipt::unknown(
                operation.to_owned(),
                workspace.repository_id.clone(),
                path.into(),
            )
        }
    };
    receipt.workspace_id = Some(workspace.id.clone());
    receipt.workspace_name = Some(workspace.name.clone());
    receipt.base_sha = workspace.base_sha.clone();
    receipt.worktree_path = workspace.worktree_path.clone();
    receipt.branch_name = workspace.branch_name.clone();
    receipt.profile = Some(CreationProfileEvidence {
        name: workspace.profile.name.clone(),
        source_path: workspace.profile.source_path.clone(),
        source_sha256: workspace.profile.source_hash.clone(),
        model: workspace.profile.model_override.clone(),
    });
    if workspace.codex_thread_id.is_some() {
        receipt.codex_thread_id = workspace.codex_thread_id.clone();
    }
    receipt.created_at_ms.get_or_insert(workspace.created_at_ms);
    receipt.updated_at_ms = Some(now_ms());
    receipt.state = if deleted {
        WorkspaceCreationState::Deleted
    } else if receipt.request_sha256.is_none() {
        WorkspaceCreationState::Unavailable
    } else if receipt.state == WorkspaceCreationState::Preparing {
        match workspace.lifecycle {
            WorkspaceLifecycle::Ready => WorkspaceCreationState::Configured,
            WorkspaceLifecycle::Failed => {
                receipt.error_code = workspace.last_error_code.clone();
                WorkspaceCreationState::Failed
            }
            _ => WorkspaceCreationState::Preparing,
        }
    } else {
        receipt.state
    };
    if receipt.worker_mcp.is_some() {
        connection.execute(
            "UPDATE workspaces SET worker_mcp_bound = 1 WHERE id = ?1",
            [&workspace.id],
        )?;
    }
    write_creation(connection, &receipt)
}
