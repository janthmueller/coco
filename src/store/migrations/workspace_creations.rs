use rusqlite::Connection;

use super::StoreError;

pub(super) fn migrate(connection: &Connection) -> Result<(), StoreError> {
    let result = (|| -> Result<(), StoreError> {
        connection.execute_batch("BEGIN IMMEDIATE;")?;
        if !super::table_has_columns(connection, "workspaces", &["worker_mcp_bound"])? {
            connection.execute_batch("ALTER TABLE workspaces ADD COLUMN worker_mcp_bound INTEGER NOT NULL DEFAULT 0 CHECK (worker_mcp_bound IN (0,1));")?;
        }
        connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS workspace_creations (
            operation_id TEXT PRIMARY KEY,
            repository_id TEXT NOT NULL,
            request_sha256 TEXT,
            workspace_id TEXT UNIQUE,
            worker_mcp_bound INTEGER NOT NULL DEFAULT 0 CHECK (worker_mcp_bound IN (0,1)),
            receipt_json TEXT NOT NULL CHECK (json_valid(receipt_json))
         );
         CREATE INDEX IF NOT EXISTS workspace_creations_repository_idx ON workspace_creations(repository_id);
         INSERT OR IGNORE INTO workspace_creations(operation_id, repository_id, workspace_id, receipt_json)
         SELECT w.create_operation_id, w.repository_id, w.id, json_object(
            'operationId', w.create_operation_id, 'requestSha256', NULL,
            'state', 'unavailable', 'repositoryId', w.repository_id,
            'repositoryPath', r.root_path, 'workspaceId', w.id, 'workspaceName', w.name,
            'baseSha', w.base_sha, 'worktreePath', w.worktree_path, 'branchName', w.branch_name,
            'profile', NULL, 'workerMcp', NULL, 'codexThreadId', w.codex_thread_id,
            'errorCode', NULL, 'createdAtMs', w.created_at_ms, 'updatedAtMs', w.updated_at_ms
         ) FROM workspaces w JOIN repositories r ON r.id = w.repository_id
         WHERE w.create_operation_id IS NOT NULL;
         PRAGMA user_version = 19;
         COMMIT;",
    )?;
        Ok(())
    })();
    if result.is_err() {
        let _ = connection.execute_batch("ROLLBACK;");
    }
    result
}
