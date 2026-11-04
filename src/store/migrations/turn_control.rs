use rusqlite::Connection;

use super::StoreError;

pub(super) fn migrate(connection: &Connection) -> Result<(), StoreError> {
    let result = (|| -> Result<(), StoreError> {
        connection.execute_batch("BEGIN IMMEDIATE;")?;
        for (column, definition) in [
            ("native_thread_id", "TEXT"),
            (
                "terminal_status",
                "TEXT CHECK (terminal_status IN ('completed', 'interrupted', 'failed'))",
            ),
            ("terminal_observed_at_ms", "INTEGER"),
            ("interrupt_requested_at_ms", "INTEGER"),
        ] {
            if !super::table_has_columns(connection, "operations", &[column])? {
                connection.execute_batch(&format!(
                    "ALTER TABLE operations ADD COLUMN {column} {definition};"
                ))?;
            }
        }
        connection.execute_batch("PRAGMA user_version = 16; COMMIT;")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = connection.execute_batch("ROLLBACK;");
    }
    result
}
