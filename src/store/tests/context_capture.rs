use super::*;

#[test]
fn restart_fails_stranded_create_time_capture_but_preserves_valid_ready_workspaces() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("capture.sqlite3");
    let repo = repository(&temporary.path().join("source"));
    let mut retained_ids = Vec::new();
    let stranded_id;
    {
        let store = Store::open(&path).unwrap();
        store.register_repository(&repo).unwrap();
        for (name, version, capture_timing, confirmed) in [
            ("stranded", 4, "create", false),
            ("fresh", 4, "activation", false),
            ("legacy", 3, "activation", false),
            ("confirmed", 4, "create", true),
        ] {
            let mut input = new_workspace(&repo.id, name);
            if name != "fresh" {
                input.context_mode = ContextMode::Fork;
            }
            input.context = json!({"version": version, "resolved": {"context": {
                "captureTiming": capture_timing, "compactionPending": false,
            }}});
            let (workspace, _) = store
                .create_workspace_with_event(
                    input,
                    EventDraft::workspace(
                        EventKind::WorkspaceCreated,
                        EventSource::Coco,
                        json!({}),
                    ),
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
                .unwrap();
            if confirmed {
                store
                    .bind_thread_with_event(
                        &workspace.id,
                        WorkspaceLifecycle::Ready,
                        WorkspaceLifecycle::Ready,
                        NewThreadBinding {
                            thread_id: "confirmed-child".to_owned(),
                            parent_thread_id: Some("source-thread".to_owned()),
                        },
                        EventDraft::workspace(
                            EventKind::AgentStarted,
                            EventSource::Codex,
                            json!({}),
                        ),
                    )
                    .unwrap();
            }
            retained_ids.push((name, workspace.id));
        }
        stranded_id = retained_ids.remove(0).1;
    }

    let store = Store::open(&path).unwrap();
    assert_eq!(
        store
            .reconcile_unfinished()
            .unwrap()
            .failed_workspace_preparations,
        1
    );
    let stranded = store.workspace_by_id(&stranded_id).unwrap().unwrap();
    assert_eq!(stranded.lifecycle, WorkspaceLifecycle::Failed);
    assert_eq!(stranded.phase, WorkspacePhase::Failed);
    assert_eq!(stranded.last_error_code.as_deref(), Some("DAEMON_RESTART"));
    assert!(stranded.codex_thread_id.is_none());
    assert_eq!(
        store
            .events_after(Some(&stranded_id), 0)
            .unwrap()
            .iter()
            .filter(|event| event.source_method.as_deref() == Some("startup.reconcile"))
            .count(),
        1
    );
    for (name, id) in retained_ids {
        let workspace = store.workspace_by_id(&id).unwrap().unwrap();
        assert_eq!(workspace.lifecycle, WorkspaceLifecycle::Ready, "{name}");
        assert!(workspace.last_error_code.is_none(), "{name}");
        assert_eq!(workspace.codex_thread_id.is_some(), name == "confirmed");
    }
    assert_eq!(
        store
            .reconcile_unfinished()
            .unwrap()
            .failed_workspace_preparations,
        0
    );
}
