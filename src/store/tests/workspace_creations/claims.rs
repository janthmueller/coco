use super::*;

#[test]
fn atomic_association_requires_the_live_public_claim_and_never_consumes_another_intent() {
    let store = Store::in_memory().unwrap();
    let repo = repository(Path::new("/tmp/create-claim"));
    store.register_repository(&repo).unwrap();
    let mut intent = WorkspaceCreationReceipt::unknown(
        "create-owner".into(),
        repo.id.clone(),
        repo.root_path.clone(),
    );
    intent.request_sha256 = Some("public-owner-hash".into());
    intent.state = WorkspaceCreationState::Preparing;
    assert!(store.claim_workspace_creation(&intent).unwrap().1);
    let mut input = new_workspace(&repo.id, "owner");
    for wrong_claim in [None, Some("another-request".into())] {
        input.creation_request_sha256 = wrong_claim;
        assert!(matches!(
            store.create_workspace_with_event(
                input.clone(),
                EventDraft::workspace(EventKind::WorkspaceCreated, EventSource::Coco, json!({}))
            ),
            Err(StoreError::CreationKeyRetired)
        ));
        assert_eq!(
            store.workspace_creation("create-owner").unwrap().unwrap(),
            intent
        );
        assert!(store.list_workspaces(None).unwrap().is_empty());
        assert!(store.events_after(None, 0).unwrap().is_empty());
    }
    input.creation_request_sha256 = intent.request_sha256.clone();
    let (workspace, _) = store
        .create_workspace_with_event(
            input,
            EventDraft::workspace(EventKind::WorkspaceCreated, EventSource::Coco, json!({})),
        )
        .unwrap();
    assert_eq!(
        store
            .workspace_creation("create-owner")
            .unwrap()
            .unwrap()
            .workspace_id
            .as_deref(),
        Some(workspace.id.as_str())
    );
    assert!(
        matches!(
            store.workspace_by_creation_claim("create-owner", None),
            Err(StoreError::CreationKeyRetired)
        ),
        "a private replay cannot adopt the public workspace either"
    );
}

#[test]
fn missing_rejected_foreign_and_retired_claims_do_not_allow_workspace_association() {
    for condition in ["missing", "rejected", "foreign", "deleted"] {
        let store = Store::in_memory().unwrap();
        let repo = repository(Path::new("/tmp/invalid-create-claim"));
        store.register_repository(&repo).unwrap();
        let mut intent = WorkspaceCreationReceipt::unknown(
            "create-owner".into(),
            repo.id.clone(),
            repo.root_path.clone(),
        );
        intent.request_sha256 = Some("public-owner-hash".into());
        intent.state = match condition {
            "rejected" => WorkspaceCreationState::Rejected,
            "deleted" => WorkspaceCreationState::Deleted,
            _ => WorkspaceCreationState::Preparing,
        };
        if condition == "foreign" {
            intent.repository_id = "other-repository".into();
        }
        if condition != "missing" {
            store.claim_workspace_creation(&intent).unwrap();
        }
        let mut input = new_workspace(&repo.id, "owner");
        input.creation_request_sha256 = intent.request_sha256.clone();
        assert!(
            matches!(
                store.create_workspace_with_event(
                    input,
                    EventDraft::workspace(
                        EventKind::WorkspaceCreated,
                        EventSource::Coco,
                        json!({})
                    )
                ),
                Err(StoreError::CreationKeyRetired)
            ),
            "{condition}"
        );
        assert!(store.list_workspaces(None).unwrap().is_empty());
    }
}
