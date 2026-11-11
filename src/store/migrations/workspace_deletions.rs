use rusqlite::Connection;

use super::StoreError;

pub(super) fn migrate(connection: &Connection) -> Result<(), StoreError> {
    let result = connection.execute_batch(
        "BEGIN IMMEDIATE;
         CREATE TABLE IF NOT EXISTS workspace_deletions (
            operation_id TEXT PRIMARY KEY,
            repository_id TEXT NOT NULL,
            workspace_id TEXT NOT NULL,
            associated INTEGER NOT NULL DEFAULT 0 CHECK (associated IN (0, 1)),
            receipt_json TEXT NOT NULL CHECK (json_valid(receipt_json))
         );
         CREATE UNIQUE INDEX IF NOT EXISTS workspace_deletions_active_target
            ON workspace_deletions(workspace_id) WHERE associated = 1;
         PRAGMA user_version = 21;
         COMMIT;",
    );
    if result.is_err() {
        let _ = connection.execute_batch("ROLLBACK;");
    }
    result.map_err(StoreError::from)
}
