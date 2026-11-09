use rusqlite::Connection;

use super::StoreError;

pub(super) fn migrate(connection: &Connection) -> Result<(), StoreError> {
    let result = connection.execute_batch(
        "BEGIN IMMEDIATE;
         CREATE TABLE IF NOT EXISTS worker_mcp_threads (
            thread_id TEXT PRIMARY KEY,
            workspace_id TEXT NOT NULL
         );
         INSERT OR IGNORE INTO worker_mcp_threads(thread_id, workspace_id)
         SELECT w.codex_thread_id, w.id FROM workspaces w
         LEFT JOIN workspace_creations c ON c.workspace_id = w.id
         WHERE w.codex_thread_id IS NOT NULL AND (w.worker_mcp_bound = 1
            OR c.worker_mcp_bound = 1 OR json_type(c.receipt_json, '$.workerMcp') = 'object');
         INSERT OR IGNORE INTO worker_mcp_threads(thread_id, workspace_id)
         SELECT o.native_thread_id, o.workspace_id FROM operations o
         JOIN workspaces w ON w.id = o.workspace_id
         LEFT JOIN workspace_creations c ON c.workspace_id = w.id
         WHERE o.native_thread_id IS NOT NULL AND (w.worker_mcp_bound = 1
            OR c.worker_mcp_bound = 1 OR json_type(c.receipt_json, '$.workerMcp') = 'object');
         INSERT OR IGNORE INTO worker_mcp_threads(thread_id, workspace_id)
         SELECT json_extract(receipt_json, '$.codexThreadId'), workspace_id
         FROM workspace_creations WHERE workspace_id IS NOT NULL
            AND json_type(receipt_json, '$.codexThreadId') = 'text'
            AND (worker_mcp_bound = 1 OR json_type(receipt_json, '$.workerMcp') = 'object');
         PRAGMA user_version = 20;
         COMMIT;",
    );
    if result.is_err() {
        let _ = connection.execute_batch("ROLLBACK;");
    }
    result.map_err(StoreError::from)
}
