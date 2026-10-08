use super::*;
use crate::protocol::DiagnosticStatus;

async fn inspect(fixture: &Fixture) -> super::super::doctor::DoctorChecks {
    fixture
        .coordinator
        .doctor_checks(tokio::time::Instant::now() + crate::diagnostics::REPORT_TIMEOUT)
        .await
}

#[tokio::test]
async fn doctor_accepts_a_registered_repository_without_a_first_commit() {
    let fixture = Fixture::new(FakeWorker::default());
    let unborn = fixture._temp.path().join("unborn");
    run_git(
        fixture._temp.path(),
        &["init", "--initial-branch=main", unborn.to_str().unwrap()],
    );
    let repository = fixture
        .coordinator
        .register_repository(RepositoryRegisterParams { path: unborn })
        .await
        .unwrap();
    let report = inspect(&fixture).await;
    assert!(report.complete);
    assert!(
        !report
            .checks
            .iter()
            .any(|check| check.status == DiagnosticStatus::Error)
    );
    assert!(report.checks.iter().any(|check| check.id == "repository"
        && check.status == DiagnosticStatus::Ok
        && check.subject_id.as_ref() == Some(&repository.id)));
    assert_eq!(fixture.worker.calls(), vec![WorkerCall::Models]);
}

#[tokio::test]
async fn doctor_inspects_a_dirty_prepared_workspace_without_starting_or_mutating_it() {
    let fixture = Fixture::new(FakeWorker::default());
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    fs::write(
        workspace
            .worktree_path
            .as_ref()
            .unwrap()
            .join("untracked.txt"),
        "local data",
    )
    .unwrap();
    let before = fixture.store.workspace_by_id(&workspace.id).unwrap();
    fixture.worker.calls.lock().unwrap().clear();
    let report = inspect(&fixture).await;
    assert!(report.complete);
    assert!(
        !report
            .checks
            .iter()
            .any(|check| check.status == DiagnosticStatus::Error)
    );
    assert_eq!(fixture.worker.calls(), vec![WorkerCall::Models]);
    assert_eq!(
        fixture.store.workspace_by_id(&workspace.id).unwrap(),
        before
    );
    assert!(
        report
            .checks
            .iter()
            .any(|check| check.id == "worktree" && check.status == DiagnosticStatus::Ok)
    );
    assert!(
        report
            .checks
            .iter()
            .any(|check| check.id == "thread" && check.status == DiagnosticStatus::Skipped)
    );
}

#[tokio::test]
async fn doctor_reads_a_bound_thread_without_loading_it_or_changing_stored_state() {
    let fixture = Fixture::new(FakeWorker::default());
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    fixture.worker.calls.lock().unwrap().clear();
    let before = fixture.store.workspace_by_id(&workspace.id).unwrap();
    let report = inspect(&fixture).await;
    assert!(
        !report
            .checks
            .iter()
            .any(|check| check.status == DiagnosticStatus::Error)
    );
    assert_eq!(
        fixture.worker.calls(),
        vec![
            WorkerCall::Models,
            WorkerCall::Locate {
                thread_id: workspace.codex_thread_id.unwrap()
            }
        ]
    );
    assert_eq!(
        fixture.store.workspace_by_id(&workspace.id).unwrap(),
        before
    );
}

#[tokio::test]
async fn doctor_reports_missing_and_mismatched_thread_bindings() {
    let fixture = Fixture::new(FakeWorker::default());
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let thread_id = workspace.codex_thread_id.as_ref().unwrap();
    fixture
        .worker
        .native_threads
        .lock()
        .unwrap()
        .get_mut(thread_id)
        .unwrap()
        .cwd = fixture.source.clone();
    let report = inspect(&fixture).await;
    assert!(report.checks.iter().any(|check| check.id == "thread"
        && check.status == DiagnosticStatus::Error
        && check.message.contains("different workspace")));
    fixture
        .worker
        .native_threads
        .lock()
        .unwrap()
        .remove(thread_id);
    let report = inspect(&fixture).await;
    assert!(report.checks.iter().any(|check| check.id == "thread"
        && check.status == DiagnosticStatus::Error
        && check.message.contains("not found")));
}

#[tokio::test]
async fn doctor_detects_an_unexpected_worktree_branch() {
    let fixture = Fixture::new(FakeWorker::default());
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    run_git(
        workspace.worktree_path.as_deref().unwrap(),
        &["checkout", "--detach"],
    );
    let report = inspect(&fixture).await;
    assert!(report.checks.iter().any(|check| check.id == "worktree"
        && check.status == DiagnosticStatus::Error
        && check.subject_id.as_ref() == Some(&workspace.id)));
}

#[tokio::test]
async fn closed_worktrees_and_archived_threads_are_expected_not_errors() {
    let fixture = Fixture::new(FakeWorker::default());
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    fixture
        .coordinator
        .close_workspace(WorkspaceCloseParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id,
            archive_thread: true,
            discard_changes: false,
            dry_run: false,
            expected_plan: None,
        })
        .await
        .unwrap();
    let report = inspect(&fixture).await;
    assert!(!report.checks.iter().any(|check| matches!(
        check.status,
        DiagnosticStatus::Error | DiagnosticStatus::Warning
    )));
    assert!(
        report
            .checks
            .iter()
            .any(|check| check.id == "worktree" && check.status == DiagnosticStatus::Skipped)
    );
    assert!(
        report
            .checks
            .iter()
            .any(|check| check.id == "thread" && check.status == DiagnosticStatus::Ok)
    );
}

#[tokio::test]
async fn a_spent_report_budget_is_reported_as_incomplete() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let report = fixture
        .coordinator
        .doctor_checks(tokio::time::Instant::now())
        .await;
    assert!(!report.complete);
    assert!(
        report
            .checks
            .iter()
            .any(|check| check.id == "coverage.timeout" || check.status == DiagnosticStatus::Error)
    );
}
