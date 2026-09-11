use rusqlite::Connection;

use super::StoreError;

pub(super) fn migrate(connection: &Connection) -> Result<(), StoreError> {
    let result = (|| -> Result<(), StoreError> {
        connection.execute_batch("BEGIN IMMEDIATE;")?;
        for column in ["delete_discard_unretained_commits", "delete_from_open"] {
            if !super::table_has_columns(connection, "workspaces", &[column])? {
                connection.execute_batch(&format!(
                    "ALTER TABLE workspaces ADD COLUMN {column} INTEGER NOT NULL DEFAULT 0 CHECK ({column} IN (0, 1));"
                ))?;
            }
        }
        connection.execute_batch("PRAGMA user_version = 12; COMMIT;")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = connection.execute_batch("ROLLBACK;");
    }
    result
}
