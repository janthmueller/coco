use super::*;
use std::time::Duration;

fn reopen(fixture: Fixture) -> Fixture {
    let Fixture {
        _temp,
        source,
        worktrees,
        codex_home,
        store,
        worker,
        coordinator,
    } = fixture;
    drop(coordinator);
    drop(store);
    let store = Arc::new(Store::open(_temp.path().join("coordinator.db")).unwrap());
    let coordinator = Coordinator::new(
        store.clone(),
        Git::default(),
        worker.clone(),
        worktrees.clone(),
        codex_home.clone(),
        Arc::new(HookRegistry::empty()),
        "reopened".into(),
    );
    Fixture {
        _temp,
        source,
        worktrees,
        codex_home,
        store,
        worker,
        coordinator,
    }
}

async fn claim(
    fixture: &Fixture,
    params: &WorkspaceDeleteOperationParams,
) -> WorkspaceDeletionReceipt {
    let mut receipt = preview(fixture, params).await;
    receipt.state = WorkspaceDeletionState::Preparing;
    receipt.created_at_ms = Some(1);
    receipt.updated_at_ms = Some(1);
    assert!(fixture.store.claim_workspace_deletion(&receipt).unwrap().1);
    receipt
}

fn begin(fixture: &Fixture, receipt: &WorkspaceDeletionReceipt) {
    let plan = receipt.plan.as_ref().unwrap();
    fixture
        .store
        .begin_workspace_deletion_with_operation(
            &plan.workspace_id,
            WorkspaceDeletionIntent {
                delete_thread: true,
                delete_branch: true,
                from_open: true,
                discard_unretained_commits: false,
            },
            plan.head_sha.as_deref(),
            Some(&receipt.operation_id),
        )
        .unwrap();
}

#[tokio::test]
async fn successful_receipt_survives_actual_database_reopen_without_native_reads() {
    let fixture = Fixture::new_persistent(FakeWorker::default());
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let params = request(&fixture, &workspace);
    let deleted = fixture
        .coordinator
        .delete_workspace_operation(params.clone())
        .await
        .unwrap();
    let fixture = reopen(fixture);
    let calls = fixture.worker.calls();
    assert_eq!(lookup(&fixture, &params), deleted);
    assert_eq!(
        fixture
            .coordinator
            .delete_workspace_operation(params)
            .await
            .unwrap(),
        deleted
    );
    assert_eq!(fixture.worker.calls(), calls);
}

#[tokio::test]
async fn interrupted_pre_saga_claim_stays_uncertain_and_never_dispatches_on_retry() {
    let fixture = Fixture::new_persistent(FakeWorker::default());
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let params = request(&fixture, &workspace);
    let receipt = claim(&fixture, &params).await;
    let second = Store::open(fixture._temp.path().join("coordinator.db")).unwrap();
    assert!(!second.claim_workspace_deletion(&receipt).unwrap().1);
    drop(second);
    let fixture = reopen(fixture);
    fixture.coordinator.recover_workspace_retirements().await;
    let result = fixture
        .coordinator
        .delete_workspace_operation(params.clone())
        .await
        .unwrap();
    assert_eq!(result.state, WorkspaceDeletionState::Uncertain);
    assert_eq!(
        result.error_code.as_deref(),
        Some("DELETION_DISPATCH_UNCONFIRMED")
    );
    assert!(workspace.worktree_path.as_ref().unwrap().exists());
    assert_eq!(lookup(&fixture, &params), result);
}

#[tokio::test]
async fn recovery_never_repeats_file_discard_or_attributes_a_later_cli_delete() {
    let fixture = Fixture::new_persistent(FakeWorker::default());
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let params = request(&fixture, &workspace);
    let receipt = claim(&fixture, &params).await;
    begin(&fixture, &receipt);
    let path = workspace.worktree_path.as_ref().unwrap();
    fs::write(path.join("after-crash"), "must survive recovery").unwrap();
    let fixture = reopen(fixture);
    assert_eq!(fixture.coordinator.recover_workspace_retirements().await, 1);
    assert_eq!(
        lookup(&fixture, &params).state,
        WorkspaceDeletionState::Uncertain
    );
    assert!(path.join("after-crash").exists());
    assert_eq!(
        fixture
            .coordinator
            .delete_workspace_operation(params.clone())
            .await
            .unwrap()
            .state,
        WorkspaceDeletionState::Uncertain
    );
    let mut private = super::super::retirement_deletion::delete_params(&fixture, &workspace);
    private.discard_changes = true;
    fixture.coordinator.delete_workspace(private).await.unwrap();
    assert_eq!(
        lookup(&fixture, &params).state,
        WorkspaceDeletionState::Uncertain
    );
}

