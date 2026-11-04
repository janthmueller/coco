use super::super::{Coordinator, CoordinatorError, validate_operation_id};
use std::time::Duration;

use crate::coordinator::{NativeTurnOutcome, WorkerError};
use crate::protocol::{
    TurnDispatchState, TurnGetParams, TurnInterruptAttemptState, TurnInterruptResult,
    TurnInterruptionState, TurnObservation, TurnObservationState, TurnTerminalStatus,
    TurnUnavailableReason,
};
use crate::store::{Operation, OperationKind, OperationState};

const LOOKUP_TIMEOUT: Duration = Duration::from_secs(2);

mod interrupt;

impl Coordinator {
    pub(crate) async fn get_turn(
        &self,
        params: TurnGetParams,
    ) -> Result<TurnObservation, CoordinatorError> {
        let operation = self.scoped_turn_operation(&params)?;
        let mut observed = observation(&operation);
        if operation.terminal_status.is_some() || operation.state != OperationState::Accepted {
            self.reconcile_turn_receipt(&operation);
            return Ok(observed);
        }
        let (Some(thread), Some(turn)) = (
            operation
                .native_thread_id
                .as_deref()
                .filter(|id| !id.is_empty()),
            operation
                .native_result_id
                .as_deref()
                .filter(|id| !id.is_empty()),
        ) else {
            observed.unavailable_reason = Some(TurnUnavailableReason::LegacyCorrelationMissing);
            return Ok(observed);
        };
        let native =
            tokio::time::timeout(LOOKUP_TIMEOUT, self.worker.read_turn_outcome(thread, turn)).await;
        // Completion may race either a successful or failed metadata read.
        let latest = self.scoped_turn_operation(&TurnGetParams {
            workspace: operation.workspace_id.clone(),
            ..params
        })?;
        if latest.terminal_status.is_some() {
            self.reconcile_turn_receipt(&latest);
            return Ok(observation(&latest));
        }
        match native {
            Ok(Ok(NativeTurnOutcome::Finished(status))) => {
                let recorded =
                    self.store
                        .record_operation_completion(&operation.id, thread, turn, status)?;
                self.reconcile_turn_receipt(&recorded);
                Ok(observation(&recorded))
            }
            Ok(Ok(native)) => {
                let mut active = self
                    .active_turn_operations
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let running = active.get_mut(thread).is_some_and(|runtime| {
                    if runtime.id != operation.id
                        || runtime.native_result_id.as_deref() != Some(turn)
                    {
                        return false;
                    }
                    if native == NativeTurnOutcome::InProgress {
                        runtime.completion_tracking_lost = false;
                    }
                    !runtime.uncertain && !runtime.completion_tracking_lost
                });
                let mut result = observation(&latest);
                if native == NativeTurnOutcome::HistoryUnavailable {
                    result.unavailable_reason =
                        Some(TurnUnavailableReason::NativeHistoryUnavailable);
                }
                if running || native == NativeTurnOutcome::InProgress {
                    result.state = TurnObservationState::Running;
                    result.unavailable_reason = None;
                }
                Ok(result)
            }
            Ok(Err(_)) => {
                let mut result = observation(&latest);
                result.unavailable_reason = Some(TurnUnavailableReason::NativeLookupFailed);
                Ok(result)
            }
            Err(_) => {
                let mut result = observation(&latest);
                result.unavailable_reason = Some(TurnUnavailableReason::NativeLookupTimedOut);
                Ok(result)
            }
        }
    }

    fn scoped_turn_operation(&self, params: &TurnGetParams) -> Result<Operation, CoordinatorError> {
        validate_operation_id(&params.operation_id)?;
        let workspace = self.resolve_workspace(&params.scope, &params.workspace)?;
        self.store
            .operation_by_client_id(&params.operation_id)?
            .filter(|operation| {
                operation.workspace_id == workspace.id && operation.kind == OperationKind::TurnStart
            })
            .ok_or(CoordinatorError::TurnOperationNotFound)
    }
}

fn observation(operation: &Operation) -> TurnObservation {
    let (dispatch, state, unavailable_reason) = match operation.state {
        OperationState::Prepared => (
            TurnDispatchState::Prepared,
            TurnObservationState::NotStarted,
            None,
        ),
        OperationState::Dispatching => (
            TurnDispatchState::Dispatching,
            TurnObservationState::Starting,
            None,
        ),
        OperationState::Uncertain => (
            TurnDispatchState::Uncertain,
            TurnObservationState::Unavailable,
            Some(TurnUnavailableReason::AcceptanceUnconfirmed),
        ),
        OperationState::Accepted => (
            TurnDispatchState::Accepted,
            TurnObservationState::Unavailable,
            Some(TurnUnavailableReason::CompletionNotObserved),
        ),
    };
    TurnObservation {
        workspace_id: operation.workspace_id.clone(),
        operation_id: operation.operation_id.clone(),
        codex_thread_id: operation.native_thread_id.clone(),
        codex_turn_id: operation.native_result_id.clone(),
        dispatch,
        state: if operation.terminal_status.is_some() {
            TurnObservationState::Finished
        } else {
            state
        },
        outcome: operation.terminal_status,
        outcome_observed_at_ms: operation.terminal_observed_at_ms,
        interrupt_requested_at_ms: operation.interrupt_requested_at_ms,
        interrupt_attempt: operation.interrupt_attempt,
        unavailable_reason: if operation.terminal_status.is_some() {
            None
        } else {
            unavailable_reason
        },
    }
}

fn interrupt_result(turn: TurnObservation) -> TurnInterruptResult {
    let interruption = match turn.outcome {
        Some(TurnTerminalStatus::Interrupted) => TurnInterruptionState::Confirmed,
        Some(_) => TurnInterruptionState::AlreadyFinished,
        None => match turn.interrupt_attempt.map(|attempt| attempt.state) {
            Some(TurnInterruptAttemptState::Requested) => TurnInterruptionState::Requested,
            Some(TurnInterruptAttemptState::Uncertain) => TurnInterruptionState::Uncertain,
            Some(TurnInterruptAttemptState::Rejected) => TurnInterruptionState::Rejected,
            None => TurnInterruptionState::NotRequested,
        },
    };
    TurnInterruptResult { turn, interruption }
}
