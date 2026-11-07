use super::{Message, RelayControl, ThreadBinding, message_json};
use crate::protocol::WorkspaceAttachDisarmCleanupParams;
use anyhow::{Context, Result};
use serde_json::Value;

/// Temporary cleanup assumes a single native conversation. Extra traffic must
/// consume that permission durably *before* it can reach the native server.
pub(super) struct CleanupGuard {
    enabled: bool,
    original_thread: Option<String>,
    fresh_start_seen: bool,
    disarmed: bool,
}

impl CleanupGuard {
    pub(super) fn new(binding: ThreadBinding) -> Self {
        Self {
            enabled: binding.temporary,
            original_thread: binding.thread_id,
            fresh_start_seen: false,
            disarmed: false,
        }
    }

    pub(super) fn disarmed(&self) -> bool {
        self.disarmed
    }

    pub(super) async fn before_forward(
        &mut self,
        message: &Message,
        fresh_candidate: Option<&str>,
        control: &RelayControl,
    ) -> Result<()> {
        if !self.enabled || self.disarmed {
            return Ok(());
        }
        let Some(frame) = message_json(message) else {
            return Ok(());
        };
        if !self.uses_additional_conversation(&frame, fresh_candidate) {
            return Ok(());
        }
        control
            .client
            .request(WorkspaceAttachDisarmCleanupParams {
                workspace_id: control.workspace_id.clone(),
                lease_id: control.lease_id.clone(),
            })
            .await
            .context("could not safely keep the workspace for another conversation")?;
        self.disarmed = true;
        Ok(())
    }

    fn uses_additional_conversation(&mut self, frame: &Value, candidate: Option<&str>) -> bool {
        let original = self.original_thread.as_deref().or(candidate);
        match frame.get("method").and_then(Value::as_str) {
            Some("thread/start") => {
                if original.is_some() || self.fresh_start_seen || frame.get("id").is_none() {
                    return true;
                }
                self.fresh_start_seen = true;
                false
            }
            Some("thread/fork") => true,
            Some(_) => frame
                .pointer("/params/threadId")
                .and_then(Value::as_str)
                .is_some_and(|thread| Some(thread) != original),
            None => false,
        }
    }
}

#[cfg(test)]
mod tests;
