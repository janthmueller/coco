use super::*;
use crate::domain::clients::{ClientIntegration, ClientMetadata};

fn metadata() -> ClientMetadata {
    ClientMetadata {
        kind: "native_tui".to_owned(),
        integration: Some(ClientIntegration {
            kind: "tmux".to_owned(),
            scope: "opaque-server".to_owned(),
            locator: "%7".to_owned(),
            label: Some("dev:2.1".to_owned()),
        }),
    }
}

async fn status(fixture: &Fixture, params: WorkspaceGetParams) -> WorkspaceStatusResult {
    fixture.coordinator.get_workspace(params).await.unwrap()
}

#[tokio::test]
async fn status_projects_multiple_clients_passively_and_keeps_native_state_separate() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let mut leases = Vec::new();
    for client in [Some(metadata()), None] {
        let attached = fixture
            .coordinator
            .attach_workspace(WorkspaceAttachParams {
                scope: RepositoryScope::repository(fixture.source.clone()),
                workspace: workspace.id.clone(),
                client,
            })
            .await
            .unwrap();
        let WorkspaceAttachLaunch::Resume { lease_id, .. } = attached.launch else {
            panic!("bound workspace did not resume");
        };
        leases.push(lease_id);
    }
    let before_calls = fixture.worker.calls.lock().unwrap().len();
    let get = WorkspaceGetParams {
        scope: RepositoryScope::repository(fixture.source.clone()),
        workspace: workspace.id.clone(),
        include_resources: false,
        include_clients: true,
    };
    let projected = status(&fixture, get.clone()).await;
    assert_eq!(projected.workspace.phase, WorkspacePhase::Idle);
    assert_eq!(projected.clients.as_ref().unwrap().len(), 2);
    let listed = fixture
        .coordinator
        .list_workspaces(WorkspaceListParams {
            scope: RepositoryScope::AllRepositories,
            phases: None,
            include_resources: false,
            include_activity: false,
            include_clients: true,
        })
        .await
        .unwrap();
    assert_eq!(listed[0].clients, projected.clients);
    assert!(
        fixture.worker.calls.lock().unwrap()[before_calls..]
            .iter()
            .all(|call| matches!(call, WorkerCall::Read { .. }))
    );
    let encoded = serde_json::to_string(&projected).unwrap();
    for lease_id in &leases {
        assert!(!encoded.contains(lease_id));
    }
    let omitted = status(
        &fixture,
        WorkspaceGetParams {
            include_clients: false,
            ..get.clone()
        },
    )
    .await;
    assert!(
        serde_json::to_value(&omitted)
            .unwrap()
            .get("clients")
            .is_none()
    );
    fixture
        .coordinator
        .release_workspace_attach(WorkspaceAttachReleaseParams {
            workspace_id: workspace.id.clone(),
            lease_id: leases.remove(0),
        })
        .unwrap();
    assert_eq!(
        status(&fixture, get.clone()).await.clients.unwrap().len(),
        1
    );
    fixture
        .coordinator
        .release_workspace_attach(WorkspaceAttachReleaseParams {
            workspace_id: workspace.id,
            lease_id: leases.remove(0),
        })
        .unwrap();
    assert!(status(&fixture, get).await.clients.unwrap().is_empty());
}

#[tokio::test]
async fn prepared_presence_does_not_create_a_thread_and_invalid_metadata_has_no_side_effects() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let mut invalid = metadata();
    invalid.integration.as_mut().unwrap().label = Some("bad\nlabel".to_owned());
    let params = WorkspaceAttachParams {
        scope: RepositoryScope::repository(fixture.source.clone()),
        workspace: workspace.id.clone(),
        client: Some(invalid),
    };
    let before = fixture.worker.calls.lock().unwrap().len();
    assert!(matches!(
        fixture.coordinator.attach_workspace(params.clone()).await,
        Err(CoordinatorError::InvalidParams(_))
    ));
    assert_eq!(fixture.worker.calls.lock().unwrap().len(), before);
    assert!(
        fixture
            .coordinator
            .workspace_clients(&workspace.id)
            .is_empty()
    );
    fixture
        .coordinator
        .attach_workspace(WorkspaceAttachParams {
            client: Some(metadata()),
            ..params
        })
        .await
        .unwrap();
    let status = fixture
        .coordinator
        .get_workspace(WorkspaceGetParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id,
            include_resources: false,
            include_clients: true,
        })
        .await
        .unwrap();
    assert_eq!(status.workspace.phase, WorkspacePhase::Prepared);
    assert!(status.workspace.codex_thread_id.is_none());
    assert_eq!(status.clients.unwrap()[0].metadata, metadata());
    assert!(
        fixture.worker.calls.lock().unwrap()[before..]
            .iter()
            .all(|call| !matches!(call, WorkerCall::Thread { .. } | WorkerCall::Resume { .. }))
    );
}
