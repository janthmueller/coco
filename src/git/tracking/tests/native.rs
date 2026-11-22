use super::*;
use tempfile::TempDir;

struct Fixture(TempDir);

#[tokio::test]
async fn separate_git_directory_keeps_ref_metadata_without_inventing_a_worktree() {
    let fixture = Fixture::new();
    let admin = fixture.path().join("separate-admin");
    fixture.run(&[
        "init",
        "--quiet",
        "--separate-git-dir",
        admin.to_str().unwrap(),
    ]);
    assert_eq!(
        fixture.tracking("main").await.state,
        GitTrackingState::InSync
    );
    let repository = Git::default().discover(fixture.path()).unwrap();
    assert_eq!(
        fixture
            .snapshot()
            .await
            .worktree(
                &repository.root_path,
                WorktreeMode::ExistingBranch,
                Some("main"),
                None
            )
            .state,
        GitTrackingState::Unknown
    );
}

impl Fixture {
    fn new() -> Self {
        let fixture = Self(tempfile::tempdir().unwrap());
        fixture.run(&["init", "--quiet", "--initial-branch=main"]);
        fixture.commit("base");
        fixture.run(&[
            "remote",
            "add",
            "origin",
            "https://example.invalid/never-contacted.git",
        ]);
        let head = fixture.run(&["rev-parse", "HEAD"]);
        fixture.run(&["update-ref", "refs/remotes/origin/main", &head]);
        fixture.run(&["branch", "--set-upstream-to=origin/main", "main"]);
        fixture
    }

    fn path(&self) -> &Path {
        self.0.path()
    }

