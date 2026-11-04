use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use super::{ObservedAgentOutput, RuntimeTurnOperation};
use crate::coordinator::{Coordinator, CoordinatorError, validate_operation_id};
use crate::protocol::{TurnResult, TurnResultParams, TurnTerminalStatus};
use crate::store::{Operation, OperationKind, OperationState, StoreError};

const MAX_COMPLETED_TURN_RESULTS: usize = 256;
const MAX_COMPLETED_TURN_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const OUTPUT_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);
// One MiB remains below the 8 MiB local RPC frame even if JSON must escape
// every retained byte as a six-byte Unicode sequence.
pub(in crate::coordinator) const MAX_TURN_RESPONSE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompletedTurnResult {
    codex_thread_id: String,
    codex_turn_id: String,
    status: TurnTerminalStatus,
    response: Option<String>,
    response_truncated: bool,
    output_pending_until: Option<Instant>,
}

#[derive(Debug, Default)]
pub(in crate::coordinator) struct TurnResultRegistry {
    entries: HashMap<String, CompletedTurnResult>,
    order: VecDeque<String>,
    response_bytes: usize,
}

impl TurnResultRegistry {
    fn insert(&mut self, operation_id: String, result: CompletedTurnResult) {
        let result_bytes = result.response.as_ref().map_or(0, String::len);
        if let Some(previous) = self.entries.insert(operation_id.clone(), result) {
            self.response_bytes = self
                .response_bytes
                .saturating_sub(previous.response.as_ref().map_or(0, String::len));
        } else {
            self.order.push_back(operation_id);
        }
        self.response_bytes += result_bytes;
        while self.order.len() > MAX_COMPLETED_TURN_RESULTS
            || self.response_bytes > MAX_COMPLETED_TURN_RESPONSE_BYTES
        {
            if let Some(expired) = self.order.pop_front()
                && let Some(result) = self.entries.remove(&expired)
            {
                self.response_bytes = self
                    .response_bytes
                    .saturating_sub(result.response.as_ref().map_or(0, String::len));
            }
        }
    }

    fn get(&self, operation_id: &str) -> Option<&CompletedTurnResult> {
        self.entries.get(operation_id)
    }

    fn observe_late_output(&mut self, thread: &str, turn: &str, response: &str) -> bool {
        let Some((key, mut result)) = self
            .entries
            .iter()
            .find(|(_, result)| result.codex_thread_id == thread && result.codex_turn_id == turn)
            .map(|(key, result)| (key.clone(), result.clone()))
        else {
            return false;
        };
        let (response, truncated) = bounded_turn_response(response);
        result.response = Some(response);
        result.response_truncated = truncated;
        self.insert(key, result);
        true
    }

    fn finish_output_delivery(
        &mut self,
        thread: &str,
        turn: &str,
        status: TurnTerminalStatus,
    ) -> bool {
        if let Some(result) = self
            .entries
            .values_mut()
            .find(|result| result.codex_thread_id == thread && result.codex_turn_id == turn)
        {
            if result.status == status {
                result.output_pending_until = None;
            }
            return true;
        }
        false
    }
}

impl Coordinator {
    pub(crate) fn turn_result(
        &self,
        params: TurnResultParams,
    ) -> Result<TurnResult, CoordinatorError> {
        validate_operation_id(&params.operation_id)?;
        let operation = self
            .store
            .operation_by_client_id(&params.operation_id)?
            .ok_or_else(|| crate::store::StoreError::NotFound {
                entity: "turn operation",
                id: params.operation_id.clone(),
            })?;
        if operation.kind != OperationKind::TurnStart {
            return Err(CoordinatorError::InvalidParams(
                "operationId does not identify a turn start".to_owned(),
            ));
        }

        if let Some(result) = self
            .completed_turn_results
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&params.operation_id)
            .cloned()
        {
            if result
                .output_pending_until
                .is_some_and(|until| Instant::now() < until)
            {
                return Ok(TurnResult::Pending {
                    codex_turn_id: Some(result.codex_turn_id),
                });
            }
            return Ok(TurnResult::Finished {
                codex_turn_id: result.codex_turn_id,
                status: result.status,
                response: result.response,
                response_truncated: result.response_truncated,
            });
        }
        if let Some(runtime) = self
            .active_turn_operations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .values()
            .find(|runtime| runtime.operation_id == params.operation_id)
            .cloned()
        {
            if runtime.completion_tracking_lost {
                return Ok(TurnResult::Unavailable {
                    reason: "turn completion could not be confirmed; inspect the operation outcome"
                        .to_owned(),
                });
            }
            return Ok(TurnResult::Pending {
                codex_turn_id: runtime.native_result_id,
            });
        }

