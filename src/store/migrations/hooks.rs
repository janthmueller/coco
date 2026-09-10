use rusqlite::Connection;

use super::StoreError;

pub(super) fn migrate(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "BEGIN IMMEDIATE;
         CREATE TABLE IF NOT EXISTS hook_events (
            sequence INTEGER PRIMARY KEY AUTOINCREMENT,
            id TEXT NOT NULL UNIQUE,
            kind TEXT NOT NULL CHECK (kind IN (
                'signal.emitted', 'workspace.created', 'workspace.closed',
                'workspace.reopened', 'workspace.deleted'
            )),
            repository_id TEXT NOT NULL,
            workspace_id TEXT NOT NULL,
            occurred_at_ms INTEGER NOT NULL,
            body_json TEXT NOT NULL CHECK (json_valid(body_json))
         );
         CREATE TABLE IF NOT EXISTS hook_deliveries (
            id TEXT PRIMARY KEY,
            event_id TEXT NOT NULL REFERENCES hook_events(id) ON DELETE CASCADE,
            hook_id TEXT NOT NULL,
            definition_hash TEXT NOT NULL,
            state TEXT NOT NULL CHECK (state IN (
                'pending', 'running', 'succeeded', 'failed', 'cancelled'
            )),
            attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
            next_attempt_at_ms INTEGER,
            started_at_ms INTEGER,
            finished_at_ms INTEGER,
            last_error_message TEXT,
            created_at_ms INTEGER NOT NULL,
            UNIQUE(event_id, hook_id)
         );
         CREATE INDEX IF NOT EXISTS hook_deliveries_pending_idx
            ON hook_deliveries(state, next_attempt_at_ms, created_at_ms);
         PRAGMA user_version = 11;
         COMMIT;",
    )?;
    Ok(())
}
