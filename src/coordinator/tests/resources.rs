use super::*;
use crate::domain::runtime::WorkspaceResourcePolicy;
use crate::protocol::{
    ResourcePolicyUpdate, WorkspaceLimitsGetParams, WorkspaceLimitsSetParams,
    WorkspaceResourcePolicyPatch,
};

#[tokio::test]
async fn resource_policies_are_persisted_patched_and_sent_to_the_runtime_boundary() {
    let fixture = Fixture::new(FakeWorker {
        resource_limits_supported: true,
        ..FakeWorker::default()
    });
    fixture.register().await;
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let scope = RepositoryScope::repository(fixture.source.clone());

    let result = fixture
        .coordinator
        .set_workspace_limits(WorkspaceLimitsSetParams {
            scope: scope.clone(),
            workspace: workspace.name.clone(),
            patch: WorkspaceResourcePolicyPatch {
                memory_high_bytes: Some(ResourcePolicyUpdate::Set(256 * 1024 * 1024)),
                memory_max_bytes: Some(ResourcePolicyUpdate::Set(512 * 1024 * 1024)),
                cpu_max_millicores: Some(ResourcePolicyUpdate::Set(1_500)),
                cpu_weight: Some(ResourcePolicyUpdate::Set(200)),
                tasks_max: Some(ResourcePolicyUpdate::Set(256)),
            },
        })
        .await
        .unwrap();
    assert_eq!(result.policy.revision, 1);
    assert_eq!(
        result.policy.policy.memory_high_bytes,
        Some(256 * 1024 * 1024)
    );
    assert_eq!(result.policy.policy.cpu_max_millicores, Some(1_500));
    assert_eq!(
        result.controller.runtime_state,
        WorkspaceRuntimeState::Inactive
    );

    let result = fixture
        .coordinator
        .set_workspace_limits(WorkspaceLimitsSetParams {
            scope: scope.clone(),
            workspace: workspace.id.clone(),
            patch: WorkspaceResourcePolicyPatch {
                memory_high_bytes: Some(ResourcePolicyUpdate::Clear),
                cpu_weight: Some(ResourcePolicyUpdate::Set(50)),
                ..WorkspaceResourcePolicyPatch::default()
            },
        })
        .await
        .unwrap();
    assert_eq!(result.policy.revision, 2);
    assert_eq!(result.policy.policy.memory_high_bytes, None);
    assert_eq!(
        result.policy.policy.memory_max_bytes,
        Some(512 * 1024 * 1024)
    );
    assert_eq!(result.policy.policy.cpu_weight, Some(50));

    let loaded = fixture
        .coordinator
        .get_workspace_limits(WorkspaceLimitsGetParams {
            scope,
            workspace: workspace.id.clone(),
        })
        .await
        .unwrap();
    assert_eq!(loaded.policy, result.policy);
    assert!(fixture.worker.calls().iter().any(|call| matches!(
        call,
        WorkerCall::ConfigureResources {
            workspace_id,
            snapshot
        } if workspace_id == &workspace.id && snapshot.revision == 2
    )));
}

