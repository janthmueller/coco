use rusqlite::{Connection, params};

use crate::store::{Store, StoreError};

impl Store {
    /// Save a known native identity before a first send can be accepted or
    /// become uncertain. This is exclusion evidence, not a confirmed workspace
    /// binding, turn acceptance, or permission to resume an empty thread.
    pub(crate) fn remember_worker_mcp_thread(
        &self,
        workspace_id: &str,
        thread_id: &str,
    ) -> Result<(), StoreError> {
        remember_worker_thread(&*self.lock()?, workspace_id, thread_id)
    }
}

pub(in crate::store) fn remember_worker_thread(
    connection: &Connection,
    workspace_id: &str,
    thread_id: &str,
) -> Result<(), StoreError> {
    connection.execute(
        "INSERT OR IGNORE INTO worker_mcp_threads(thread_id, workspace_id)
         SELECT ?1, w.id FROM workspaces w
         LEFT JOIN workspace_creations c ON c.workspace_id = w.id
         WHERE w.id = ?2 AND (w.worker_mcp_bound = 1 OR c.worker_mcp_bound = 1
             OR json_type(c.receipt_json, '$.workerMcp') = 'object')",
        params![thread_id, workspace_id],
    )?;
    Ok(())
}
