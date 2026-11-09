use std::path::Path;

use serde_json::json;

use super::{CodexWorker, WorkerError};

impl CodexWorker {
    pub(super) async fn check_worker_mcp_slots(
        &self,
        cwd: &Path,
        slots: &[String],
    ) -> Result<(), WorkerError> {
        if self.client.server_version() != Some("0.166.0") {
            return Err(WorkerError::InvalidResponse(
                "verified worker MCP configuration version",
            ));
        }
        let response = self
            .client
            .request("config/read", json!({"cwd": cwd, "includeLayers": false}))
            .await
            .map_err(|_| WorkerError::InvalidResponse("worker MCP base configuration"))?;
        let config = response
            .get("config")
            .and_then(serde_json::Value::as_object)
            .ok_or(WorkerError::InvalidResponse(
                "worker MCP base configuration",
            ))?;
        if let Some(servers) = config.get("mcp_servers").filter(|value| !value.is_null()) {
            let servers = servers
                .as_object()
                .ok_or(WorkerError::InvalidResponse("native MCP server map"))?;
            if slots.iter().any(|slot| {
                servers.keys().any(|existing| {
                    crate::domain::setup::worker_slot_key(existing)
                        == crate::domain::setup::worker_slot_key(slot)
                })
            }) {
                return Err(WorkerError::InvalidResponse(
                    "worker MCP slot is already configured",
                ));
            }
        }
        Ok(())
    }
}