        match operation.state {
            OperationState::Prepared | OperationState::Dispatching => Ok(TurnResult::Pending {
                codex_turn_id: operation.native_result_id,
            }),
            OperationState::Uncertain => Ok(TurnResult::Unavailable {
                reason: "Codex did not confirm whether this turn was accepted".to_owned(),
            }),
            OperationState::Accepted => Ok(TurnResult::Unavailable {
                reason: "this daemon generation no longer has this turn's output".to_owned(),
            }),
        }
    }

    pub(in crate::coordinator) fn observe_native_agent_message(
        &self,
        thread_id: &str,
        native_turn_id: &str,
        response: &str,
    ) {
        let mut active = self
            .active_turn_operations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // The shared lock order is active guard -> bounded output cache. A
        // receipt reconciliation transfers between them without a delivery gap.
        if self
            .completed_turn_results
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .observe_late_output(thread_id, native_turn_id, response)
        {
            return;
        }
        if let Some(runtime) = active.get_mut(thread_id) {
            match runtime.native_result_id.as_deref() {
                None => runtime
                    .pending_observations
                    .observe_output(native_turn_id, response),
                Some(id) if id == native_turn_id => {
                    let (response, truncated) = bounded_turn_response(response);
                    runtime.agent_output = Some(ObservedAgentOutput {
                        native_turn_id: native_turn_id.to_owned(),
                        response,
                        truncated,
                    });
                }
                Some(_) => {}
            }
        }
    }

    pub(in crate::coordinator) fn observe_native_turn_completed(
        &self,
        thread_id: &str,
        native_turn_id: &str,
        status: TurnTerminalStatus,
    ) -> Result<(), StoreError> {
        let accepted = self
            .store
            .operation_by_native_result_id(native_turn_id)?
            .filter(|operation| {
                operation.kind == OperationKind::TurnStart
                    && operation.state == OperationState::Accepted
                    && operation.native_thread_id.as_deref() == Some(thread_id)
            });
        let mut active = self
            .active_turn_operations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if self
            .completed_turn_results
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .finish_output_delivery(thread_id, native_turn_id, status)
        {
            return Ok(());
        }
        // Accepted native IDs outlive runtime guards and the answer cache.
        // Never add an old operation's completion to a newer start's buffer.
        // Keep our own pre-ack buffering/binding path unchanged, including its
        // handling of contradictory early observations.
        if let Some(operation) = accepted
            && active
                .get(thread_id)
                .is_none_or(|runtime| runtime.id != operation.id)
        {
            drop(active);
            self.store.record_operation_completion(
                &operation.id,
                thread_id,
                native_turn_id,
                status,
            )?;
            return Ok(());
        }
        let Some(runtime) = active.get_mut(thread_id) else {
            return Ok(());
        };
        let completed = match runtime.native_result_id.as_deref() {
            Some(accepted_id) if accepted_id == native_turn_id => true,
            None => {
                // JSON-RPC notifications can overtake the turn/start response.
                // Keep distinct candidates until that response confirms the id.
                runtime
                    .pending_observations
                    .observe_completion(native_turn_id, status);
                false
            }
            Some(_) => false,
        };
        let completed = completed.then(|| active.remove(thread_id)).flatten();
        drop(active);
        if let Some(runtime) = completed {
            self.remember_completed_turn(runtime, native_turn_id, status)?;
        }
        Ok(())
    }

    pub(in crate::coordinator) fn bind_runtime_turn_result(
        &self,
        thread_id: &str,
        operation_id: &str,
        native_id: &str,
    ) -> Result<(), StoreError> {
        let mut active = self
            .active_turn_operations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut completion = None;
        if let Some(runtime) = active.get_mut(thread_id)
            && runtime.id == operation_id
            && runtime
                .native_result_id
                .as_deref()
                .is_none_or(|existing| existing == native_id)
        {
            runtime.native_result_id = Some(native_id.to_owned());
            runtime.uncertain = false;
            let (observed, lost) =
                std::mem::take(&mut runtime.pending_observations).resolve(native_id);
            runtime.completion_tracking_lost = lost;
            runtime.agent_output = observed.output;
            completion = observed.completion;
        }
        let completed = completion.and_then(|completion| {
            active
                .remove(thread_id)
                .map(|runtime| (runtime, completion))
        });
        drop(active);
        if let Some((runtime, status)) = completed {
            self.remember_completed_turn(runtime, native_id, status)?;
        }
        Ok(())
    }

    fn remember_completed_turn(
        &self,
        runtime: RuntimeTurnOperation,
        native_turn_id: &str,
        status: TurnTerminalStatus,
    ) -> Result<(), StoreError> {
        let receipt = self.store.record_operation_completion(
            &runtime.id,
            &runtime.native_thread_id,
            native_turn_id,
            status,
        );
        self.cache_completed_turn(runtime, native_turn_id, status, None);
        receipt.map(|_| ())
    }

    pub(in crate::coordinator) fn reconcile_turn_receipt(&self, operation: &Operation) {
        let (Some(thread), Some(turn), Some(status)) = (
            &operation.native_thread_id,
            &operation.native_result_id,
            operation.terminal_status,
        ) else {
            return;
        };
        let mut active = self
            .active_turn_operations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let matches = active.get(thread).is_some_and(|runtime| {
            runtime.id == operation.id
                && runtime.workspace_id == operation.workspace_id
                && runtime.native_thread_id == *thread
                && runtime
                    .native_result_id
                    .as_deref()
                    .is_none_or(|id| id == turn)
        });
        let completed = matches.then(|| active.remove(thread)).flatten();
        if let Some(runtime) = completed {
            let already_delivered = runtime.pending_observations.completion(turn) == Some(status);
            let until = (!already_delivered).then(|| Instant::now() + OUTPUT_DRAIN_TIMEOUT);
            self.cache_completed_turn(runtime, turn, status, until);
        }
        drop(active);
    }

    fn cache_completed_turn(
        &self,
        mut runtime: RuntimeTurnOperation,
        native_turn_id: &str,
        status: TurnTerminalStatus,
        output_pending_until: Option<Instant>,
    ) {
        let output = runtime
            .agent_output
            .filter(|output| output.native_turn_id == native_turn_id)
            .or_else(|| {
                std::mem::take(&mut runtime.pending_observations)
                    .resolve(native_turn_id)
                    .0
                    .output
            });
        self.completed_turn_results
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(
                runtime.operation_id,
                CompletedTurnResult {
                    codex_thread_id: runtime.native_thread_id,
                    codex_turn_id: native_turn_id.to_owned(),
                    status,
                    response: output.as_ref().map(|output| output.response.clone()),
                    response_truncated: output.is_some_and(|output| output.truncated),
                    output_pending_until,
                },
            );
    }
}

pub(in crate::coordinator) fn bounded_turn_response(response: &str) -> (String, bool) {
    bounded_response(response, MAX_TURN_RESPONSE_BYTES)
}

pub(super) fn bounded_response(response: &str, budget: usize) -> (String, bool) {
    if response.len() <= budget {
        return (response.to_owned(), false);
    }
    let mut end = budget;
    while !response.is_char_boundary(end) {
        end -= 1;
    }
    (response[..end].to_owned(), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_cache_really_evicts_the_oldest_terminal_entry() {
        let mut cache = TurnResultRegistry::default();
        for index in 0..=MAX_COMPLETED_TURN_RESULTS {
            cache.insert(
                format!("operation-{index}"),
                CompletedTurnResult {
                    codex_thread_id: "thread".to_owned(),
                    codex_turn_id: format!("turn-{index}"),
                    status: TurnTerminalStatus::Completed,
                    response: Some("bounded response".to_owned()),
                    response_truncated: false,
                    output_pending_until: None,
                },
            );
        }
        assert!(cache.get("operation-0").is_none());
        assert!(cache.get("operation-256").is_some());
        assert_eq!(cache.entries.len(), MAX_COMPLETED_TURN_RESULTS);
    }
}
