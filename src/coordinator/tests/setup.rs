use super::*;
use crate::domain::setup::{WorkerMcpReference, WorkspaceCreationReceipt, WorkspaceCreationState};
use crate::protocol::{WorkspaceCreationGetParams, WorkspaceSetupParams, WorkspaceSetupRequest};
use sha2::{Digest, Sha256};

mod bindings;
mod claims;
mod context;
mod recovery;

fn params(fixture: &Fixture) -> WorkspaceSetupParams {
    let existing = fixture.create_params();
    WorkspaceSetupParams {
        repository_path: fixture.source.clone(),
        request: WorkspaceSetupRequest {
            operation_id: "public-setup-key".into(),
            name: existing.name,
            worktree: existing.worktree,
            changes: existing.changes,
            profile: "default".into(),
            context: Default::default(),
            model: Some("fixture-model".into()),
            worker_mcp: None,
        },
    }
}

fn get(fixture: &Fixture) -> WorkspaceCreationGetParams {
    WorkspaceCreationGetParams {
        repository_path: fixture.source.clone(),
        operation_id: "public-setup-key".into(),
    }
}

fn worker_file(fixture: &Fixture, grant: &str) -> WorkerMcpReference {
    let bytes = serde_json::to_vec(&json!({"mcp_servers": {"worker": {
        "command": "/fixture/worker", "args": ["--grant", grant], "enabled_tools": ["run.submit"],
        "default_tools_approval_mode": "auto", "required": true
    }}}))
    .unwrap();
    let path = fixture._temp.path().join(format!("{grant}.json"));
    fs::write(&path, &bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    WorkerMcpReference {
        path,
        sha256: hex::encode(Sha256::digest(&bytes)),
    }
}

fn workspace(fixture: &Fixture, receipt: &WorkspaceCreationReceipt) -> Workspace {
    fixture
        .store
        .workspace_by_id(receipt.workspace_id.as_deref().unwrap())
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn identical_retry_after_lost_response_never_recaptures_a_moving_ref() {
    let fixture = Fixture::new(FakeWorker::default());
    let request = params(&fixture);
    let first = fixture
        .coordinator
        .setup_workspace(request.clone())
        .await
        .unwrap();
    assert_eq!(first.state, WorkspaceCreationState::Configured);
    assert!(first.codex_thread_id.is_none());
    assert!(
        fixture.worker.calls().is_empty(),
        "setup must not start native execution"
    );
    fs::write(fixture.source.join("later.txt"), "later\n").unwrap();
    run_git(&fixture.source, &["add", "later.txt"]);
    run_git(&fixture.source, &["commit", "-m", "move source"]);
    let retried = fixture.coordinator.setup_workspace(request).await.unwrap();
    assert_eq!(first, retried);
    assert_ne!(
        first.base_sha.as_deref().unwrap(),
        git_output(&fixture.source, &["rev-parse", "HEAD"])
    );
    assert_eq!(
        fixture
            .coordinator
            .get_workspace_creation(get(&fixture))
            .unwrap(),
        first
    );
}

#[tokio::test]
async fn changed_requests_conflict_and_name_collisions_are_not_adopted() {
    let fixture = Fixture::new(FakeWorker::default());
    let original = params(&fixture);
    fixture
        .coordinator
        .setup_workspace(original.clone())
        .await
        .unwrap();
    let mut alternatives = vec![original.clone(); 5];
    alternatives[0].request.name = "other".into();
    alternatives[1].request.profile = "other".into();
    alternatives[2].request.model = None;
    alternatives[3].request.changes = WorkspaceChangesRequest::Ignore;
    alternatives[4].request.worker_mcp = Some(worker_file(&fixture, "different-grant"));
    for changed in alternatives {
        assert_eq!(
            fixture
                .coordinator
                .setup_workspace(changed)
                .await
                .unwrap_err()
                .code(),
            "IDEMPOTENCY_CONFLICT"
        );
    }
    let mut collision = original;
    collision.request.operation_id = "different-key".into();
    let rejected = fixture
        .coordinator
        .setup_workspace(collision)
        .await
        .unwrap();
    assert_eq!(rejected.state, WorkspaceCreationState::Rejected);
    assert_eq!(rejected.error_code.as_deref(), Some("WORKSPACE_EXISTS"));
    assert!(rejected.workspace_id.is_none());
}

#[tokio::test]
async fn concurrent_requests_share_one_durable_intent_and_one_workspace() {
    let fixture = Fixture::new(FakeWorker::default());
    let request = params(&fixture);
    let repository = fixture.coordinator.git.discover(&fixture.source).unwrap();
    let gate = fixture.coordinator.repository_lock(&repository.id).await;
    let held = gate.lock().await;
    let mut first = Box::pin(fixture.coordinator.setup_workspace(request.clone()));
    assert!(futures_util::poll!(first.as_mut()).is_pending());
    let second = fixture.coordinator.setup_workspace(request).await.unwrap();
    assert_eq!(second.state, WorkspaceCreationState::Preparing);
    assert!(second.workspace_id.is_none());
    drop(held);
    let first = first.await.unwrap();
    assert_eq!(first.request_sha256, second.request_sha256);
    assert_eq!(first.state, WorkspaceCreationState::Configured);
    let receipt = fixture
        .coordinator
        .get_workspace_creation(get(&fixture))
        .unwrap();
    assert_eq!(receipt.state, WorkspaceCreationState::Configured);
    let repository = fixture.coordinator.git.discover(&fixture.source).unwrap();
    assert_eq!(
        fixture
            .store
            .list_workspaces(Some(&repository.id))
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn foreign_keys_reveal_nothing_and_cannot_mutate_the_other_repository() {
    let first = Fixture::new(FakeWorker::default());
    let second = Fixture::new(FakeWorker::default());
    let created = first
        .coordinator
        .setup_workspace(params(&first))
        .await
        .unwrap();
    let lookup = first
        .coordinator
        .get_workspace_creation(WorkspaceCreationGetParams {
            repository_path: second.source.clone(),
            operation_id: created.operation_id.clone(),
        })
        .unwrap();
    assert_eq!(lookup.state, WorkspaceCreationState::Unknown);
    assert!(lookup.workspace_id.is_none() && lookup.request_sha256.is_none());
    let mut request = params(&first);
    request.repository_path = second.source.clone();
    assert_eq!(
        first
            .coordinator
            .setup_workspace(request)
            .await
            .unwrap_err()
            .code(),
        "IDEMPOTENCY_CONFLICT"
    );
    assert!(second.worktrees.read_dir().is_err());
}
