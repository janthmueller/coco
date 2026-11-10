use super::*;
use crate::protocol::ControlSessionPrepareParams;

#[tokio::test]
async fn preparation_includes_an_unregistered_current_repository_without_registering_it() {
    let fixture = Fixture::new(FakeWorker::default());
    let result = fixture
        .coordinator
        .prepare_control_session(ControlSessionPrepareParams {
            repository_path: fixture.source.clone(),
        })
        .await
        .unwrap();
    assert_eq!(result.repositories.len(), 1);
    assert_eq!(
        result.default_repository_id.as_deref(),
        Some(result.repositories[0].id.as_str())
    );
    assert_eq!(
        result.repositories[0].path,
        fixture.source.canonicalize().unwrap()
    );
    assert!(fixture.store.list_repositories().unwrap().is_empty());
    assert!(fixture.store.list_workspaces(None).unwrap().is_empty());
    assert_eq!(
        fixture.worker.calls(),
        [WorkerCall::ControlConfig {
            cwd: fixture.source.canonicalize().unwrap()
        }]
    );
}

#[tokio::test]
async fn known_repositories_are_automatic_and_current_remains_the_default() {
    let first = Fixture::new(FakeWorker::default());
    let second = Fixture::new(FakeWorker::default());
    first.register().await;
    first
        .coordinator
        .register_repository(RepositoryRegisterParams {
            path: second.source.clone(),
        })
        .await
        .unwrap();
    first
        .worker
        .native_worker_slots
        .lock()
        .unwrap()
        .insert("inherited-server".into());
    let result = first
        .coordinator
        .prepare_control_session(ControlSessionPrepareParams {
            repository_path: first.source.clone(),
        })
        .await
        .unwrap();
    assert_eq!(result.repositories.len(), 2);
    assert_eq!(
        result.repositories[0].path,
        first.source.canonicalize().unwrap()
    );
    assert_eq!(
        result.repositories[1].path,
        second.source.canonicalize().unwrap()
    );
    assert_eq!(
        result.default_repository_id,
        Some(result.repositories[0].id.clone())
    );
    assert_eq!(
        result.mcp_servers,
        [crate::domain::control::InheritedMcpServer {
            name: "inherited-server".into(),
            transport: crate::domain::control::McpTransportKind::Stdio,
        }]
    );
    let global = first
        .coordinator
        .prepare_control_session(ControlSessionPrepareParams {
            repository_path: first._temp.path().into(),
        })
        .await
        .unwrap();
    assert!(global.default_repository_id.is_none());
    assert_eq!(global.repositories.len(), 2);
}

#[tokio::test]
async fn unavailable_registry_entries_do_not_block_other_repositories() {
    let first = Fixture::new(FakeWorker::default());
    let second = Fixture::new(FakeWorker::default());
    first
        .coordinator
        .register_repository(RepositoryRegisterParams {
            path: second.source.clone(),
        })
        .await
        .unwrap();
    fs::rename(&second.source, second._temp.path().join("moved-source")).unwrap();
    let prepared = first
        .coordinator
        .prepare_control_session(ControlSessionPrepareParams {
            repository_path: first.source.clone(),
        })
        .await
        .unwrap();
    assert_eq!(prepared.repositories.len(), 1);
    assert_eq!(
        prepared.unavailable_repositories.as_slice(),
        std::slice::from_ref(&second.source)
    );
    // Inventory never changes registry state as a side effect.
    assert_eq!(first.store.list_repositories().unwrap().len(), 1);
}

#[tokio::test]
async fn no_repository_does_not_start_native_execution_or_invent_a_default() {
    let fixture = Fixture::new(FakeWorker::default());
    let error = fixture
        .coordinator
        .prepare_control_session(ControlSessionPrepareParams {
            repository_path: fixture._temp.path().into(),
        })
        .await
        .unwrap_err();
    assert_eq!(error.code(), "INVALID_PARAMS");
    assert!(fixture.worker.calls().is_empty());
    assert!(fixture.store.list_repositories().unwrap().is_empty());
}

#[tokio::test]
async fn a_managed_worktree_keeps_the_registered_repo_name_and_its_own_code_scope() {
    let fixture = Fixture::new(FakeWorker::default());
    let registered = fixture.register().await;
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let worktree = workspace.worktree_path.unwrap();
    let nested = worktree.join("nested");
    fs::create_dir(&nested).unwrap();
    let prepared = fixture
        .coordinator
        .prepare_control_session(ControlSessionPrepareParams {
            repository_path: nested.clone(),
        })
        .await
        .unwrap();
    assert_eq!(prepared.default_repository_id, Some(registered.id));
    assert_eq!(prepared.repositories.len(), 1);
    assert_eq!(prepared.repositories[0].name, registered.display_name);
    assert_eq!(
        prepared.repositories[0].path,
        worktree.canonicalize().unwrap()
    );
    assert_eq!(prepared.working_directory, nested.canonicalize().unwrap());
}
