use super::*;
use crate::protocol::{WorkspacePathGetParams, WorkspacePathListParams};

mod recency;

fn lookup(fixture: &Fixture, reference: &str) -> WorkspacePathGetParams {
    WorkspacePathGetParams {
        scope: RepositoryScope::repository(fixture.source.clone()),
        workspace: reference.to_owned(),
    }
}

#[tokio::test]
async fn prepared_and_bound_workspaces_resolve_without_any_native_calls_or_mutations() {
    let fixture = Fixture::new(FakeWorker::default());
    let mut params = fixture.create_params();
    params.temporary = Some(crate::domain::TemporaryWorkspacePolicy { keep_thread: true });
    let prepared = fixture
        .coordinator
        .create_workspace(params)
        .await
        .unwrap()
        .workspace;
    let before = fixture
        .store
        .workspace_by_id(&prepared.id)
        .unwrap()
        .unwrap();
    for reference in [&prepared.name, &prepared.id] {
        let path = fixture
            .coordinator
            .workspace_path(lookup(&fixture, reference))
            .unwrap();
        assert_eq!(Some(path), prepared.worktree_path);
    }
    assert_eq!(
        fixture
            .store
            .workspace_by_id(&prepared.id)
            .unwrap()
            .unwrap(),
        before
    );
    assert!(fixture.worker.calls().is_empty());

    let bound = fixture
        .coordinator
        .materialize_workspace_thread(prepared)
        .await
        .unwrap();
    fixture.worker.calls.lock().unwrap().clear();
    fixture
        .worker
        .failed_thread_reads
        .lock()
        .unwrap()
        .push("thread-1".to_owned());
    let before = fixture.store.workspace_by_id(&bound.id).unwrap().unwrap();
    assert_eq!(
        Some(
            fixture
                .coordinator
                .workspace_path(lookup(&fixture, &bound.name))
                .unwrap()
        ),
        bound.worktree_path
    );
    let rows = fixture
        .coordinator
        .list_workspace_paths(WorkspacePathListParams {
            scope: RepositoryScope::AllRepositories,
            include_recency: false,
        })
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].workspace_id, bound.id);
    assert_eq!(
        fixture.store.workspace_by_id(&bound.id).unwrap().unwrap(),
        before
    );
    assert!(fixture.worker.calls().is_empty());
}

#[tokio::test]
async fn closed_and_missing_worktrees_fail_explicit_lookup_and_are_omitted_from_picker() {
    let fixture = Fixture::new(FakeWorker::default());
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let original = workspace.worktree_path.as_ref().unwrap();
    let relocated = original.with_extension("moved");
    fs::rename(original, &relocated).unwrap();
    let error = fixture
        .coordinator
        .workspace_path(lookup(&fixture, &workspace.name))
        .unwrap_err();
    assert_eq!(error.code(), "WORKTREE_UNAVAILABLE");
    assert!(
        fixture
            .coordinator
            .list_workspace_paths(WorkspacePathListParams {
                scope: RepositoryScope::AllRepositories,
                include_recency: false,
            })
            .await
            .unwrap()
            .is_empty()
    );
    fs::rename(&relocated, original).unwrap();

    fixture
        .coordinator
        .close_workspace(WorkspaceCloseParams {
            scope: RepositoryScope::AllRepositories,
            workspace: workspace.id.clone(),
            archive_thread: false,
            discard_changes: false,
            dry_run: false,
            expected_plan: None,
        })
        .await
        .unwrap();
    fixture.worker.calls.lock().unwrap().clear();
    let error = fixture
        .coordinator
        .workspace_path(lookup(&fixture, &workspace.name))
        .unwrap_err();
    assert_eq!(error.code(), "INVALID_WORKSPACE_STATE");
    assert!(error.to_string().contains("coco reopen"));
    assert!(
        fixture
            .coordinator
            .list_workspace_paths(WorkspacePathListParams {
                scope: RepositoryScope::AllRepositories,
                include_recency: false,
            })
            .await
            .unwrap()
            .is_empty()
    );
    assert!(!original.exists());
    assert!(fixture.worker.calls().is_empty());
}

#[tokio::test]
async fn navigation_uses_existing_repository_isolation_and_global_ambiguity_rules() {
    let fixture = Fixture::new(FakeWorker::default());
    let first = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let other = fixture.source.parent().unwrap().join("other");
    run_git(
        fixture.source.parent().unwrap(),
        &[
            "clone",
            "--local",
            fixture.source.to_str().unwrap(),
            other.to_str().unwrap(),
        ],
    );
    let mut params = fixture.create_params();
    params.repository_path = other.clone();
    params.operation_id = "create-in-other-repo".to_owned();
    let second = fixture
        .coordinator
        .create_workspace(params)
        .await
        .unwrap()
        .workspace;
    assert_ne!(first.repository_id, second.repository_id);

    assert_eq!(
        fixture
            .coordinator
            .workspace_path(lookup(&fixture, &second.id))
            .unwrap_err()
            .code(),
        "WORKSPACE_NOT_FOUND"
    );
    assert_eq!(
        fixture
            .coordinator
            .workspace_path(WorkspacePathGetParams {
                scope: RepositoryScope::AllRepositories,
                workspace: first.name.clone(),
            })
            .unwrap_err()
            .code(),
        "WORKSPACE_REFERENCE_AMBIGUOUS"
    );
    assert_eq!(
        Some(
            fixture
                .coordinator
                .workspace_path(WorkspacePathGetParams {
                    scope: RepositoryScope::AllRepositories,
                    workspace: second.id.clone(),
                })
                .unwrap()
        ),
        second.worktree_path
    );

    let rows = fixture
        .coordinator
        .list_workspace_paths(WorkspacePathListParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            include_recency: false,
        })
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].workspace_id, first.id);
    assert!(fixture.worker.calls().is_empty());
}
