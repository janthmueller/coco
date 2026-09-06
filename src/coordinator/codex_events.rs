use serde_json::{Value, json};
use tracing::{debug, warn};

use super::Coordinator;
use crate::codex::CodexEvent;
use crate::domain::{
    CodexThreadStatus, EventKind, EventSource, Turn, TurnPhase, Workspace, WorkspaceLifecycle,
};
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

    pub(crate) fn record_codex_disconnected(&self) -> Result<usize, StoreError> {
        self.clear_file_change_previews();
        self.fail_pending_compactions();
        let stale = self.store.mark_thread_statuses_stale()?;
        self.store
            .orphan_open_decisions(Some(&self.runtime_generation), "app_server_disconnected")?;
        Ok(stale)
    }

    fn record_codex_notification(&self, method: &str, params: Value) -> Result<(), StoreError> {
        if method == "serverRequest/resolved" {
            let Some(thread_id) = params.get("threadId").and_then(Value::as_str) else {
                warn!("ignoring serverRequest/resolved without a thread id");
                return Ok(());
            };
            let Some(request_id) = params.get("requestId") else {
                warn!("ignoring serverRequest/resolved without a request id");
                return Ok(());
            };
            self.store.resolve_decision_by_native_request(
                &self.runtime_generation,
                thread_id,
                request_id,
            )?;
            return Ok(());
        }
        let Some(workspace) = self.workspace_for_codex_params(&params)? else {
            debug!(method, "ignoring uncorrelated Codex notification");
            return Ok(());
        };
        if self.observe_pending_compaction(method, &params) {
            return Ok(());
        }
        if method == "turn/started" {
            return self.record_external_turn_started(&workspace, &params);
        }
        if method == "thread/status/changed" {
            return self.record_thread_status_changed(&workspace, &params);
        }
        if (method == "item/started"
            && params.pointer("/item/type").and_then(Value::as_str) == Some("fileChange"))
            || method == "item/fileChange/patchUpdated"
        {
            self.cache_file_change_preview(&params);
        }
        let turn = self.turn_for_codex_params(&workspace, &params)?;
        if method == "item/completed" {
            self.forget_file_change_preview(&params);
        }
        match method {
            "turn/completed" => self.record_turn_completed(&workspace, turn.as_ref(), &params)?,
            "item/completed"
                if params.pointer("/item/type").and_then(Value::as_str) == Some("agentMessage") =>
            {
                self.store.append_event(EventDraft {
                    workspace_id: Some(workspace.id),
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
                    &workspace,
                    turn.as_ref(),
                    EventKind::PlanUpdated,
                    method,
                    params,
                ))?;
            }
            "turn/diff/updated" => {
                self.store.append_event(codex_event_draft(
                    &workspace,
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
                    &workspace,
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

    fn record_turn_completed(
        &self,
        workspace: &Workspace,
        turn: Option<&Turn>,
        params: &Value,
    ) -> Result<(), StoreError> {
        let Some(turn) = turn else {
            warn!(workspace_id = %workspace.id, "ignoring turn completion without a correlated turn");
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
            &workspace.id,
            &turn.id,
            TurnCompletion {
                phase,
                error,
                completed_at_ms: None,
            },
            EventDraft::workspace(
                EventKind::TurnCompleted,
                EventSource::Codex,
                json!({"status": status}),
            ),
        )?;
        Ok(())
    }

    fn record_external_turn_started(
        &self,
        workspace: &Workspace,
        params: &Value,
    ) -> Result<(), StoreError> {
        let Some(codex_turn_id) = params.pointer("/turn/id").and_then(Value::as_str) else {
            warn!(workspace_id = %workspace.id, "ignoring turn/started without a turn id");
            return Ok(());
        };
        let pending = workspace
            .codex_thread_id
            .as_deref()
            .is_some_and(|thread_id| {
                self.pending_turn_threads
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .contains(thread_id)
            });
        if self.store.turn_by_codex_id(codex_turn_id)?.is_some() || pending {
            return Ok(());
        }
        if workspace.lifecycle != WorkspaceLifecycle::Ready || workspace.active_turn_id.is_some() {
            warn!(
                workspace_id = %workspace.id,
                phase = workspace.phase.as_str(),
                "ignoring an external turn for a workspace that cannot accept one"
            );
            return Ok(());
        }
        self.store.start_turn_with_event(
            &workspace.id,
            NewTurn {
                operation_id: None,
                client_message_id: format!("codex-external:{codex_turn_id}"),
                codex_turn_id: Some(codex_turn_id.to_owned()),
                started_at_ms: None,
            },
            EventDraft::workspace(
                EventKind::TurnStarted,
                EventSource::Codex,
                json!({"codexTurnId": codex_turn_id, "origin": "external_client"}),
            ),
        )?;
        Ok(())
    }

    fn record_thread_status_changed(
        &self,
        workspace: &Workspace,
        params: &Value,
    ) -> Result<(), StoreError> {
        let Some(status) = params.get("status").cloned() else {
            warn!(workspace_id = %workspace.id, "ignoring thread status notification without status");
            return Ok(());
        };
        let status = match serde_json::from_value::<CodexThreadStatus>(status) {
            Ok(status) => status.canonicalized(),
            Err(source) => {
                warn!(workspace_id = %workspace.id, %source, "ignoring invalid native thread status");
                return Ok(());
            }
        };
        self.store.observe_thread_status_with_event(
            &workspace.id,
            status.clone(),
            &self.runtime_generation,
            EventDraft {
                workspace_id: Some(workspace.id.clone()),
                turn_id: workspace.active_turn_id.clone(),
                kind: EventKind::ThreadStatusChanged,
                source: EventSource::Codex,
                source_method: Some("thread/status/changed".to_owned()),
                occurred_at_ms: None,
                payload: json!({"status": status}),
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
        let Some(workspace) = self.workspace_for_codex_params(&params)? else {
            warn!(method, "ignoring uncorrelated Codex server request");
            return Ok(());
        };
        let turn = self.turn_for_codex_params(&workspace, &params)?;
        if self.capture_decision_request(id, method, &params, &workspace, turn.as_ref())? {
            return Ok(());
        }
        let payload = json!({
            "method": method,
            "reason": params.get("reason"),
        });
        let event = EventDraft {
            workspace_id: Some(workspace.id.clone()),
            turn_id: turn.map(|turn| turn.id),
            kind: EventKind::ServerRequestReceived,
            source: EventSource::Codex,
            source_method: Some(method.to_owned()),
            occurred_at_ms: None,
            payload,
        };
        self.store.append_event(event)?;
        Ok(())
    }

    fn workspace_for_codex_params(&self, params: &Value) -> Result<Option<Workspace>, StoreError> {
        let thread_id = params
            .get("threadId")
            .or_else(|| params.pointer("/thread/id"))
            .and_then(Value::as_str);
        match thread_id {
            Some(thread_id) => self.store.workspace_by_thread_id(thread_id),
            None => Ok(None),
        }
    }

    fn turn_for_codex_params(
        &self,
        workspace: &Workspace,
        params: &Value,
    ) -> Result<Option<Turn>, StoreError> {
        let codex_turn_id = params
            .get("turnId")
            .or_else(|| params.pointer("/turn/id"))
            .and_then(Value::as_str);
        if let Some(codex_turn_id) = codex_turn_id
            && let Some(turn) = self.store.turn_by_codex_id(codex_turn_id)?
        {
            return Ok((turn.workspace_id == workspace.id).then_some(turn));
        }
        workspace
            .active_turn_id
            .as_deref()
            .map(|turn_id| self.store.turn_by_id(turn_id))
            .transpose()
            .map(Option::flatten)
    }
}

fn codex_event_draft(
    workspace: &Workspace,
    turn: Option<&Turn>,
    kind: EventKind,
    method: &str,
    payload: Value,
) -> EventDraft {
    EventDraft {
        workspace_id: Some(workspace.id.clone()),
        turn_id: turn.map(|turn| turn.id.clone()),
        kind,
        source: EventSource::Codex,
        source_method: Some(method.to_owned()),
        occurred_at_ms: None,
        payload,
    }
}
