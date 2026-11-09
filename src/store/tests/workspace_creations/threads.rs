use super::*;
use crate::domain::setup::WorkerMcpReference;

fn bound_workspace(store: &Store, repo: &Repository, name: &str) -> Workspace {
    let mut input = new_workspace(&repo.id, name);
    let mut intent = WorkspaceCreationReceipt::unknown(
        input.create_operation_id.clone().unwrap(),
        repo.id.clone(),
        repo.root_path.clone(),
    );
    intent.state = WorkspaceCreationState::Preparing;
    intent.request_sha256 = Some(format!("request-{name}"));
    intent.worker_mcp = Some(WorkerMcpReference {
        path: "/private/fixture-worker.json".into(),
        sha256: "a".repeat(64),
    });
    store.claim_workspace_creation(&intent).unwrap();
    input.creation_request_sha256 = intent.request_sha256;
    let (workspace, _) = store
        .create_workspace_with_event(
            input,
            EventDraft::workspace(EventKind::WorkspaceCreated, EventSource::Coco, json!({})),
        )
        .unwrap();
    store
        .transition_workspace_lifecycle_with_event(
            &workspace.id,
            WorkspaceLifecycle::Provisioning,
            WorkspaceLifecycle::Ready,
            None,
            EventDraft::workspace(EventKind::WorktreeCreated, EventSource::Git, json!({})),
        )
        .unwrap()
        .0
}

fn dispatch(store: &Store, workspace: &Workspace, thread: &str) -> Operation {
    let (operation, _) = store
        .prepare_operation(NewOperation {
            operation_id: format!("send-{thread}"),
            workspace_id: workspace.id.clone(),
            kind: OperationKind::TurnStart,
            request_fingerprint: format!("fixture-{thread}"),
        })
        .unwrap();
    store
        .begin_operation_dispatch(&operation.id, thread)
        .unwrap()
}

#[test]
fn every_known_worker_identity_survives_operation_reset_cleanup_and_database_reopen() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("identities.db");
    let repo = repository(&temporary.path().join("repository"));
    let workspace_id = {
        let store = Store::open(&path).unwrap();
        store.register_repository(&repo).unwrap();
        let bound = bound_workspace(&store, &repo, "bound");
        let uncertain = dispatch(&store, &bound, "uncertain-thread");
        store.mark_operation_uncertain(&uncertain.id).unwrap();
        let rejected = dispatch(&store, &bound, "rejected-thread");
        store.reset_rejected_operation(&rejected.id).unwrap();
        store
            .remember_worker_mcp_thread(&bound.id, "empty-known-thread")
            .unwrap();
        let ordinary = ready_workspace(&store, &repo.id, "unbound");
        dispatch(&store, &ordinary, "ordinary-thread");
        assert!(
            !store
                .thread_has_worker_mcp_binding("ordinary-thread")
                .unwrap()
        );
        bound.id
    };
    let store = Store::open(&path).unwrap();
    assert!(
        store
            .workspace_by_id(&workspace_id)
            .unwrap()
            .unwrap()
            .codex_thread_id
            .is_none()
    );
    store
        .begin_workspace_deletion(
            &workspace_id,
            WorkspaceDeletionIntent {
                from_open: true,
                ..WorkspaceDeletionIntent::default()
            },
            None,
        )
        .unwrap();
    store.delete_workspace_record(&workspace_id).unwrap();
    assert!(
        store
            .operation_by_client_id("send-uncertain-thread")
            .unwrap()
            .is_none()
    );
    drop(store);
    let store = Store::open(&path).unwrap();
    for thread in ["uncertain-thread", "rejected-thread", "empty-known-thread"] {
        assert!(
            store.thread_has_worker_mcp_binding(thread).unwrap(),
            "{thread}"
        );
    }
    for thread in ["ordinary-thread", "never-observed-thread"] {
        assert!(
            !store.thread_has_worker_mcp_binding(thread).unwrap(),
            "{thread}"
        );
    }
}

#[test]
fn v19_migration_recovers_operation_bound_and_retired_thread_evidence() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("v19.db");
    let repo = repository(&temporary.path().join("repository"));
    {
        let store = Store::open(&path).unwrap();
        store.register_repository(&repo).unwrap();
        let bound = bound_workspace(&store, &repo, "bound");
        dispatch(&store, &bound, "unconfirmed-thread");
        store
            .bind_thread_with_event(
                &bound.id,
                WorkspaceLifecycle::Ready,
                WorkspaceLifecycle::Ready,
                NewThreadBinding {
                    thread_id: "bound-thread".into(),
                    parent_thread_id: None,
                },
                EventDraft::workspace(EventKind::AgentStarted, EventSource::Codex, json!({})),
            )
            .unwrap();
        let retired = bound_workspace(&store, &repo, "retired");
        store
            .bind_thread_with_event(
                &retired.id,
                WorkspaceLifecycle::Ready,
                WorkspaceLifecycle::Ready,
                NewThreadBinding {
                    thread_id: "retired-thread".into(),
                    parent_thread_id: None,
                },
                EventDraft::workspace(EventKind::AgentStarted, EventSource::Codex, json!({})),
            )
            .unwrap();
        store
            .begin_workspace_deletion(
                &retired.id,
                WorkspaceDeletionIntent {
                    from_open: true,
                    ..WorkspaceDeletionIntent::default()
                },
                None,
            )
            .unwrap();
        store.delete_workspace_record(&retired.id).unwrap();
        store
            .lock()
            .unwrap()
            .execute_batch("DROP TABLE worker_mcp_threads; PRAGMA user_version=19;")
            .unwrap();
    }
    let store = Store::open(&path).unwrap();
    assert_eq!(
        store
            .lock()
            .unwrap()
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        20
    );
    // These assertions deliberately inspect the new independent evidence, not
    // the existing fallback queries on workspaces/creation receipts.
    for thread in ["unconfirmed-thread", "bound-thread", "retired-thread"] {
        let present: bool = store
            .lock()
            .unwrap()
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM worker_mcp_threads WHERE thread_id = ?1)",
                [thread],
                |r| r.get(0),
            )
            .unwrap();
        assert!(present, "{thread}");
    }
}