#[tokio::test]
async fn recovery_confirms_only_the_associated_saga_after_worktree_removal() {
    let fixture = Fixture::new_persistent(FakeWorker::default());
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let params = request(&fixture, &workspace);
    let receipt = claim(&fixture, &params).await;
    begin(&fixture, &receipt);
    git_output(
        &fixture.source,
        &[
            "worktree",
            "remove",
            "--",
            workspace.worktree_path.as_ref().unwrap().to_str().unwrap(),
        ],
    );
    let fixture = reopen(fixture);
    assert_eq!(fixture.coordinator.recover_workspace_retirements().await, 1);
    let result = lookup(&fixture, &params);
    assert_eq!(result.state, WorkspaceDeletionState::Deleted);
    assert!(result.error_code.is_none());
    assert_eq!(
        fixture
            .coordinator
            .delete_workspace_operation(params)
            .await
            .unwrap(),
        result
    );
    assert!(
        fixture
            .store
            .workspace_by_id(&workspace.id)
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn partial_native_failure_is_uncertain_and_not_blindly_resent_after_restart() {
    let fixture = Fixture::new_persistent(FakeWorker {
        fail_delete_thread: true,
        ..Default::default()
    });
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let params = request(&fixture, &workspace);
    let receipt = fixture
        .coordinator
        .delete_workspace_operation(params.clone())
        .await
        .unwrap();
    assert_eq!(receipt.state, WorkspaceDeletionState::Uncertain);
    assert!(receipt.error_code.is_some());
    assert!(!workspace.worktree_path.as_ref().unwrap().exists());
    assert!(
        fixture
            .store
            .workspace_by_id(&workspace.id)
            .unwrap()
            .is_some()
    );
    let fixture = reopen(fixture);
    let calls = fixture.worker.calls();
    assert_eq!(
        fixture
            .coordinator
            .delete_workspace_operation(params.clone())
            .await
            .unwrap(),
        receipt
    );
    assert_eq!(lookup(&fixture, &params), receipt);
    assert_eq!(fixture.worker.calls(), calls);
}

#[tokio::test]
async fn matching_completion_wins_when_it_overtakes_another_callers_planning() {
    let fixture = Fixture::new(FakeWorker::default());
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let params = request(&fixture, &workspace);
    let mut receipt = preview(&fixture, &params).await;
    receipt.state = WorkspaceDeletionState::Preparing;
    receipt.created_at_ms = Some(1);
    receipt.updated_at_ms = Some(1);
    let lock = fixture
        .coordinator
        .repository_lock(&workspace.repository_id)
        .await;
    let guard = lock.lock().await;
    let pending = fixture
        .coordinator
        .delete_workspace_operation(params.clone());
    tokio::pin!(pending);
    assert!(
        tokio::time::timeout(Duration::from_millis(30), &mut pending)
            .await
            .is_err()
    );
    assert!(fixture.store.claim_workspace_deletion(&receipt).unwrap().1);
    begin(&fixture, &receipt);
    git_output(
        &fixture.source,
        &[
            "worktree",
            "remove",
            "--",
            workspace.worktree_path.as_ref().unwrap().to_str().unwrap(),
        ],
    );
    git_output(
        &fixture.source,
        &["branch", "-d", workspace.branch_name.as_ref().unwrap()],
    );
    fixture
        .store
        .delete_workspace_record(&workspace.id)
        .unwrap();
    drop(guard);
    let observed = pending.await.unwrap();
    assert_eq!(observed.state, WorkspaceDeletionState::Deleted);
    assert_eq!(observed, lookup(&fixture, &params));
}
