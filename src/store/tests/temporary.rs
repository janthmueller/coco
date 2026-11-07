use super::*;
use crate::domain::TemporaryWorkspacePolicy;

fn create(store: &Store, keep_thread: bool) -> Workspace {
    let repo = repository(Path::new("/tmp/temporary-fixture"));
    store.register_repository(&repo).unwrap();
    let mut input = new_workspace(&repo.id, "temporary");
    input.temporary = Some(TemporaryWorkspacePolicy { keep_thread });
    store
        .create_workspace_with_event(
            input,
            EventDraft::workspace(EventKind::WorkspaceCreated, EventSource::Coco, json!({})),
        )
        .unwrap()
        .0
}

#[test]
fn policy_and_one_shot_consumption_survive_database_reopen() {
    for keep_thread in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state.db");
        let workspace = {
            let store = Store::open(&path).unwrap();
            create(&store, keep_thread)
        };
        {
            let store = Store::open(&path).unwrap();
            let saved = store.workspace_by_id(&workspace.id).unwrap().unwrap();
            assert_eq!(saved.temporary, workspace.temporary);
            assert_eq!(
                store.claim_temporary_cleanup(&workspace.id).unwrap(),
                workspace.temporary
            );
        }
        let store = Store::open(&path).unwrap();
        assert_eq!(store.claim_temporary_cleanup(&workspace.id).unwrap(), None);
        assert_eq!(
            store
                .workspace_by_id(&workspace.id)
                .unwrap()
                .unwrap()
                .temporary,
            None
        );
    }
}

#[test]
fn released_v17_workspaces_migrate_as_persistent() {
    let store = Store::in_memory().unwrap();
    let repo = repository(Path::new("/tmp/migration"));
    store.register_repository(&repo).unwrap();
    let workspace = ready_workspace(&store, &repo.id, "existing");
    {
        let connection = store.lock().unwrap();
        connection.execute_batch("ALTER TABLE workspaces DROP COLUMN temporary_cleanup_json; PRAGMA user_version=17;").unwrap();
        migrations::migrate(&connection).unwrap();
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            18
        );
    }
    assert_eq!(
        store.workspace_by_id(&workspace.id).unwrap().unwrap(),
        workspace
    );
    assert_eq!(store.claim_temporary_cleanup(&workspace.id).unwrap(), None);
}

#[test]
fn two_connections_cannot_both_claim_cleanup() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state.db");
    let workspace = create(&Store::open(&path).unwrap(), false);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let path = path.clone();
            let id = workspace.id.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let store = Store::open(&path).unwrap();
                barrier.wait();
                store.claim_temporary_cleanup(&id).unwrap()
            })
        })
        .collect();
    let claims: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(claims.iter().filter(|claim| claim.is_some()).count(), 1);
}

#[test]
fn malformed_policy_cannot_be_claimed_or_overwritten() {
    let store = Store::in_memory().unwrap();
    let workspace = create(&store, false);
    store
        .lock()
        .unwrap()
        .execute(
            "UPDATE workspaces SET temporary_cleanup_json = 'false' WHERE id = ?1",
            [&workspace.id],
        )
        .unwrap();
    assert!(store.claim_temporary_cleanup(&workspace.id).is_err());
    assert_eq!(
        store
            .lock()
            .unwrap()
            .query_row(
                "SELECT temporary_cleanup_json FROM workspaces WHERE id = ?1",
                [&workspace.id],
                |r| r.get::<_, String>(0),
            )
            .unwrap(),
        "false"
    );
}
