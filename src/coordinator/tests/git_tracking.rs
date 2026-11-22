use super::*;
use crate::protocol::{GitTrackingState, RepositoryGitInspectParams, WorkspacePathListParams};

async fn prepared(fixture: &Fixture) -> Workspace {
    fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace
}

fn upstream(fixture: &Fixture, workspace: &Workspace) {
    run_git(
        &fixture.source,
        &[
            "remote",
            "add",
            "origin",
            "https://example.invalid/unused.git",
        ],
    );
    let branch = workspace.branch_name.as_deref().unwrap();
    let reference = format!("refs/remotes/origin/{branch}");
    run_git(
        &fixture.source,
        &[
            "update-ref",
            &reference,
            workspace.base_sha.as_ref().unwrap(),
        ],
    );
    run_git(
        &fixture.source,
        &[
            "branch",
            &format!("--set-upstream-to=origin/{branch}"),
            branch,
        ],
    );
}

async fn listed(fixture: &Fixture) -> Vec<WorkspaceListItem> {
    fixture
        .coordinator
        .list_workspaces(WorkspaceListParams {
            scope: RepositoryScope::AllRepositories,
            phases: None,
            include_resources: false,
            include_activity: false,
            include_clients: false,
        })
        .await
        .unwrap()
}

async fn shown(fixture: &Fixture, workspace: &Workspace) -> WorkspaceStatusResult {
    fixture
        .coordinator
        .get_workspace(WorkspaceGetParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id.clone(),
            include_resources: false,
            include_clients: false,
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn base_counts_remain_distinct_from_upstream_in_status_list_and_paths() {
    let fixture = Fixture::new(FakeWorker::default());
    let workspace = prepared(&fixture).await;
    upstream(&fixture, &workspace);
    run_git(
        workspace.worktree_path.as_ref().unwrap(),
        &["commit", "--allow-empty", "-m", "workspace progress"],
    );
    let head = git_output(
        workspace.worktree_path.as_ref().unwrap(),
        &["rev-parse", "HEAD"],
    );
    let remote = format!("refs/remotes/origin/{}", workspace.name);
    run_git(&fixture.source, &["update-ref", &remote, &head]);
    let shown = shown(&fixture, &workspace).await;
    assert!(
        matches!(shown.git, WorkspaceGitStatus::Observed(ref observation) if observation.ahead_by == Some(1))
    );
    let tracking = shown.git_tracking.unwrap();
    assert_eq!(tracking.state, GitTrackingState::InSync);
    assert_eq!(tracking.ahead_by, Some(0));
    assert!(tracking.observed_at_ms.is_some());
    assert_eq!(
        listed(&fixture).await[0].git_tracking,
        Some(tracking.clone())
    );
    let paths = fixture
        .coordinator
        .list_workspace_paths(WorkspacePathListParams {
            scope: RepositoryScope::AllRepositories,
            include_recency: false,
        })
        .await
        .unwrap();
    assert_eq!(paths[0].git_tracking, Some(tracking));
    assert!(fixture.worker.calls().is_empty());
}

#[tokio::test]
async fn fresh_source_inspection_enrichment_does_not_change_reviewed_base_or_register() {
    let fixture = Fixture::new(FakeWorker::default());
    let params = RepositoryGitInspectParams {
        path: fixture.source.clone(),
        include_branches: true,
        base: Some(WorkspaceBaseRequest::Revision {
            revision: "HEAD".into(),
        }),
    };
    let mut state = fixture
        .coordinator
        .inspect_repository_git(params.clone())
        .unwrap();
    let base = state.base_sha.clone();
    let index = fs::read(fixture.source.join(".git/index")).unwrap();
    fixture
        .coordinator
        .enrich_git_inspection(&mut state, &params)
        .await;
    assert_eq!(
        state.tracking.as_ref().unwrap().state,
        GitTrackingState::NoUpstream
    );
    assert_eq!(state.base_tracking, state.tracking);
    assert_eq!(
        state.branches[0].tracking.as_ref().unwrap().state,
        GitTrackingState::NoUpstream
    );
    assert_eq!(state.base_sha, base);
    assert_eq!(fs::read(fixture.source.join(".git/index")).unwrap(), index);
    assert!(fixture.store.list_repositories().unwrap().is_empty());
    assert!(fixture.worker.calls().is_empty() && !fixture.worktrees.exists());
    state.head_sha = Some("b".repeat(40));
    fixture
        .coordinator
        .enrich_git_inspection(&mut state, &params)
        .await;
    assert_eq!(
        state.tracking.as_ref().unwrap().state,
        GitTrackingState::Unknown
    );
    assert_eq!(state.base_sha, base);
}

#[tokio::test]
async fn repeated_reads_use_one_cached_snapshot_then_refresh_after_expiry() {
    let fixture = Fixture::new(FakeWorker::default());
    let workspace = prepared(&fixture).await;
    upstream(&fixture, &workspace);
    run_git(
        workspace.worktree_path.as_ref().unwrap(),
        &["commit", "--allow-empty", "-m", "progress"],
    );
    let first = listed(&fixture).await[0].git_tracking.clone().unwrap();
    assert_eq!(first.state, GitTrackingState::Ahead);
    let head = git_output(
        workspace.worktree_path.as_ref().unwrap(),
        &["rev-parse", "HEAD"],
    );
    run_git(
        &fixture.source,
        &[
            "update-ref",
            &format!("refs/remotes/origin/{}", workspace.name),
            &head,
        ],
    );
    let (one, two) = tokio::join!(listed(&fixture), listed(&fixture));
    assert_eq!(one[0].git_tracking.as_ref(), Some(&first));
    assert_eq!(two[0].git_tracking.as_ref(), Some(&first));
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    assert_eq!(
        listed(&fixture).await[0]
            .git_tracking
            .as_ref()
            .unwrap()
            .state,
        GitTrackingState::InSync
    );
}

#[tokio::test]
async fn missing_metadata_is_optional_and_failure_is_cached_without_breaking_navigation() {
    let mut fixture = Fixture::new(FakeWorker::default());
    let workspace = prepared(&fixture).await;
    fixture.coordinator.git = Git::new(fixture._temp.path().join("missing-git"));
    assert_eq!(
        listed(&fixture).await[0]
            .git_tracking
            .as_ref()
            .unwrap()
            .state,
        GitTrackingState::Unknown
    );
    fixture.coordinator.git = Git::default();
    assert_eq!(
        listed(&fixture).await[0]
            .git_tracking
            .as_ref()
            .unwrap()
            .state,
        GitTrackingState::Unknown
    );
    assert_eq!(
        fixture
            .coordinator
            .workspace_path(crate::protocol::WorkspacePathGetParams {
                scope: RepositoryScope::AllRepositories,
                workspace: workspace.id.clone(),
            })
            .unwrap(),
        workspace.worktree_path.unwrap()
    );
    fixture.coordinator.git_tracking.clear();
    assert_eq!(
        listed(&fixture).await[0]
            .git_tracking
            .as_ref()
            .unwrap()
            .state,
        GitTrackingState::NoUpstream
    );
    assert!(fixture.worker.calls().is_empty());
}

#[tokio::test]
async fn drift_is_unknown_and_closed_workspaces_have_no_live_tracking_projection() {
    let fixture = Fixture::new(FakeWorker::default());
    let workspace = prepared(&fixture).await;
    run_git(
        workspace.worktree_path.as_ref().unwrap(),
        &["checkout", "--detach"],
    );
    assert_eq!(
        listed(&fixture).await[0]
            .git_tracking
            .as_ref()
            .unwrap()
            .state,
        GitTrackingState::Unknown
    );
    assert!(matches!(
        shown(&fixture, &workspace).await.git,
        WorkspaceGitStatus::Unavailable(_)
    ));
    run_git(
        workspace.worktree_path.as_ref().unwrap(),
        &["checkout", workspace.branch_name.as_deref().unwrap()],
    );
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
    assert!(shown(&fixture, &workspace).await.git_tracking.is_none());
}

#[tokio::test]
async fn same_branch_names_in_different_repositories_keep_their_own_tracking_and_scope() {
    let fixture = Fixture::new(FakeWorker::default());
    let workspace = prepared(&fixture).await;
    upstream(&fixture, &workspace);
    let other = Fixture::new(FakeWorker::default());
    let mut params = fixture.create_params();
    params.repository_path = other.source.clone();
    params.operation_id = "other-repository".into();
    let second = fixture
        .coordinator
        .create_workspace(params)
        .await
        .unwrap()
        .workspace;
    let rows = listed(&fixture).await;
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows.iter()
            .find(|row| row.workspace.id == workspace.id)
            .unwrap()
            .git_tracking
            .as_ref()
            .unwrap()
            .state,
        GitTrackingState::InSync
    );
    assert_eq!(
        rows.iter()
            .find(|row| row.workspace.id == second.id)
            .unwrap()
            .git_tracking
            .as_ref()
            .unwrap()
            .state,
        GitTrackingState::NoUpstream
    );
    let rejected = fixture
        .coordinator
        .get_workspace(WorkspaceGetParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: second.id,
            include_resources: false,
            include_clients: false,
        })
        .await
        .unwrap_err();
    assert_eq!(rejected.code(), "WORKSPACE_NOT_FOUND");
}

#[cfg(unix)]
#[tokio::test]
async fn slow_optional_git_is_bounded_and_repeated_views_do_not_spawn_again() {
    use std::os::unix::fs::PermissionsExt;
    let mut fixture = Fixture::new(FakeWorker::default());
    prepared(&fixture).await;
    let executable = fixture._temp.path().join("slow-git");
    fs::write(
        &executable,
        "#!/bin/sh\nprintf 'read\\n' >> \"$0.calls\"\nexec sleep 30\n",
    )
    .unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    fixture.coordinator.git = Git::new(&executable);
    let rows = tokio::time::timeout(std::time::Duration::from_secs(3), listed(&fixture))
        .await
        .unwrap();
    assert_eq!(
        rows[0].git_tracking.as_ref().unwrap().state,
        GitTrackingState::Unknown
    );
    listed(&fixture).await;
    assert_eq!(
        fs::read_to_string(executable.with_extension("calls")).unwrap(),
        "read\n"
    );
    assert!(fixture.worker.calls().is_empty());
}
