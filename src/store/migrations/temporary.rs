use rusqlite::Connection;

use super::StoreError;

pub(super) fn migrate(connection: &Connection) -> Result<(), StoreError> {
    let result = (|| -> Result<(), StoreError> {
        connection.execute_batch("BEGIN IMMEDIATE;")?;
        if !super::table_has_columns(connection, "workspaces", &["temporary_cleanup_json"])? {
            connection
                .execute_batch("ALTER TABLE workspaces ADD COLUMN temporary_cleanup_json TEXT;")?;
        }
        connection.execute_batch("PRAGMA user_version = 18; COMMIT;")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = connection.execute_batch("ROLLBACK;");
    }
    result
}
