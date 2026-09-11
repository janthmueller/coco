use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use tracing::{debug, warn};

use super::Coordinator;
use crate::codex::CodexEvent;
use crate::domain::Workspace;
use crate::domain::usage::{
    TokenUsageBreakdown, WORKSPACE_TOKEN_USAGE_SCHEMA_VERSION, WorkspaceTokenUsageCheckpoint,
};
use crate::protocol::TurnTerminalStatus;
use crate::store::StoreError;

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
        self.clear_jump_leases();
        self.clear_file_change_previews();
        self.fail_pending_compactions();
        self.clear_runtime_turns();
        let orphaned = self.orphan_open_runtime_decisions();
        if orphaned > 0 {
            warn!(orphaned, "orphaned decisions after App Server disconnect");
        }
        let uncertain = self.store.mark_unconfirmed_operations_uncertain()?;
        let stale = self.store.mark_thread_statuses_stale()?;
        Ok(stale + uncertain)
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
            self.resolve_decision_by_native_request(thread_id, request_id);
            return Ok(());
        }
        if method == "thread/status/changed" {
            // Passive and mutating reads project current status directly from
            // thread/read. Do not recreate that native truth in SQLite.
            return Ok(());
        }
        if method == "thread/tokenUsage/updated" {
            return self.record_thread_token_usage(params);
        }
        let Some(workspace) = self.workspace_for_codex_params(&params)? else {
            debug!(method, "ignoring uncorrelated Codex notification");
            return Ok(());
        };
        if self.observe_pending_compaction(method, &params) {
            return Ok(());
        }
        if method == "turn/started" {
            // The notification has no clientUserMessageId, so it cannot
            // safely prove which client dispatched the turn. Native reads
            // remain authoritative for externally started turns.
            return Ok(());
        }
        if (method == "item/started"
            && params.pointer("/item/type").and_then(Value::as_str) == Some("fileChange"))
            || method == "item/fileChange/patchUpdated"
        {
            self.cache_file_change_preview(&params);
        }
        if method == "item/completed" {
            self.forget_file_change_preview(&params);
        }
        match method {
            "turn/completed" => self.record_turn_completed(&workspace, &params)?,
            "item/completed"
                if params.pointer("/item/type").and_then(Value::as_str) == Some("agentMessage") =>
            {
                let Some(thread_id) = params
                    .get("threadId")
                    .and_then(Value::as_str)
                    .or(workspace.codex_thread_id.as_deref())
                else {
                    return Ok(());
                };
                let Some(native_turn_id) = params.get("turnId").and_then(Value::as_str) else {
                    warn!(workspace_id = %workspace.id, "ignoring agent output without a native turn id");
                    return Ok(());
                };
                let Some(text) = params.pointer("/item/text").and_then(Value::as_str) else {
                    return Ok(());
                };
                self.observe_native_agent_message(thread_id, native_turn_id, text);
            }
            _ => {}
        }
        Ok(())
    }

    fn record_thread_token_usage(&self, params: Value) -> Result<(), StoreError> {
        let notification = match serde_json::from_value::<TokenUsageUpdatedWire>(params) {
            Ok(notification) => notification,
            Err(source) => {
                warn!(%source, "ignoring an invalid thread token-usage notification");
                return Ok(());
            }
        };
        let Some(workspace) = self.store.workspace_by_thread_id(&notification.thread_id)? else {
            debug!(
                thread_id = notification.thread_id,
                "ignoring token usage for an unbound Codex thread"
            );
            return Ok(());
        };
        let checkpoint = WorkspaceTokenUsageCheckpoint {
            schema_version: WORKSPACE_TOKEN_USAGE_SCHEMA_VERSION,
            thread_id: notification.thread_id,
            turn_id: notification.turn_id,
            total: notification.token_usage.total,
            last: notification.token_usage.last,
            model_context_window: notification.token_usage.model_context_window,
            runtime_generation: self.runtime_generation.clone(),
            observed_at_ms: Utc::now().timestamp_millis(),
        };
        if let Err(source) = checkpoint.validate() {
            warn!(workspace_id = %workspace.id, %source, "ignoring an invalid token-usage checkpoint");
            return Ok(());
        }
        if self
            .store
            .observe_workspace_token_usage(&workspace.id, &checkpoint)?
        {
            self.invalidate_thread_cost(&checkpoint.thread_id);
        } else {
            debug!(
                workspace_id = %workspace.id,
                thread_id = checkpoint.thread_id,
                "ignored a regressing cumulative token-usage notification"
            );
        }
        Ok(())
    }

    fn record_turn_completed(
        &self,
        workspace: &Workspace,
        params: &Value,
    ) -> Result<(), StoreError> {
        let status = params
            .pointer("/turn/status")
            .and_then(Value::as_str)
            .unwrap_or("failed");
        let status = match status {
            "completed" => TurnTerminalStatus::Completed,
            "interrupted" => TurnTerminalStatus::Interrupted,
            "failed" => TurnTerminalStatus::Failed,
            _ => {
                warn!(status, "ignoring non-terminal turn/completed payload");
                return Ok(());
            }
        };
        let Some(thread_id) = params
            .get("threadId")
            .and_then(Value::as_str)
            .or(workspace.codex_thread_id.as_deref())
        else {
            return Ok(());
        };
        let Some(native_turn_id) = params.pointer("/turn/id").and_then(Value::as_str) else {
            warn!(workspace_id = %workspace.id, "ignoring turn completion without a native turn id");
            return Ok(());
        };
        self.observe_native_turn_completed(thread_id, native_turn_id, status);
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
        let operation_id = self.operation_id_for_codex_params(&workspace, &params)?;
        if self.capture_decision_request(id, method, &params, &workspace, operation_id.as_deref()) {
            return Ok(());
        }
        warn!(
            workspace_id = %workspace.id,
            method,
            "leaving an unsupported Codex server request unanswered"
        );
        Ok(())
    }

    fn workspace_for_codex_params(&self, params: &Value) -> Result<Option<Workspace>, StoreError> {
        let thread_id = params
            .get("threadId")
            .or_else(|| params.pointer("/thread/id"))
            .and_then(Value::as_str);
        match thread_id {
            Some(thread_id) => {
                if let Some(workspace) = self.store.workspace_by_thread_id(thread_id)? {
                    return Ok(Some(workspace));
                }
                let Some(workspace_id) = self.runtime_workspace_id_for_thread(thread_id) else {
                    return Ok(None);
                };
                self.store.workspace_by_id(&workspace_id)
            }
            None => Ok(None),
        }
    }

    fn operation_id_for_codex_params(
        &self,
        workspace: &Workspace,
        params: &Value,
    ) -> Result<Option<String>, StoreError> {
        let native_turn_id = params
            .get("turnId")
            .or_else(|| params.pointer("/turn/id"))
            .and_then(Value::as_str);
        let Some(native_turn_id) = native_turn_id else {
            return Ok(None);
        };
        Ok(self
            .store
            .operation_by_native_result_id(native_turn_id)?
            .and_then(|operation| (operation.workspace_id == workspace.id).then_some(operation.id)))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TokenUsageUpdatedWire {
    thread_id: String,
    turn_id: String,
    token_usage: ThreadTokenUsageWire,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadTokenUsageWire {
    total: TokenUsageBreakdown,
    last: TokenUsageBreakdown,
    #[serde(default)]
    model_context_window: Option<u64>,
}
