use super::*;
use crate::domain::git_source::GitChangeSummary;

#[test]
fn source_reports_real_head_changes_and_cached_branches_without_writes() {
    let fixture = Fixture::new();
    run(&fixture.source, ["branch", "free"]);
    run(
        &fixture.source,
        ["update-ref", "refs/remotes/origin/main", "HEAD"],
    );
    run(
        &fixture.source,
        [
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ],
    );
    run(
        &fixture.source,
        ["commit", "--allow-empty", "-m", "advance main"],
    );
    std::fs::write(fixture.source.join("README.md"), "changed").unwrap();
    std::fs::write(fixture.source.join("new.txt"), "untracked").unwrap();
    std::fs::write(fixture.source.join(".git/info/exclude"), "ignored.txt\n").unwrap();
    std::fs::write(fixture.source.join("ignored.txt"), "ignored").unwrap();
    let git = Git::default();
    let repository = git.discover(&fixture.source).unwrap();
    let index = std::fs::read(fixture.source.join(".git/index")).unwrap();
    let state = git.inspect_source(&repository, true).unwrap();
    assert_eq!(state.branch.as_deref(), Some("main"));
    assert_eq!(
        state.head_sha,
        Some(git.resolve_commit(&repository, "HEAD").unwrap())
    );
    assert_eq!(
        state.changes,
        GitChangeSummary {
            tracked_files: 1,
            untracked_files: 1
        }
    );
    assert_eq!(state.branches.len(), 3);
    assert_ne!(
        state
            .branches
            .iter()
            .find(|branch| branch.remote)
            .unwrap()
            .commit,
        state.head_sha.clone().unwrap()
    );
    assert!(
        state
            .branches
            .iter()
            .any(|branch| branch.name == "main" && branch.checkout_path.is_some())
    );
    assert!(
        state
            .branches
            .iter()
            .any(|branch| branch.name == "free" && branch.checkout_path.is_none())
    );
    assert!(
        state
            .branches
            .iter()
            .any(|branch| branch.name == "origin/main" && branch.remote)
    );
    assert_eq!(
        index,
        std::fs::read(fixture.source.join(".git/index")).unwrap()
    );
}

#[test]
fn linked_and_detached_sources_keep_the_selected_checkout_identity() {
    let fixture = Fixture::new();
    let child = fixture._temp.path().join("child");
    run(
        &fixture.source,
        ["worktree", "add", "-b", "linked", child.to_str().unwrap()],
    );
    let git = Git::default();
    let state = git
        .inspect_source(&git.discover(&child).unwrap(), false)
        .unwrap();
    assert_eq!(state.branch.as_deref(), Some("linked"));
    assert_eq!(state.root_path, std::fs::canonicalize(&child).unwrap());
    run(&child, ["checkout", "--detach"]);
    let state = git
        .inspect_source(&git.discover(&child).unwrap(), false)
        .unwrap();
    assert!(state.branch.is_none());
    assert!(state.head_sha.is_some());
    assert!(state.branches.is_empty());
}

#[test]
fn unborn_checkout_has_a_name_without_a_commit() {
    let root = tempfile::tempdir().unwrap();
    run(root.path(), ["init", "--initial-branch=main"]);
    let git = Git::default();
    let state = git
        .inspect_source(&git.discover(root.path()).unwrap(), true)
        .unwrap();
    assert_eq!(state.branch.as_deref(), Some("main"));
    assert!(state.head_sha.is_none());
    assert!(state.branches.is_empty());
}