    fn run(&self, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .current_dir(self.path())
            .args([
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "user.name=CoCo fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "-c",
                "commit.gpgSign=false",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    fn commit(&self, message: &str) {
        self.run(&["commit", "--quiet", "--allow-empty", "-m", message]);
    }

    async fn snapshot(&self) -> TrackingSnapshot {
        let git = Git::default();
        let repository = git.discover(self.path()).unwrap();
        git.tracking_snapshot(&repository.root_path, &repository.id)
            .await
            .unwrap()
    }

    async fn tracking(&self, branch: &str) -> GitTracking {
        let head = self.run(&["rev-parse", &format!("refs/heads/{branch}")]);
        self.snapshot()
            .await
            .branch(&format!("refs/heads/{branch}"), &head)
    }
}

#[tokio::test]
async fn native_equal_ahead_behind_diverged_missing_and_unconfigured() {
    let fixture = Fixture::new();
    let base = fixture.run(&["rev-parse", "HEAD"]);
    assert_eq!(
        fixture.tracking("main").await.state,
        GitTrackingState::InSync
    );
    fixture.commit("local");
    assert_eq!(fixture.tracking("main").await.ahead_by, Some(1));
    fixture.run(&["checkout", "--quiet", "--detach", &base]);
    fixture.commit("remote");
    let remote = fixture.run(&["rev-parse", "HEAD"]);
    fixture.run(&["update-ref", "refs/remotes/origin/main", &remote]);
    let divergent = fixture.tracking("main").await;
    assert_eq!(divergent.state, GitTrackingState::Diverged);
    assert_eq!(
        (divergent.ahead_by, divergent.behind_by),
        (Some(1), Some(1))
    );
    fixture.run(&["branch", "behind", &base]);
    fixture.run(&["branch", "--set-upstream-to=origin/main", "behind"]);
    assert_eq!(
        fixture.tracking("behind").await.state,
        GitTrackingState::Behind
    );
    fixture.run(&["update-ref", "-d", "refs/remotes/origin/main"]);
    assert_eq!(
        fixture.tracking("main").await.state,
        GitTrackingState::UpstreamMissing
    );
    fixture.run(&["branch", "unconfigured", &base]);
    assert_eq!(
        fixture.tracking("unconfigured").await.state,
        GitTrackingState::NoUpstream
    );
}

#[tokio::test]
async fn native_local_and_non_origin_upstream_are_preserved() {
    let fixture = Fixture::new();
    fixture.run(&["branch", "local-base"]);
    fixture.run(&["branch", "--set-upstream-to=local-base", "main"]);
    let tracking = fixture.tracking("main").await;
    assert_eq!(tracking.upstream.as_deref(), Some("refs/heads/local-base"));
    assert!(tracking.remote.is_none());
    fixture.run(&[
        "remote",
        "add",
        "upstream",
        "https://example.invalid/also-never-contacted.git",
    ]);
    let head = fixture.run(&["rev-parse", "HEAD"]);
    fixture.run(&["update-ref", "refs/remotes/upstream/release", &head]);
    fixture.run(&["branch", "--set-upstream-to=upstream/release", "main"]);
    let tracking = fixture.tracking("main").await;
    assert_eq!(
        tracking.upstream.as_deref(),
        Some("refs/remotes/upstream/release")
    );
    assert_eq!(tracking.remote.as_deref(), Some("upstream"));
}

#[tokio::test]
async fn native_binding_drift_detached_and_foreign_repository_fail_closed_only_for_metadata() {
    let fixture = Fixture::new();
    let repository = Git::default().discover(fixture.path()).unwrap();
    let other = Fixture::new();
    assert!(
        Git::default()
            .tracking_snapshot(other.path(), &repository.id)
            .await
            .is_err()
    );
    fixture.run(&["checkout", "--quiet", "--detach"]);
    let snapshot = fixture.snapshot().await;
    assert_eq!(
        snapshot
            .worktree(&repository.root_path, WorktreeMode::Detached, None, None)
            .state,
        GitTrackingState::Detached
    );
    assert_eq!(
        snapshot
            .worktree(
                &repository.root_path,
                WorktreeMode::NewBranch,
                Some("main"),
                None
            )
            .state,
        GitTrackingState::Unknown
    );
}

#[tokio::test]
async fn native_incomplete_history_is_not_reported_as_exact_counts() {
    let fixture = Fixture::new();
    let head = fixture.run(&["rev-parse", "HEAD"]);
    std::fs::write(fixture.path().join(".git/shallow"), format!("{head}\n")).unwrap();
    let tracking = fixture.tracking("main").await;
    assert_eq!(tracking.state, GitTrackingState::Unknown);
    assert!(tracking.ahead_by.is_none() && tracking.behind_by.is_none());
    assert_eq!(
        tracking.upstream.as_deref(),
        Some("refs/remotes/origin/main")
    );
}

#[tokio::test]
async fn linked_worktree_cannot_replace_its_admin_binding_with_the_common_directory() {
    let fixture = Fixture::new();
    let directories = tempfile::tempdir().unwrap();
    let path = directories.path().join("child");
    fixture.run(&["worktree", "add", "-b", "child", path.to_str().unwrap()]);
    let path = std::fs::canonicalize(path).unwrap();
    let marker = path.join(".git");
    let original = std::fs::read(&marker).unwrap();
    let git = Git::default();
    let repository = git.discover(fixture.path()).unwrap();
    std::fs::write(
        &marker,
        format!("gitdir: {}\n", repository.git_common_dir.display()),
    )
    .unwrap();
    for root in [&repository.root_path, &path] {
        let snapshot = git.tracking_snapshot(root, &repository.id).await.unwrap();
        assert_eq!(
            snapshot
                .worktree(&path, WorktreeMode::NewBranch, Some("child"), None)
                .state,
            GitTrackingState::Unknown
        );
    }
    std::fs::write(&marker, original).unwrap();
    assert_eq!(
        fixture
            .snapshot()
            .await
            .worktree(&path, WorktreeMode::NewBranch, Some("child"), None)
            .state,
        GitTrackingState::NoUpstream
    );
}

#[tokio::test]
async fn review_regression_linked_worktree_rejects_foreign_common_directory_at_same_commit() {
    let fixture = Fixture::new();
    let foreign = Fixture::new();
    let directories = tempfile::tempdir().unwrap();
    let path = directories.path().join("child");
    fixture.run(&["worktree", "add", "-b", "child", path.to_str().unwrap()]);
    fixture.run(&["branch", "--set-upstream-to=main", "child"]);
    foreign.run(&[
        "fetch",
        "--quiet",
        "--no-auto-gc",
        fixture.path().to_str().unwrap(),
        "main",
    ]);
    foreign.run(&["branch", "child", "FETCH_HEAD"]);
    assert_eq!(
        fixture.run(&["rev-parse", "refs/heads/child"]),
        foreign.run(&["rev-parse", "refs/heads/child"])
    );
    assert_eq!(
        foreign.tracking("child").await.state,
        GitTrackingState::NoUpstream
    );
    let path = std::fs::canonicalize(path).unwrap();
    let repository = Git::default().discover(fixture.path()).unwrap();
    let common_file = repository.git_common_dir.join("worktrees/child/commondir");
    let original = std::fs::read(&common_file).unwrap();
    std::fs::write(
        &common_file,
        foreign.path().join(".git").as_os_str().as_encoded_bytes(),
    )
    .unwrap();
    assert_eq!(
        fixture
            .snapshot()
            .await
            .worktree(&path, WorktreeMode::NewBranch, Some("child"), None)
            .state,
        GitTrackingState::Unknown,
        "a matching branch and commit cannot validate a foreign common directory"
    );
    for invalid in [
        String::new(),
        "missing\n".into(),
        "../..\nextra\n".into(),
        "x".repeat(8193),
    ] {
        std::fs::write(&common_file, invalid).unwrap();
        assert!(!binding::registered(&path, &repository.git_common_dir, false).await);
    }
    for valid in [
        original,
        repository
            .git_common_dir
            .as_os_str()
            .as_encoded_bytes()
            .to_vec(),
    ] {
        std::fs::write(&common_file, valid).unwrap();
        assert_eq!(
            fixture
                .snapshot()
                .await
                .worktree(&path, WorktreeMode::NewBranch, Some("child"), None)
                .state,
            GitTrackingState::InSync,
            "relative and absolute common-directory bindings remain supported"
        );
    }
}

#[tokio::test]
async fn missing_or_replaced_worktree_directories_are_not_mistaken_for_live_bindings() {
    let fixture = Fixture::new();
    let directories = tempfile::tempdir().unwrap();
    let path = directories.path().join("child");
    fixture.run(&["worktree", "add", "-b", "child", path.to_str().unwrap()]);
    let path = std::fs::canonicalize(path).unwrap();
    let snapshot = fixture.snapshot().await;
    assert_eq!(
        snapshot
            .worktree(&path, WorktreeMode::NewBranch, Some("child"), None)
            .state,
        GitTrackingState::NoUpstream
    );
    std::fs::rename(&path, directories.path().join("saved")).unwrap();
    assert_eq!(
        fixture
            .snapshot()
            .await
            .worktree(&path, WorktreeMode::NewBranch, Some("child"), None)
            .state,
        GitTrackingState::Unknown
    );
    std::fs::create_dir(&path).unwrap();
    let output = std::process::Command::new("git")
        .args(["init", "--quiet"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        fixture
            .snapshot()
            .await
            .worktree(&path, WorktreeMode::NewBranch, Some("child"), None)
            .state,
        GitTrackingState::Unknown
    );
}
