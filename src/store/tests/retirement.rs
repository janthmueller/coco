use super::*;

#[test]
fn migrates_v11_deletion_intents_without_expanding_authorized_loss() {
    let store = Store::in_memory().unwrap();
    let repo = repository(Path::new("/tmp/v11-retirement"));
    store.register_repository(&repo).unwrap();
    let workspace = ready_workspace(&store, &repo.id, "pending-delete");
    let connection = store.lock().unwrap();
    connection
        .execute(
            "UPDATE workspaces SET availability = 'deleting', delete_thread_requested = 1,
            delete_branch_requested = 1, closed_head_sha = base_sha WHERE id = ?1",
            [&workspace.id],
        )
        .unwrap();
    connection
        .execute_batch(
            "ALTER TABLE workspaces DROP COLUMN delete_discard_unretained_commits;
         ALTER TABLE workspaces DROP COLUMN delete_from_open;
         PRAGMA user_version = 11;",
        )
        .unwrap();
    crate::store::migrations::migrate(&connection).unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        14
    );
    drop(connection);
    assert_eq!(
        store.workspace_deletion_intent(&workspace.id).unwrap(),
        WorkspaceDeletionIntent {
            delete_thread: true,
            delete_branch: true,
            discard_unretained_commits: false,
            from_open: false,
        }
    );
    let retained = store.workspace_by_id(&workspace.id).unwrap().unwrap();
    assert_eq!(retained.availability, WorkspaceAvailability::Deleting);
    assert_eq!(retained.closed_head_sha, workspace.base_sha);
    assert_eq!(retained.codex_thread_id, workspace.codex_thread_id);
}
