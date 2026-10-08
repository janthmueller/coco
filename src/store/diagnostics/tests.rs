use super::*;
use crate::store::tests::{ready_workspace, repository};

#[test]
fn production_diagnostics_use_an_independent_read_only_connection_even_when_store_is_locked() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("state.db");
    let store = Arc::new(Store::open(&path).unwrap());
    store
        .register_repository(&repository(temporary.path()))
        .unwrap();
    let connection = store.connection.lock().unwrap();
    let changes = connection.total_changes();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let snapshot = runtime.block_on(store.diagnostic_snapshot()).unwrap();
    assert!(snapshot.integrity_ok);
    assert_eq!(snapshot.repositories.len(), 1);
    assert_eq!(connection.total_changes(), changes);
}

#[tokio::test]
async fn diagnostics_do_not_migrate_or_repair_existing_state() {
    let temporary = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(temporary.path().join("state.db")).unwrap());
    store
        .connection
        .lock()
        .unwrap()
        .execute_batch("PRAGMA user_version = 14")
        .unwrap();
    let snapshot = store.diagnostic_snapshot().await.unwrap();
    assert_eq!(snapshot.schema_version, 14);
    let version: i64 = store
        .connection
        .lock()
        .unwrap()
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 14);
}

#[tokio::test]
async fn binding_collections_are_capped_without_losing_their_total_count() {
    let store = Arc::new(Store::in_memory().unwrap());
    let repository = store
        .register_repository(&repository(PathBuf::from("/tmp/doctor-test").as_path()))
        .unwrap();
    for index in 0..MAX_WORKSPACES + 2 {
        ready_workspace(&store, &repository.id, &format!("sample-{index}"));
    }
    let snapshot = store.diagnostic_snapshot().await.unwrap();
    assert_eq!(snapshot.workspaces.len(), MAX_WORKSPACES);
    assert_eq!(snapshot.workspace_count, MAX_WORKSPACES + 2);
    assert_eq!(snapshot.repository_count, 1);
}
