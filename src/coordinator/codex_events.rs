use serde_json::{Value, json};
use tracing::{debug, warn};

use super::Coordinator;
use crate::codex::CodexEvent;
use crate::domain::{EventKind, EventSource, Task, TaskPhase, Turn, TurnPhase};
use crate::store::{EventDraft, NewTurn, StoreError, TurnCompletion};

impl Coordinator {
    pub(crate) fn record_codex_event(&self, event: CodexEvent) -> Result<(), StoreError> {
        match event {
            CodexEvent::Notification { method, params } => {
                self.record_codex_notification(&method, params)
            }
            CodexEvent::ServerRequest { id, method, params } => {
                self.record_codex_server_request(id, &method, params)
            }
        }
    }

    fn record_codex_notification(&self, method: &str, params: Value) -> Result<(), StoreError> {
        let Some(task) = self.task_for_codex_params(&params)? else {
            debug!(method, "ignoring uncorrelated Codex notification");
            return Ok(());
        };
        if method == "turn/started" {
            return self.record_external_turn_started(&task, &params);
        }
        if method == "thread/status/changed" {
            return self.record_thread_status_changed(&task, &params);
        }
        let turn = self.turn_for_codex_params(&task, &params)?;
        match method {
            "turn/completed" => {
                let Some(turn) = turn else {
                    warn!(task_id = %task.id, "ignoring turn completion without a correlated turn");
                    return Ok(());
                };
                if matches!(
                    turn.phase,
                    TurnPhase::Completed | TurnPhase::Failed | TurnPhase::Interrupted
                ) {
                    return Ok(());
                }
                let status = params
                    .pointer("/turn/status")
                    .and_then(Value::as_str)
                    .unwrap_or("failed");
                let phase = match status {
                    "completed" => TurnPhase::Completed,
                    "interrupted" => TurnPhase::Interrupted,
                    "failed" => TurnPhase::Failed,
                    _ => {
                        warn!(status, "ignoring non-terminal turn/completed payload");
                        return Ok(());
                    }
                };
                let error = params
                    .pointer("/turn/error")
                    .filter(|value| !value.is_null())
                    .cloned();
                self.store.complete_turn_with_event(
                    &task.id,
                    &turn.id,
                    TurnCompletion {
                        phase,
                        error,
                        completed_at_ms: None,
                    },
                    EventDraft::task(
                        EventKind::TurnCompleted,
                        EventSource::Codex,
                        json!({"status": status}),
                    ),
                )?;
            }
            "item/completed"
                if params.pointer("/item/type").and_then(Value::as_str) == Some("agentMessage") =>
            {
                self.store.append_event(EventDraft {
                    task_id: Some(task.id),
                    turn_id: turn.map(|turn| turn.id),
                    kind: EventKind::AgentMessageCompleted,
                    source: EventSource::Codex,
                    source_method: Some(method.to_owned()),
                    occurred_at_ms: params.get("completedAtMs").and_then(Value::as_i64),
                    payload: json!({
                        "itemId": params.pointer("/item/id"),
                        "text": params.pointer("/item/text"),
                    }),
                })?;
            }
            "turn/plan/updated" => {
                self.store.append_event(codex_event_draft(
                    &task,
                    turn.as_ref(),
                    EventKind::PlanUpdated,
                    method,
                    params,
                ))?;
            }
            "turn/diff/updated" => {
                self.store.append_event(codex_event_draft(
                    &task,
                    turn.as_ref(),
                    EventKind::DiffUpdated,
                    method,
                    params,
                ))?;
            }
            "error"
                if !params
                    .get("willRetry")
                    .and_then(Value::as_bool)
                    .unwrap_or(false) =>
            {
                self.store.append_event(codex_event_draft(
                    &task,
                    turn.as_ref(),
                    EventKind::AgentFailed,
                    method,
                    params,
                ))?;
            }
            _ => {}
        }
        Ok(())
    }

    fn record_external_turn_started(&self, task: &Task, params: &Value) -> Result<(), StoreError> {
        let Some(codex_turn_id) = params.pointer("/turn/id").and_then(Value::as_str) else {
            warn!(task_id = %task.id, "ignoring turn/started without a turn id");
            return Ok(());
        };
        let pending = task.codex_thread_id.as_deref().is_some_and(|thread_id| {
            self.pending_turn_threads
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .contains(thread_id)
        });
        if self.store.turn_by_codex_id(codex_turn_id)?.is_some() || pending {
            return Ok(());
        }
        if task.phase != TaskPhase::Idle {
            warn!(
                task_id = %task.id,
                phase = task.phase.as_str(),
                "ignoring an external turn for a task that is not idle"
            );
            return Ok(());
        }
        self.store.start_turn_with_event(
            &task.id,
            &[TaskPhase::Idle],
            NewTurn {
                operation_id: None,
                client_message_id: format!("codex-external:{codex_turn_id}"),
                codex_turn_id: Some(codex_turn_id.to_owned()),
                started_at_ms: None,
            },
            EventDraft::task(
                EventKind::TurnStarted,
                EventSource::Codex,
                json!({"codexTurnId": codex_turn_id, "origin": "external_client"}),
            ),
        )?;
        Ok(())
    }