#[tokio::test]
async fn unsupported_or_failed_policy_updates_never_leave_new_desired_limits() {
    let unsupported = Fixture::new(FakeWorker::default());
    unsupported.register().await;
    let workspace = unsupported
        .coordinator
        .create_workspace(unsupported.create_params())
        .await
        .unwrap()
        .workspace;
    let request = WorkspaceLimitsSetParams {
        scope: RepositoryScope::repository(unsupported.source.clone()),
        workspace: workspace.id.clone(),
        patch: WorkspaceResourcePolicyPatch {
            memory_max_bytes: Some(ResourcePolicyUpdate::Set(512 * 1024 * 1024)),
            ..WorkspaceResourcePolicyPatch::default()
        },
    };
    assert!(matches!(
        unsupported.coordinator.set_workspace_limits(request).await,
        Err(CoordinatorError::ResourcePolicyUnsupported { .. })
    ));
    assert_eq!(
        unsupported
            .store
            .workspace_resource_policy(&workspace.id)
            .unwrap(),
        WorkspaceResourcePolicySnapshot::default()
    );

    let failed = Fixture::new(FakeWorker {
        resource_limits_supported: true,
        fail_next_resource_policy: StdMutex::new(true),
        ..FakeWorker::default()
    });
    failed.register().await;
    let workspace = failed
        .coordinator
        .create_workspace(failed.create_params())
        .await
        .unwrap()
        .workspace;
    let request = WorkspaceLimitsSetParams {
        scope: RepositoryScope::repository(failed.source.clone()),
        workspace: workspace.id.clone(),
        patch: WorkspaceResourcePolicyPatch {
            tasks_max: Some(ResourcePolicyUpdate::Set(128)),
            ..WorkspaceResourcePolicyPatch::default()
        },
    };
    assert!(matches!(
        failed.coordinator.set_workspace_limits(request).await,
        Err(CoordinatorError::ResourcePolicyApplication(_))
    ));
    let restored = failed
        .store
        .workspace_resource_policy(&workspace.id)
        .unwrap();
    assert_eq!(restored.revision, 2);
    assert_eq!(restored.policy, WorkspaceResourcePolicy::default());
}

#[tokio::test]
async fn a_persisted_policy_cannot_activate_on_an_unsupported_backend() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    fixture
        .store
        .replace_workspace_resource_policy(
            &workspace.id,
            0,
            &WorkspaceResourcePolicy {
                memory_max_bytes: Some(512 * 1024 * 1024),
                ..WorkspaceResourcePolicy::default()
            },
        )
        .unwrap();

    let calls_before = fixture.worker.calls().len();
    assert!(matches!(
        fixture
            .coordinator
            .start_turn(TurnStartParams {
                scope: RepositoryScope::repository(fixture.source.clone()),
                workspace: workspace.id,
                message: "must remain contained".to_owned(),
                operation_id: "limits-fail-closed".to_owned(),
            })
            .await,
        Err(CoordinatorError::ResourcePolicyUnsupported { .. })
    ));
    assert_eq!(fixture.worker.calls().len(), calls_before);
}

#[tokio::test]
async fn clearing_a_live_cpu_cap_never_stops_the_runtime_implicitly() {
    let fixture = Fixture::new(FakeWorker {
        resource_limits_supported: true,
        resource_runtime_running: StdMutex::new(true),
        ..FakeWorker::default()
    });
    fixture.register().await;
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let scope = RepositoryScope::repository(fixture.source.clone());
    fixture
        .coordinator
        .set_workspace_limits(WorkspaceLimitsSetParams {
            scope: scope.clone(),
            workspace: workspace.id.clone(),
            patch: WorkspaceResourcePolicyPatch {
                cpu_max_millicores: Some(ResourcePolicyUpdate::Set(750)),
                ..WorkspaceResourcePolicyPatch::default()
            },
        })
        .await
        .unwrap();

    let result = fixture
        .coordinator
        .reset_workspace_limits(crate::protocol::WorkspaceLimitsResetParams {
            scope,
            workspace: workspace.id.clone(),
        })
        .await
        .unwrap();
    assert_eq!(result.policy.revision, 2);
    assert!(result.policy.policy.is_empty());
    assert_eq!(
        result.controller.runtime_state,
        WorkspaceRuntimeState::Running
    );

    let calls = fixture.worker.calls();
    assert!(!calls.iter().any(
        |call| matches!(call, WorkerCall::StopExecution { workspace_id } if workspace_id == &workspace.id)
    ));
    assert!(
        calls.iter().any(|call| {
            matches!(
                call,
                WorkerCall::ConfigureResources { snapshot, .. }
                    if snapshot.revision == 2 && snapshot.policy.is_empty()
            )
        }),
        "the running runtime manager must receive the desired reset policy"
    );
}
