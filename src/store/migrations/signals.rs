use super::StoreError;
use rusqlite::Connection;

pub(super) fn migrate(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "BEGIN IMMEDIATE;
         CREATE TABLE IF NOT EXISTS signal_types (
            repository_id TEXT NOT NULL REFERENCES repositories(id),
            name TEXT NOT NULL,
            version INTEGER NOT NULL CHECK (version > 0),
            body_json TEXT NOT NULL CHECK (json_valid(body_json)),
            PRIMARY KEY (repository_id, name, version)
         );
         CREATE TABLE IF NOT EXISTS signal_stream (
            singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
            stream_id TEXT NOT NULL,
            high_water INTEGER NOT NULL DEFAULT 0,
            expired_through INTEGER NOT NULL DEFAULT 0
         );
         INSERT OR IGNORE INTO signal_stream(singleton, stream_id)
            VALUES (1, lower(hex(randomblob(16))));
         CREATE TABLE IF NOT EXISTS signals (
            sequence INTEGER PRIMARY KEY,
            repository_id TEXT NOT NULL REFERENCES repositories(id),
            workspace_id TEXT NOT NULL,
            name TEXT NOT NULL,
            version INTEGER NOT NULL,
            idempotency_key TEXT NOT NULL,
            occurred_at_ms INTEGER NOT NULL,
            body_json TEXT NOT NULL CHECK (json_valid(body_json)),
            FOREIGN KEY (repository_id, name, version)
                REFERENCES signal_types(repository_id, name, version),
            UNIQUE (workspace_id, idempotency_key)
         );
         CREATE INDEX IF NOT EXISTS signals_scope_idx ON signals(repository_id, sequence);
         CREATE INDEX IF NOT EXISTS signals_rate_idx ON signals(workspace_id, occurred_at_ms);
         PRAGMA user_version = 10;
         COMMIT;",
    )?;
    Ok(())
}
