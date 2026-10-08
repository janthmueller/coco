use super::support::{Fixture, TEST_TIMEOUT, finish_activation};
use super::*;

#[tokio::test]
async fn blocked_registration_leaves_other_workspaces_responsive() {
    let mut fixture = Fixture::new().await;
    let ready = fixture.activate("ready");
    fixture.complete_activation("ready").await;
    finish_activation(ready).await;

    let blocked = fixture.activate("blocked");
    let blocked_add = fixture.next_request("environment/add", "blocked").await;

    let observed = timeout(TEST_TIMEOUT, fixture.executors.resources("ready"))
        .await
        .expect("an unrelated resource read waited for blocked registration")
        .unwrap();
    assert_eq!(observed.state, WorkspaceRuntimeState::Running);
    let inactive = timeout(TEST_TIMEOUT, fixture.executors.resources("unstarted"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(inactive.state, WorkspaceRuntimeState::Inactive);

    let unrelated = fixture.activate("unrelated");
    fixture.complete_activation("unrelated").await;
    finish_activation(unrelated).await;
    timeout(TEST_TIMEOUT, fixture.executors.stop("ready"))
        .await
        .expect("an unrelated stop waited for blocked registration")
        .unwrap();
    let policy = WorkspaceResourcePolicySnapshot {
        revision: 1,
        ..WorkspaceResourcePolicySnapshot::default()
    };
    let updated = timeout(
        TEST_TIMEOUT,
        fixture
            .executors
            .configure_resource_policy("unrelated", policy.clone()),
    )
    .await
    .expect("an unrelated policy update waited for blocked registration")
    .unwrap();
    assert_eq!(updated.applied_policy, Some(policy));

    fixture.respond(&blocked_add).await;
    let info = fixture.next_request("environment/info", "blocked").await;
    fixture.respond(&info).await;
    finish_activation(blocked).await;
    fixture.finish().await;
}

#[tokio::test]
async fn a_process_waiting_to_publish_its_endpoint_does_not_block_other_workspaces() {
    let mut fixture = Fixture::new().await;
    let gate = fixture.cwd("blocked").join("start.block");
    std::fs::write(&gate, "wait").unwrap();
    let blocked = fixture.activate("blocked");
    fixture.pid("blocked").await;

    let unrelated = fixture.activate("unrelated");
    fixture.complete_activation("unrelated").await;
    finish_activation(unrelated).await;
    std::fs::remove_file(gate).unwrap();
    fixture.complete_activation("blocked").await;
    finish_activation(blocked).await;
    fixture.finish().await;
}

#[tokio::test]
async fn blocked_connection_verification_does_not_delay_another_activation() {
    let mut fixture = Fixture::new().await;
    let blocked = fixture.activate("blocked");
    let add = fixture.next_request("environment/add", "blocked").await;
    fixture.respond(&add).await;
    let blocked_info = fixture.next_request("environment/info", "blocked").await;

    let unrelated = fixture.activate("unrelated");
    fixture.complete_activation("unrelated").await;
    finish_activation(unrelated).await;
    fixture.respond(&blocked_info).await;
    finish_activation(blocked).await;
    fixture.finish().await;
}

#[tokio::test]
async fn concurrent_activation_of_one_workspace_starts_only_one_executor() {
    let mut fixture = Fixture::new().await;
    let first = fixture.activate("same");
    let add = fixture.next_request("environment/add", "same").await;
    let cwd = fixture.cwd("same");
    let executors = fixture.executors.clone();
    let second = executors.ensure("same", &cwd);
    tokio::pin!(second);
    assert!(futures_util::poll!(&mut second).is_pending());

    fixture.respond(&add).await;
    let info = fixture.next_request("environment/info", "same").await;
    fixture.respond(&info).await;
    let first = finish_activation(first).await;
    let second = timeout(TEST_TIMEOUT, second).await.unwrap().unwrap();
    assert_eq!(first, second);
    fixture.assert_no_more_requests();
    fixture.finish().await;
}

#[tokio::test]
async fn stop_waits_for_its_workspace_start_and_cannot_race_a_replacement() {
    let mut fixture = Fixture::new().await;
    let activation = fixture.activate("same");
    let add = fixture.next_request("environment/add", "same").await;
    let executors = fixture.executors.clone();
    let stop = executors.stop("same");
    tokio::pin!(stop);
    assert!(futures_util::poll!(&mut stop).is_pending());
    fixture.respond(&add).await;
    let info = fixture.next_request("environment/info", "same").await;
    fixture.respond(&info).await;
    finish_activation(activation).await;
    timeout(TEST_TIMEOUT, stop).await.unwrap().unwrap();
    assert_eq!(
        fixture.executors.resources("same").await.unwrap().state,
        WorkspaceRuntimeState::Inactive
    );

    let replacement = fixture.activate("same");
    fixture.complete_activation("same").await;
    finish_activation(replacement).await;
    fixture.finish().await;
}

#[tokio::test]
async fn policy_revision_updates_wait_for_their_own_activation() {
    let mut fixture = Fixture::new().await;
    let activation = fixture.activate("same");
    let add = fixture.next_request("environment/add", "same").await;
    let snapshot = WorkspaceResourcePolicySnapshot {
        revision: 2,
        ..WorkspaceResourcePolicySnapshot::default()
    };
    let executors = fixture.executors.clone();
    let update = executors.configure_resource_policy("same", snapshot.clone());
    tokio::pin!(update);
    assert!(futures_util::poll!(&mut update).is_pending());
    fixture.respond(&add).await;
    let info = fixture.next_request("environment/info", "same").await;
    fixture.respond(&info).await;
    finish_activation(activation).await;
    let applied = timeout(TEST_TIMEOUT, update).await.unwrap().unwrap();
    assert_eq!(applied.applied_policy, Some(snapshot.clone()));
    assert_eq!(
        fixture
            .executors
            .configure_resource_policy("same", snapshot)
            .await
            .unwrap(),
        applied
    );
    assert!(matches!(
        fixture
            .executors
            .configure_resource_policy("same", WorkspaceResourcePolicySnapshot::default())
            .await,
        Err(WorkspaceExecutionError::StaleResourcePolicy {
            requested: 0,
            current: 2
        })
    ));
    fixture.finish().await;
}
