use super::*;
use crate::protocol::{TurnInterruptMode, TurnInterruptParams};

const INTERRUPT_TIMEOUT: Duration = Duration::from_secs(2);

impl Coordinator {
    pub(crate) async fn interrupt_turn(
        &self,
        params: TurnInterruptParams,
    ) -> Result<TurnInterruptResult, CoordinatorError> {
        let mut get = TurnGetParams {
            scope: params.scope,
            workspace: params.workspace,
            operation_id: params.operation_id,
        };
        let operation = self.scoped_turn_operation(&get)?;
        let initial = observation(&operation);
        get.workspace.clone_from(&operation.workspace_id);
        if initial.state == TurnObservationState::Finished {
            self.reconcile_turn_receipt(&operation);
            return Ok(interrupt_result(initial));
        }
        if initial.interrupt_attempt.is_none()
            && (initial.dispatch != TurnDispatchState::Accepted
                || initial.codex_thread_id.as_deref().is_none_or(str::is_empty)
                || initial.codex_turn_id.as_deref().is_none_or(str::is_empty))
        {
            return Ok(TurnInterruptResult {
                turn: initial,
                interruption: TurnInterruptionState::NotRequested,
            });
        }
        // Existing attempts are observed, never re-dispatched, even after a
        // restart/backend upgrade. The transaction enforces mode consistency.
        if operation.interrupt_attempt.is_none() {
            match params.mode {
                TurnInterruptMode::Strict if !self.worker.supports_exact_turn_interrupt() => {
                    return Err(WorkerError::ExactInterruptUnsupported.into());
                }
                TurnInterruptMode::BestEffort
                    if !self.worker.supports_best_effort_turn_interrupt() =>
                {
                    return Err(WorkerError::BestEffortInterruptUnsupported.into());
                }
                _ => {}
            }
        }
        let (claimed, dispatch) = self
            .store
            .claim_turn_interrupt(&operation.id, params.mode)?;
        if claimed.terminal_status.is_some() {
            self.reconcile_turn_receipt(&claimed);
            return Ok(interrupt_result(observation(&claimed)));
        }
        if !dispatch {
            return Ok(interrupt_result(self.get_turn(get).await?));
        }
        self.dispatch_turn_interrupt(get, claimed, params.mode)
            .await
    }

    async fn dispatch_turn_interrupt(
        &self,
        get: TurnGetParams,
        operation: Operation,
        mode: TurnInterruptMode,
    ) -> Result<TurnInterruptResult, CoordinatorError> {
        let thread = operation
            .native_thread_id
            .as_deref()
            .ok_or(CoordinatorError::IncompleteWorkspace("interrupt thread"))?;
        let turn = operation
            .native_result_id
            .as_deref()
            .ok_or(CoordinatorError::IncompleteWorkspace("interrupt turn"))?;
        // A committed uncertain claim is deliberately not a recoverable outbox:
        // dropping/crashing here consumes dispatch permission without promising
        // that the native request was ever written. TUI races remain possible.
        let call = async {
            match mode {
                TurnInterruptMode::Strict => self.worker.interrupt_turn(thread, turn).await,
                TurnInterruptMode::BestEffort => {
                    self.worker.interrupt_turn_best_effort(thread, turn).await
                }
            }
        };
        let reply = tokio::time::timeout(INTERRUPT_TIMEOUT, call)
            .await
            .unwrap_or(Err(WorkerError::TurnInterruptTimedOut));
        let state = match &reply {
            Ok(()) => Some(TurnInterruptAttemptState::Requested),
            Err(WorkerError::TurnInterruptRejected) => Some(TurnInterruptAttemptState::Rejected),
            Err(_) => None,
        };
        if let Some(state) = state {
            self.store
                .record_turn_interrupt_reply(&operation.id, state)?;
            if state == TurnInterruptAttemptState::Rejected {
                self.invalidate_rejected_turn_tracking(&operation);
            }
        }
        let after = self.get_turn(get).await?;
        if after.state != TurnObservationState::Finished {
            reply?;
        }
        Ok(interrupt_result(after))
    }

    fn invalidate_rejected_turn_tracking(&self, operation: &Operation) {
        let mut active = self
            .active_turn_operations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(runtime) = operation
            .native_thread_id
            .as_ref()
            .and_then(|thread| active.get_mut(thread))
            && runtime.id == operation.id
            && runtime.workspace_id == operation.workspace_id
            && runtime.native_result_id == operation.native_result_id
        {
            // A proven stale/no-active rejection invalidates our local in-flight
            // hint, but proves no terminal outcome. Never clear a newer guard.
            runtime.completion_tracking_lost = true;
        }
    }
}
