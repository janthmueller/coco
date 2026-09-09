use std::collections::{HashMap, VecDeque};

use super::{ObservedAgentOutput, ObservedTurnCompletion, RuntimeTurnOperation};
use crate::coordinator::{Coordinator, CoordinatorError, validate_operation_id};
use crate::protocol::{TurnResult, TurnResultParams, TurnTerminalStatus};
use crate::store::{OperationKind, OperationState};

const MAX_COMPLETED_TURN_RESULTS: usize = 256;
const MAX_COMPLETED_TURN_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
// One MiB remains below the 8 MiB local RPC frame even if JSON must escape
// every retained byte as a six-byte Unicode sequence.
pub(in crate::coordinator) const MAX_TURN_RESPONSE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompletedTurnResult {
    codex_turn_id: String,
    status: TurnTerminalStatus,
    response: Option<String>,
    response_truncated: bool,
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
        let Some(runtime) = active.get_mut(thread_id) else {
            return;
        };
        if runtime
            .native_result_id
            .as_deref()
            .is_none_or(|accepted| accepted == native_turn_id)
        {
            let (response, truncated) = bounded_turn_response(response);
            runtime.agent_output = Some(ObservedAgentOutput {
                native_turn_id: native_turn_id.to_owned(),
                response,
                truncated,
            });
        }
    }

    pub(in crate::coordinator) fn observe_native_turn_completed(
        &self,
        thread_id: &str,
        native_turn_id: &str,
        status: TurnTerminalStatus,
    ) {
        let mut active = self
            .active_turn_operations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(runtime) = active.get_mut(thread_id) else {
            return;
        };
        let completed = match runtime.native_result_id.as_deref() {
            Some(accepted_id) if accepted_id == native_turn_id => true,
            None => {
                // JSON-RPC notifications can overtake the turn/start response.
                // Keep this observation until that response confirms the id.
                runtime.completed_before_response = Some(ObservedTurnCompletion {
                    native_turn_id: native_turn_id.to_owned(),
                    status,
                });
                false
            }
            Some(_) => false,
        };
        let completed = completed.then(|| active.remove(thread_id)).flatten();
        drop(active);
        if let Some(runtime) = completed {
            self.remember_completed_turn(runtime, native_turn_id, status);
        }
    }

    pub(in crate::coordinator) fn bind_runtime_turn_result(
        &self,
        thread_id: &str,
        operation_id: &str,
        native_id: &str,
    ) {
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
            completion = runtime
                .completed_before_response
                .as_ref()
                .filter(|completed| completed.native_turn_id == native_id)
                .cloned();
        }
        let completed = completion.and_then(|completion| {
            active
                .remove(thread_id)
                .map(|runtime| (runtime, completion.status))
        });
        drop(active);
        if let Some((runtime, status)) = completed {
            self.remember_completed_turn(runtime, native_id, status);
        }
    }

    fn remember_completed_turn(
        &self,
        runtime: RuntimeTurnOperation,
        native_turn_id: &str,
        status: TurnTerminalStatus,
    ) {
        let output = runtime
            .agent_output
            .filter(|output| output.native_turn_id == native_turn_id);
        self.completed_turn_results
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(
                runtime.operation_id,
                CompletedTurnResult {
                    codex_turn_id: native_turn_id.to_owned(),
                    status,
                    response: output.as_ref().map(|output| output.response.clone()),
                    response_truncated: output.is_some_and(|output| output.truncated),
                },
            );
    }
}

pub(in crate::coordinator) fn bounded_turn_response(response: &str) -> (String, bool) {
    if response.len() <= MAX_TURN_RESPONSE_BYTES {
        return (response.to_owned(), false);
    }
    let mut end = MAX_TURN_RESPONSE_BYTES;
    while !response.is_char_boundary(end) {
        end -= 1;
    }
    (response[..end].to_owned(), true)
}
