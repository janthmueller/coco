use rusqlite::{TransactionBehavior, params};

use super::operations::require_operation;
use super::{Operation, OperationState, Store, StoreError, now_ms};
use crate::domain::turns::{TurnInterruptAttemptState, TurnInterruptMode, TurnTerminalStatus};

impl Store {
    /// Stores only a proven terminal receipt for an accepted CoCo operation.
    /// Conversation text and uncorrelated native events never enter this row.
    pub(crate) fn record_operation_completion(
        &self,
        id: &str,
        thread_id: &str,
        turn_id: &str,
        status: TurnTerminalStatus,
    ) -> Result<Operation, StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let operation = require_operation(&transaction, id)?;
        if operation.state != OperationState::Accepted
            || operation.native_thread_id.as_deref() != Some(thread_id)
            || operation.native_result_id.as_deref() != Some(turn_id)
            || operation
                .terminal_status
                .is_some_and(|previous| previous != status)
        {
            return Err(StoreError::OperationResultConflict {
                operation_id: operation.operation_id,
            });
        }
        transaction.execute(
            "UPDATE operations SET terminal_status = ?2,
                terminal_observed_at_ms = COALESCE(terminal_observed_at_ms, ?3)
             WHERE id = ?1",
            params![id, status.as_str(), now_ms()],
        )?;
        let recorded = require_operation(&transaction, id)?;
        transaction.commit()?;
        Ok(recorded)
    }

    /// Atomically consumes the only dispatch permission for this operation.
    /// Uncertain is committed BEFORE a native call; it is never auto-retried.
    pub(crate) fn claim_turn_interrupt(
        &self,
        id: &str,
        mode: TurnInterruptMode,
    ) -> Result<(Operation, bool), StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let operation = require_operation(&transaction, id)?;
        if operation.terminal_status.is_some() {
            transaction.commit()?;
            return Ok((operation, false));
        }
        if let Some(attempt) = operation.interrupt_attempt {
            if attempt.mode != mode {
                return Err(StoreError::InterruptModeConflict {
                    operation_id: operation.operation_id,
                });
            }
            transaction.commit()?;
            return Ok((operation, false));
        }
        if operation.state != OperationState::Accepted
            || operation
                .native_thread_id
                .as_deref()
                .is_none_or(str::is_empty)
            || operation
                .native_result_id
                .as_deref()
                .is_none_or(str::is_empty)
        {
            return Err(StoreError::InvalidOperationState {
                operation_id: operation.operation_id,
                expected: "accepted with exact native IDs",
                actual: operation.state.as_str().to_owned(),
            });
        }
        transaction.execute(
            "UPDATE operations SET interrupt_requested_at_ms = ?2, interrupt_mode = ?3,
                interrupt_state = 'uncertain' WHERE id = ?1 AND interrupt_requested_at_ms IS NULL",
            params![id, now_ms(), mode.as_str()],
        )?;
        let recorded = require_operation(&transaction, id)?;
        transaction.commit()?;
        Ok((recorded, true))
    }

    /// Records only a received acknowledgement or a proven pre-dispatch native
    /// rejection. A lost reply or crashed caller leaves the original claim uncertain.
    pub(crate) fn record_turn_interrupt_reply(
        &self,
        id: &str,
        state: TurnInterruptAttemptState,
    ) -> Result<Operation, StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let operation = require_operation(&transaction, id)?;
        if !matches!(
            state,
            TurnInterruptAttemptState::Requested | TurnInterruptAttemptState::Rejected
        ) || operation.interrupt_attempt.is_none_or(|attempt| {
            attempt.state != TurnInterruptAttemptState::Uncertain && attempt.state != state
        }) {
            return Err(StoreError::OperationResultConflict {
                operation_id: operation.operation_id,
            });
        }
        transaction.execute(
            "UPDATE operations SET interrupt_state = ?2 WHERE id = ?1",
            params![id, state.as_str()],
        )?;
        let recorded = require_operation(&transaction, id)?;
        transaction.commit()?;
        Ok(recorded)
    }
}
