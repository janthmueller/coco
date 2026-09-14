use rusqlite::Connection;

use super::StoreError;

pub(super) fn migrate(connection: &Connection) -> Result<(), StoreError> {
    let result = (|| -> Result<(), StoreError> {
        connection.execute_batch("BEGIN IMMEDIATE;")?;
        if !super::table_has_columns(connection, "repositories", &["is_registered"])? {
            connection.execute_batch(
                "ALTER TABLE repositories ADD COLUMN is_registered INTEGER NOT NULL
                    DEFAULT 1 CHECK (is_registered IN (0, 1));",
            )?;
        }
        connection.execute_batch("PRAGMA user_version = 15; COMMIT;")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = connection.execute_batch("ROLLBACK;");
    }
    result
}
