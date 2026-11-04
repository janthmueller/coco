use rusqlite::Connection;

use super::StoreError;

pub(super) fn migrate(connection: &Connection) -> Result<(), StoreError> {
    let result = (|| -> Result<(), StoreError> {
        connection.execute_batch("BEGIN IMMEDIATE;")?;
        for (column, definition) in [
            (
                "interrupt_mode",
                "TEXT CHECK (interrupt_mode IN ('strict', 'bestEffort'))",
            ),
            (
                "interrupt_state",
                "TEXT CHECK (interrupt_state IN ('uncertain', 'requested', 'rejected'))",
            ),
        ] {
            if !super::table_has_columns(connection, "operations", &[column])? {
                connection.execute_batch(&format!(
                    "ALTER TABLE operations ADD COLUMN {column} {definition};"
                ))?;
            }
        }
        // Before modes existed, only strict attempts could be requested. The
        // old marker proves no acknowledgement; never promote it to requested.
        connection.execute_batch(
            "UPDATE operations SET interrupt_mode = 'strict', interrupt_state = 'uncertain'
             WHERE interrupt_requested_at_ms IS NOT NULL AND interrupt_mode IS NULL AND interrupt_state IS NULL;
             PRAGMA user_version = 17; COMMIT;",
        )?;
        Ok(())
    })();
    if result.is_err() {
        let _ = connection.execute_batch("ROLLBACK;");
    }
    result
}
