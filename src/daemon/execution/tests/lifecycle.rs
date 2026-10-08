use super::support::{Fixture, TEST_TIMEOUT, finish_activation};
use super::*;

#[tokio::test]
async fn cancelled_activation_is_reconciled_before_close_finishes() {
    let mut fixture = Fixture::new().await;
    let activation = fixture.activate("cancelled");
    let add = fixture.next_request("environment/add", "cancelled").await;
    let pid = fixture.pid("cancelled").await;
    activation.abort();
    assert!(activation.await.unwrap_err().is_cancelled());

    let executors = fixture.executors.clone();
    let close = executors.close();
    tokio::pin!(close);
    assert!(futures_util::poll!(&mut close).is_pending());
    let rejected_start = fixture.activate("too-late");

    fixture.respond(&add).await;
    let info = fixture.next_request("environment/info", "cancelled").await;
    fixture.respond(&info).await;
    timeout(TEST_TIMEOUT, close).await.unwrap();
    assert!(matches!(
        timeout(TEST_TIMEOUT, rejected_start)
            .await
            .unwrap()
            .unwrap(),
        Err(WorkspaceExecutionError::ShuttingDown)
    ));
    assert!(fixture.executors.inner.entries.lock().await.is_empty());
    Fixture::assert_stopped(pid).await;
    fixture.assert_no_more_requests();
    fixture.finish().await;
}

#[tokio::test]
async fn registration_failure_cleans_up_and_permits_a_fresh_retry() {
    let mut fixture = Fixture::new().await;
    let activation = fixture.activate("failed");
    let add = fixture.next_request("environment/add", "failed").await;
    let pid = fixture.pid("failed").await;
    fixture.reject(&add).await;
    assert!(matches!(
        timeout(TEST_TIMEOUT, activation).await.unwrap().unwrap(),
        Err(WorkspaceExecutionError::Registration { .. })
    ));
    assert_eq!(
        fixture.executors.resources("failed").await.unwrap().state,
        WorkspaceRuntimeState::Inactive
    );
    Fixture::assert_stopped(pid).await;
    let retry = fixture.activate("failed");
    fixture.complete_activation("failed").await;
    finish_activation(retry).await;
    fixture.finish().await;
}

#[tokio::test]
async fn connection_failure_cleans_up_without_publishing_a_live_runtime() {
    let mut fixture = Fixture::new().await;
    let activation = fixture.activate("failed");
    let add = fixture.next_request("environment/add", "failed").await;
    fixture.respond(&add).await;
    let info = fixture.next_request("environment/info", "failed").await;
    let pid = fixture.pid("failed").await;
    fixture.reject(&info).await;
    assert!(matches!(
        timeout(TEST_TIMEOUT, activation).await.unwrap().unwrap(),
        Err(WorkspaceExecutionError::Connection { .. })
    ));
    assert_eq!(
        fixture.executors.resources("failed").await.unwrap().state,
        WorkspaceRuntimeState::Inactive
    );
    Fixture::assert_stopped(pid).await;
    fixture.finish().await;
}

#[tokio::test]
async fn changed_worktree_replaces_the_runtime_under_the_same_environment_id() {
    let mut fixture = Fixture::new().await;
    let first = fixture.activate("same");
    fixture.complete_activation("same").await;
    let original = finish_activation(first).await;
    let original_pid = fixture
        .executors
        .resources("same")
        .await
        .unwrap()
        .process_id;

    let executors = fixture.executors.clone();
    let cwd = fixture.cwd("replacement");
    let expected_cwd = cwd.clone();
    let replacement = tokio::spawn(async move { executors.ensure("same", &cwd).await });
    fixture.complete_activation("same").await;
    let replacement = finish_activation(replacement).await;
    assert_eq!(replacement.environment_id, original.environment_id);
    assert_eq!(replacement.cwd, expected_cwd);
    assert_ne!(
        fixture
            .executors
            .resources("same")
            .await
            .unwrap()
            .process_id,
        original_pid
    );
    Fixture::assert_stopped(original_pid.unwrap()).await;
    fixture.finish().await;
}

#[tokio::test]
async fn passive_reads_leave_unstarted_workspaces_out_of_the_runtime_registry() {
    let fixture = Fixture::new().await;
    assert_eq!(
        fixture.executors.resources("unknown").await.unwrap().state,
        WorkspaceRuntimeState::Inactive
    );
    assert_eq!(
        fixture
            .executors
            .resource_policy_status("unknown")
            .await
            .unwrap()
            .runtime_state,
        WorkspaceRuntimeState::Inactive
    );
    assert!(fixture.executors.inner.entries.lock().await.is_empty());
    fixture.finish().await;
}

#[tokio::test]
async fn missing_app_server_responses_expire_and_stop_the_owned_executor() {
    let mut fixture = Fixture::new().await;
    for verify_connection in [false, true] {
        let activation = fixture.activate("unresponsive");
        let add = fixture
            .next_request("environment/add", "unresponsive")
            .await;
        if verify_connection {
            fixture.respond(&add).await;
            fixture
                .next_request("environment/info", "unresponsive")
                .await;
        }
        let pid = fixture.pid("unresponsive").await;
        let error = timeout(EXEC_SERVER_REQUEST_TIMEOUT + TEST_TIMEOUT, activation)
            .await
            .expect("unresponsive App Server prevented bounded cleanup")
            .unwrap()
            .unwrap_err();
        if verify_connection {
            assert!(matches!(error, WorkspaceExecutionError::ConnectionTimeout));
        } else {
            assert!(matches!(
                error,
                WorkspaceExecutionError::RegistrationTimeout
            ));
        }
        Fixture::assert_stopped(pid).await;
    }
    fixture.finish().await;
}