    fn record_thread_status_changed(&self, task: &Task, params: &Value) -> Result<(), StoreError> {
        let status = params.pointer("/status/type").and_then(Value::as_str);
        if status != Some("active") {
            return Ok(());
        }
        let flags = params
            .pointer("/status/activeFlags")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let next = if flags
            .iter()
            .any(|flag| flag.as_str() == Some("waitingOnUserInput"))
        {
            TaskPhase::WaitingForInput
        } else if flags
            .iter()
            .any(|flag| flag.as_str() == Some("waitingOnApproval"))
        {
            TaskPhase::WaitingForApproval
        } else {
            TaskPhase::Active
        };
        if task.phase == next {
            return Ok(());
        }
        let active_or_waiting = |phase| {
            matches!(
                phase,
                TaskPhase::Active | TaskPhase::WaitingForApproval | TaskPhase::WaitingForInput
            )
        };
        let allowed = active_or_waiting(task.phase) && active_or_waiting(next);
        if !allowed {
            return Ok(());
        }
        let kind = if next == TaskPhase::Active {
            EventKind::ApprovalResolved
        } else {
            EventKind::ApprovalRequested
        };
        self.store.transition_task_with_event(
            &task.id,
            task.phase,
            next,
            None,
            EventDraft {
                task_id: Some(task.id.clone()),
                turn_id: task.active_turn_id.clone(),
                kind,
                source: EventSource::Codex,
                source_method: Some("thread/status/changed".to_owned()),
                occurred_at_ms: None,
                payload: json!({"status": params.get("status")}),
            },
        )?;
        Ok(())
    }

    fn record_codex_server_request(
        &self,
        id: Value,
        method: &str,
        params: Value,
    ) -> Result<(), StoreError> {
        let Some(task) = self.task_for_codex_params(&params)? else {
            warn!(method, "ignoring uncorrelated Codex server request");
            return Ok(());
        };
        let turn = self.turn_for_codex_params(&task, &params)?;
        let waiting_phase = if method.contains("requestUserInput") {
            TaskPhase::WaitingForInput
        } else {
            TaskPhase::WaitingForApproval
        };
        let payload = json!({
            "requestId": id,
            "method": method,
            "reason": params.get("reason"),
        });
        let event = EventDraft {
            task_id: Some(task.id.clone()),
            turn_id: turn.map(|turn| turn.id),
            kind: EventKind::ApprovalRequested,
            source: EventSource::Codex,
            source_method: Some(method.to_owned()),
            occurred_at_ms: None,
            payload,
        };
        if task.phase == TaskPhase::Active {
            self.store.transition_task_with_event(
                &task.id,
                TaskPhase::Active,
                waiting_phase,
                None,
                event,
            )?;
        } else {
            self.store.append_event(event)?;
        }
        Ok(())
    }

    fn task_for_codex_params(&self, params: &Value) -> Result<Option<Task>, StoreError> {
        let thread_id = params
            .get("threadId")
            .or_else(|| params.pointer("/thread/id"))
            .and_then(Value::as_str);
        match thread_id {
            Some(thread_id) => self.store.task_by_thread_id(thread_id),
            None => Ok(None),
        }
    }

    fn turn_for_codex_params(
        &self,
        task: &Task,
        params: &Value,
    ) -> Result<Option<Turn>, StoreError> {
        let codex_turn_id = params
            .get("turnId")
            .or_else(|| params.pointer("/turn/id"))
            .and_then(Value::as_str);
        if let Some(codex_turn_id) = codex_turn_id
            && let Some(turn) = self.store.turn_by_codex_id(codex_turn_id)?
        {
            return Ok((turn.task_id == task.id).then_some(turn));
        }
        task.active_turn_id
            .as_deref()
            .map(|turn_id| self.store.turn_by_id(turn_id))
            .transpose()
            .map(Option::flatten)
    }
}

fn codex_event_draft(
    task: &Task,
    turn: Option<&Turn>,
    kind: EventKind,
    method: &str,
    payload: Value,
) -> EventDraft {
    EventDraft {
        task_id: Some(task.id.clone()),
        turn_id: turn.map(|turn| turn.id.clone()),
        kind,
        source: EventSource::Codex,
        source_method: Some(method.to_owned()),
        occurred_at_ms: None,
        payload,
    }
}
