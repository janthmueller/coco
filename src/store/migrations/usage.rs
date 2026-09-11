use rusqlite::Connection;

use super::StoreError;

pub(super) fn migrate(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "BEGIN IMMEDIATE;
         CREATE TABLE IF NOT EXISTS workspace_token_usage (
            workspace_id TEXT PRIMARY KEY REFERENCES workspaces(id) ON DELETE CASCADE,
            thread_id TEXT NOT NULL,
            checkpoint_json TEXT NOT NULL CHECK (json_valid(checkpoint_json)),
            updated_at_ms INTEGER NOT NULL
         );
         PRAGMA user_version = 14;
         COMMIT;",
    )?;
    Ok(())
}
