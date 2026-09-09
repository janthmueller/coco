use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use super::events::insert_event;
use super::rows::{OPERATION_SELECT, map_operation, require_workspace};
use super::workspaces::assert_workspace_lifecycle;
use super::{
    NewOperation, Operation, OperationState, Store, StoreError, ThreadOperationAcceptance,
    json_to_sql_error, new_id, now_ms,
};
use crate::domain::{NormalizedEvent, Workspace, WorkspaceLifecycle};

impl Store {
    /// Persists a turn-start intent before any App Server side effect.
    ///
    /// The boolean is true only when this call inserted the operation. An
    /// existing record is returned unchanged so the coordinator can apply the
    /// idempotency fingerprint and state-machine rules itself.
    pub fn prepare_operation(&self, input: NewOperation) -> Result<(Operation, bool), StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        assert_workspace_lifecycle(
            &transaction,
            &input.workspace_id,
            &[WorkspaceLifecycle::Ready],
        )?;
        if let Some(existing) = operation_by_client_id(&transaction, &input.operation_id)? {
            transaction.commit()?;
            return Ok((existing, false));
        }

        let id = new_id();
        let now = now_ms();
        transaction.execute(
            "INSERT INTO operations (
                id, operation_id, workspace_id, kind, request_fingerprint,
                native_result_id, state, created_at_ms, dispatch_started_at_ms,
                result_recorded_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, 'prepared', ?6, NULL, NULL)",
            params![
                id,
                input.operation_id,
                input.workspace_id,
                input.kind.as_str(),
                input.request_fingerprint,
                now,
            ],
        )?;
        let operation = require_operation(&transaction, &id)?;
        transaction.commit()?;
        Ok((operation, true))
    }

    pub fn begin_operation_dispatch(&self, id: &str) -> Result<Operation, StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let operation = require_operation(&transaction, id)?;
        if operation.state != OperationState::Prepared {
            return Err(StoreError::InvalidOperationState {
                operation_id: operation.operation_id,
                expected: "prepared",
                actual: operation.state.as_str().to_owned(),
            });
        }
        let now = now_ms();
        transaction.execute(
            "UPDATE operations SET state = 'dispatching', dispatch_started_at_ms = ?1
             WHERE id = ?2 AND state = 'prepared'",
            params![now, id],
        )?;
        let operation = require_operation(&transaction, id)?;
        transaction.commit()?;
        Ok(operation)
    }

    /// Records a native result proven by the direct App Server response.
    pub fn accept_operation(
        &self,
        id: &str,
        native_result_id: &str,
    ) -> Result<Operation, StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let operation = require_operation(&transaction, id)?;
        if operation.state == OperationState::Accepted {
            if operation.native_result_id.as_deref() == Some(native_result_id) {
                transaction.commit()?;
                return Ok(operation);
            }
            return Err(StoreError::OperationResultConflict {
                operation_id: operation.operation_id,
            });
        }
        if !matches!(
            operation.state,
            OperationState::Dispatching | OperationState::Uncertain
        ) {
            return Err(StoreError::InvalidOperationState {
                operation_id: operation.operation_id,
                expected: "dispatching or uncertain",
                actual: operation.state.as_str().to_owned(),
            });
        }
        let now = now_ms();
        transaction.execute(
            "UPDATE operations SET state = 'accepted', native_result_id = ?1,
                result_recorded_at_ms = ?2 WHERE id = ?3",
            params![native_result_id, now, id],
        )?;
        let operation = require_operation(&transaction, id)?;
        transaction.commit()?;
        Ok(operation)
    }

    /// Atomically records the first durable thread binding and the native turn
    /// response that made that thread recoverable.
    pub fn bind_thread_and_accept_operation(
        &self,
        mut acceptance: ThreadOperationAcceptance,
    ) -> Result<(Workspace, Operation, NormalizedEvent), StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        assert_workspace_lifecycle(
            &transaction,
            &acceptance.workspace_id,
            &[WorkspaceLifecycle::Ready],
        )?;
        let workspace = require_workspace(&transaction, &acceptance.workspace_id)?;
        if workspace.codex_thread_id.is_some() || workspace.active_turn_id.is_some() {
            return Err(StoreError::InvalidWorkspaceTransition {
                workspace_id: acceptance.workspace_id,
                expected: "an unbound workspace without an active turn".to_owned(),
                actual: "a bound workspace or active turn".to_owned(),
            });
        }
        let operation = require_operation(&transaction, &acceptance.operation_id)?;
        if operation.workspace_id != acceptance.workspace_id {
            return Err(StoreError::OperationWorkspaceMismatch {
                operation_id: operation.operation_id,
                expected_workspace_id: acceptance.workspace_id,
                actual_workspace_id: operation.workspace_id,
            });
        }
        if operation.state != OperationState::Dispatching {
            return Err(StoreError::InvalidOperationState {
                operation_id: operation.operation_id,
                expected: "dispatching",
                actual: operation.state.as_str().to_owned(),
            });
        }

        let now = now_ms();
        let profile_json = serde_json::to_string(&acceptance.profile).map_err(json_to_sql_error)?;
        transaction.execute(
            "UPDATE workspaces SET profile_json = ?1, codex_thread_id = ?2,
                parent_thread_id = ?3, thread_status_json = NULL,
                thread_status_generation = NULL, thread_status_observed_at_ms = NULL,
                thread_status_is_fresh = 0, updated_at_ms = ?4 WHERE id = ?5",
            params![
                profile_json,
                acceptance.binding.thread_id,
                acceptance.binding.parent_thread_id,
                now,
                acceptance.workspace_id,
            ],
        )?;
        transaction.execute(
            "UPDATE operations SET state = 'accepted', native_result_id = ?1,
                result_recorded_at_ms = ?2 WHERE id = ?3 AND state = 'dispatching'",
            params![acceptance.native_result_id, now, acceptance.operation_id,],
        )?;
        acceptance.event.workspace_id = Some(acceptance.workspace_id.clone());
        let event = insert_event(&transaction, acceptance.event)?;
        let workspace = require_workspace(&transaction, &acceptance.workspace_id)?;
        let operation = require_operation(&transaction, &acceptance.operation_id)?;
        transaction.commit()?;
        Ok((workspace, operation, event))
    }

    pub fn mark_operation_uncertain(&self, id: &str) -> Result<Operation, StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let operation = require_operation(&transaction, id)?;
        match operation.state {
            OperationState::Accepted | OperationState::Uncertain => {
                transaction.commit()?;
                return Ok(operation);
            }
            OperationState::Dispatching => {}
            OperationState::Prepared => {
                return Err(StoreError::InvalidOperationState {
                    operation_id: operation.operation_id,
                    expected: "dispatching",
                    actual: operation.state.as_str().to_owned(),
                });
            }
        }
        let now = now_ms();
        transaction.execute(
            "UPDATE operations SET state = 'uncertain', result_recorded_at_ms = ?1
             WHERE id = ?2 AND state = 'dispatching'",
            params![now, id],
        )?;
        let operation = require_operation(&transaction, id)?;
        transaction.commit()?;
        Ok(operation)
    }

    #[cfg(test)]
    pub fn operation_by_id(&self, id: &str) -> Result<Option<Operation>, StoreError> {
        let connection = self.lock()?;
        get_operation_by_id(&connection, id)
    }

    pub fn operation_by_client_id(
        &self,
        operation_id: &str,
    ) -> Result<Option<Operation>, StoreError> {
        let connection = self.lock()?;
        operation_by_client_id(&connection, operation_id)
    }

    pub fn operation_by_native_result_id(
        &self,
        native_result_id: &str,
    ) -> Result<Option<Operation>, StoreError> {
        let connection = self.lock()?;
        connection
            .query_row(
                &format!("{OPERATION_SELECT} WHERE native_result_id = ?1"),
                [native_result_id],
                map_operation,
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn mark_unconfirmed_operations_uncertain(&self) -> Result<usize, StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let updated = reconcile_unconfirmed_operations(&transaction, now_ms())?;
        transaction.commit()?;
        Ok(updated)
    }
}

pub(super) fn reconcile_unconfirmed_operations(
    connection: &Connection,
    now: i64,
) -> Result<usize, StoreError> {
    connection
        .execute(
            "UPDATE operations SET state = 'uncertain', result_recorded_at_ms = ?1
             WHERE state = 'dispatching'",
            [now],
        )
        .map_err(StoreError::from)
}

fn operation_by_client_id(
    connection: &Connection,
    operation_id: &str,
) -> Result<Option<Operation>, StoreError> {
    connection
        .query_row(
            &format!("{OPERATION_SELECT} WHERE operation_id = ?1"),
            [operation_id],
            map_operation,
        )
        .optional()
        .map_err(StoreError::from)
}

fn get_operation_by_id(connection: &Connection, id: &str) -> Result<Option<Operation>, StoreError> {
    connection
        .query_row(
            &format!("{OPERATION_SELECT} WHERE id = ?1"),
            [id],
            map_operation,
        )
        .optional()
        .map_err(StoreError::from)
}

fn require_operation(connection: &Connection, id: &str) -> Result<Operation, StoreError> {
    get_operation_by_id(connection, id)?.ok_or_else(|| StoreError::NotFound {
        entity: "operation",
        id: id.to_owned(),
    })
}
