use super::*;

#[test]
fn v20_upgrade_preserves_workspaces_without_inventing_deletion_receipts() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("upgrade.db");
    let workspace = {
        let store = Store::open(&path).unwrap();
        let repository = repository(Path::new("/tmp/synthetic-v20-repository"));
        store.register_repository(&repository).unwrap();
        let workspace = ready_workspace(&store, &repository.id, "retained");
        store
            .lock()
            .unwrap()
            .execute_batch("DROP TABLE workspace_deletions; PRAGMA user_version=20;")
            .unwrap();
        workspace
    };
    let store = Store::open(&path).unwrap();
    assert_eq!(
        store.workspace_by_id(&workspace.id).unwrap().unwrap(),
        workspace
    );
    assert!(
        store
            .workspace_deletion("never-recorded")
            .unwrap()
            .is_none()
    );
    let connection = store.lock().unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        SUPPORTED_SCHEMA_VERSION
    );
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM workspace_deletions", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        0
    );
}
