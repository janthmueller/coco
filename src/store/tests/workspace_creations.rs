use super::*;
use crate::domain::setup::{WorkspaceCreationReceipt, WorkspaceCreationState};

mod claims;
mod threads;

#[test]
fn intent_association_and_interrupted_preparation_survive_real_reopen() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("setup.db");
    let repo = repository(&temporary.path().join("repository"));
    let mut intent = WorkspaceCreationReceipt::unknown(
        "create-pending".into(),
        repo.id.clone(),
        repo.root_path.clone(),
    );
    intent.state = WorkspaceCreationState::Preparing;
    intent.request_sha256 = Some("frozen-public-request".into());
    let workspace_id = {
        let store = Store::open(&path).unwrap();
        assert!(store.claim_workspace_creation(&intent).unwrap().1);
        assert!(!store.claim_workspace_creation(&intent).unwrap().1);
        store.register_repository(&repo).unwrap();
        let mut input = new_workspace(&repo.id, "pending");
        input.creation_request_sha256 = intent.request_sha256.clone();
        let (workspace, _) = store
            .create_workspace_with_event(
                input,
                EventDraft::workspace(EventKind::WorkspaceCreated, EventSource::Coco, json!({})),
            )
            .unwrap();
        let linked = store
            .workspace_creation(&intent.operation_id)
            .unwrap()
            .unwrap();
        assert_eq!(linked.state, WorkspaceCreationState::Preparing);
        assert_eq!(linked.workspace_id.as_ref(), Some(&workspace.id));
        workspace.id
    };
    let store = Store::open(&path).unwrap();
    assert_eq!(
        store
            .reconcile_unfinished()
            .unwrap()
            .failed_workspace_preparations,
        1
    );
    let receipt = store
        .workspace_creation(&intent.operation_id)
        .unwrap()
        .unwrap();
    assert_eq!(receipt.workspace_id.as_deref(), Some(workspace_id.as_str()));
    assert_eq!(receipt.request_sha256, intent.request_sha256);
    assert_eq!(receipt.state, WorkspaceCreationState::Failed);
    assert_eq!(receipt.error_code.as_deref(), Some("DAEMON_RESTART"));
    assert!(!store.claim_workspace_creation(&intent).unwrap().1);
}

#[test]
fn genuine_v18_migration_reserves_old_keys_without_inventing_public_request_evidence() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("old.db");
    let repo = repository(&temporary.path().join("repository"));
    let id = {
        let store = Store::open(&path).unwrap();
        store.register_repository(&repo).unwrap();
        let workspace = ready_workspace(&store, &repo.id, "older");
        store
            .lock()
            .unwrap()
            .execute_batch("DROP TABLE workspace_creations; ALTER TABLE workspaces DROP COLUMN worker_mcp_bound; PRAGMA user_version=18;")
            .unwrap();
        workspace.id
    };
    let store = Store::open(&path).unwrap();
    let receipt = store.workspace_creation("create-older").unwrap().unwrap();
    assert_eq!(receipt.workspace_id.as_deref(), Some(id.as_str()));
    assert_eq!(receipt.state, WorkspaceCreationState::Unavailable);
    assert!(receipt.request_sha256.is_none() && receipt.worker_mcp.is_none());
    assert!(!store.claim_workspace_creation(&receipt).unwrap().1);
}

#[test]
fn creation_receipt_never_allows_a_second_workspace_to_claim_the_key() {
    let store = Store::in_memory().unwrap();
    let repo = repository(Path::new("/tmp/keys"));
    store.register_repository(&repo).unwrap();
    let original = ready_workspace(&store, &repo.id, "original");
    let mut other = new_workspace(&repo.id, "other");
    other.create_operation_id = original.create_operation_id;
    assert!(
        store
            .create_workspace_with_event(
                other,
                EventDraft::workspace(EventKind::WorkspaceCreated, EventSource::Coco, json!({}))
            )
            .is_err()
    );
    assert_eq!(store.list_workspaces(Some(&repo.id)).unwrap().len(), 1);
}
