use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

use crate::domain::BaseRelation;

use super::worktree::validate_workspace_name;
use super::{Git, GitError};

struct Fixture {
    _temp: TempDir,
    source: PathBuf,
    worktrees: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        run(
            temp.path(),
            ["init", "--initial-branch=main", source.to_str().unwrap()],
        );
        run(&source, ["config", "user.name", "CoCo Tests"]);
        run(&source, ["config", "user.email", "coco@example.invalid"]);
        fs::write(source.join("README.md"), "fixture\n").unwrap();
        run(&source, ["add", "README.md"]);
        run(&source, ["commit", "-m", "fixture"]);
        let worktrees = temp.path().join("worktrees");
        Self {
            _temp: temp,
            source,
            worktrees,
        }
    }
}

#[test]
fn repository_identity_is_stable_across_worktrees() {
    let fixture = Fixture::new();
    let git = Git::default();
    let repository = git.discover(&fixture.source).unwrap();
    let base = git.resolve_commit(&repository, "HEAD").unwrap();
    let plan = git
        .plan_worktree(&repository, &fixture.worktrees, "stable", &base)
        .unwrap();
    let binding = git.create_worktree(&repository, &plan).unwrap();
    let linked = git.discover(&binding.path).unwrap();

    assert_eq!(linked.id, repository.id);
    assert_eq!(linked.git_common_dir, repository.git_common_dir);
    assert_ne!(linked.root_path, repository.root_path);
    assert!(linked.is_linked_worktree);
    assert_eq!(binding.branch_name, "coco/stable");
    assert_eq!(binding.head_sha, base);
}

#[test]
fn dirty_source_and_collisions_are_rejected() {
    let fixture = Fixture::new();
    let git = Git::default();
    let repository = git.discover(&fixture.source).unwrap();
    let base = git.resolve_commit(&repository, "HEAD").unwrap();
    let plan = git
        .plan_worktree(&repository, &fixture.worktrees, "collision", &base)
        .unwrap();
    git.create_worktree(&repository, &plan).unwrap();

    assert!(matches!(
        git.plan_worktree(&repository, &fixture.worktrees, "collision", &base),
        Err(GitError::BranchCollision { .. })
    ));
    fs::write(fixture.source.join("untracked.txt"), "dirty\n").unwrap();
    assert!(matches!(
        git.assert_clean(&repository),
        Err(GitError::DirtyRepository(_))
    ));
}

#[test]
fn diff_and_observation_include_commits_and_untracked_paths() {
    let fixture = Fixture::new();
    let git = Git::default();
    let repository = git.discover(&fixture.source).unwrap();
    let base = git.resolve_commit(&repository, "HEAD").unwrap();
    let plan = git
        .plan_worktree(&repository, &fixture.worktrees, "inspect", &base)
        .unwrap();
    let binding = git.create_worktree(&repository, &plan).unwrap();

    fs::write(binding.path.join("README.md"), "changed\n").unwrap();
    run(&binding.path, ["add", "README.md"]);
    run(&binding.path, ["commit", "-m", "change"]);
    fs::write(binding.path.join("untracked.txt"), "new\n").unwrap();

    let diff = git.diff(&binding.path, &base).unwrap();
    assert!(String::from_utf8_lossy(&diff.tracked_patch).contains("+changed"));
    assert_eq!(diff.untracked_paths, [PathBuf::from("untracked.txt")]);

    let observation = git
        .observe(&repository, &binding.path, "coco/inspect", &base)
        .unwrap();
    assert_eq!(observation.base_relation, BaseRelation::Descendant);
    assert_eq!(observation.ahead_by, Some(1));
    assert_eq!(observation.behind_by, Some(0));
    assert!(observation.dirty);
    assert!(observation.binding_valid);
}

#[test]
fn validates_workspace_names_with_safe_slash_components() {
    assert!(validate_workspace_name("workspace-42").is_ok());
    assert!(validate_workspace_name("feat/login-42").is_ok());
    assert!(validate_workspace_name("fix/api/v2").is_ok());
    for invalid in [
        "",
        "UPPER",
        "-leading",
        "trailing-",
        "/leading",
        "trailing/",
        "path//name",
        "path/-name",
        "path/name-",
        ".",
        "..",
        "a_b",
    ] {
        assert!(matches!(
            validate_workspace_name(invalid),
            Err(GitError::InvalidWorkspaceName(_))
        ));
    }
    assert!(validate_workspace_name(&"a".repeat(64)).is_err());
}

#[test]
fn creates_nested_workspace_paths_and_rejects_ref_prefix_collisions() {
    let fixture = Fixture::new();
    let git = Git::default();
    let repository = git.discover(&fixture.source).unwrap();
    let base = git.resolve_commit(&repository, "HEAD").unwrap();
    let plan = git
        .plan_worktree(&repository, &fixture.worktrees, "feat/login", &base)
        .unwrap();
    assert_eq!(
        plan.path,
        fixture
            .worktrees
            .join(&repository.id)
            .join("feat")
            .join("login")
    );
    let binding = git.create_worktree(&repository, &plan).unwrap();
    assert_eq!(binding.branch_name, "coco/feat/login");

    let error = git
        .plan_worktree(&repository, &fixture.worktrees, "feat", &base)
        .unwrap_err();
    assert!(matches!(
        error,
        GitError::BranchCollision { requested, existing }
            if requested == "coco/feat" && existing == "coco/feat/login"
    ));
}

#[test]
fn rejects_an_unsafe_nested_workspace_parent() {
    let fixture = Fixture::new();
    let git = Git::default();
    let repository = git.discover(&fixture.source).unwrap();
    let base = git.resolve_commit(&repository, "HEAD").unwrap();
    let repository_root = fixture.worktrees.join(&repository.id);
    fs::create_dir_all(&repository_root).unwrap();
    fs::write(repository_root.join("feat"), "not a directory").unwrap();

    assert!(matches!(
        git.plan_worktree(&repository, &fixture.worktrees, "feat/login", &base),
        Err(GitError::DestinationExists(_))
    ));
}

#[test]
fn rejects_an_existing_branch_that_is_a_parent_of_the_requested_ref() {
    let fixture = Fixture::new();
    let git = Git::default();
    let repository = git.discover(&fixture.source).unwrap();
    let base = git.resolve_commit(&repository, "HEAD").unwrap();
    run(&fixture.source, ["branch", "coco/chore"]);

    let error = git
        .plan_worktree(&repository, &fixture.worktrees, "chore/docs", &base)
        .unwrap_err();
    assert!(matches!(
        error,
        GitError::BranchCollision { requested, existing }
            if requested == "coco/chore/docs" && existing == "coco/chore"
    ));
}

fn run<const N: usize>(cwd: &Path, args: [&str; N]) {
    let result = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}
