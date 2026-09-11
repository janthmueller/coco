use rusqlite::Connection;

use super::StoreError;

pub(super) fn migrate(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "BEGIN IMMEDIATE;
         CREATE TABLE IF NOT EXISTS workspace_resource_policies (
            workspace_id TEXT PRIMARY KEY REFERENCES workspaces(id) ON DELETE CASCADE,
            revision INTEGER NOT NULL CHECK (revision > 0),
            policy_json TEXT NOT NULL CHECK (json_valid(policy_json)),
            updated_at_ms INTEGER NOT NULL
         );
         PRAGMA user_version = 13;
         COMMIT;",
    )?;
    Ok(())
}
