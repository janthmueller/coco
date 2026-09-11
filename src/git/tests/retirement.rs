use super::*;

#[test]
fn retirement_observes_every_local_state_class_and_reopens_the_exact_branch() {
    let fixture = Fixture::new();
    let git = Git::default();
    let repository = git.discover(&fixture.source).unwrap();
    let base = git.resolve_commit(&repository, "HEAD").unwrap();
    fs::create_dir_all(repository.git_common_dir.join("info")).unwrap();
    fs::write(
        repository.git_common_dir.join("info/exclude"),
        "ignored.log\n",
    )
    .unwrap();
    let plan = git
        .plan_worktree(
            &repository,
            &fixture.worktrees,
            "retire",
            new_branch("coco/retire"),
            &base,
        )
        .unwrap();
    let binding = git.create_worktree(&repository, &plan).unwrap();
    fs::write(binding.path.join("README.md"), "changed\n").unwrap();
    fs::write(binding.path.join("new.txt"), "new\n").unwrap();
    fs::write(binding.path.join("ignored.log"), "ignored\n").unwrap();

    let observation = git
        .observe_worktree_retirement(
            &repository,
            &binding.path,
            WorktreeMode::NewBranch,
            Some("coco/retire"),
        )
        .unwrap();
    assert!(observation.tracked_changes);
    assert_eq!(observation.untracked_file_count, 1);
    assert_eq!(observation.ignored_file_count, 1);
    assert!(observation.has_local_changes());

    git.remove_worktree(&repository, &observation.binding, true)
        .unwrap();
    assert!(!binding.path.exists());
    let reopened = git
        .restore_worktree(
            &repository,
            &fixture.worktrees,
            "retire",
            &binding.path,
            WorktreeMode::NewBranch,
            Some("coco/retire"),
            &base,
            &base,
        )
        .unwrap();
    assert_eq!(reopened.head_sha, base);
    assert_eq!(
        fs::read_to_string(reopened.path.join("README.md")).unwrap(),
        "fixture\n"
    );
    assert!(!reopened.path.join("new.txt").exists());
    assert!(!reopened.path.join("ignored.log").exists());
}

#[test]
fn retirement_reports_locks_and_branch_movement_before_side_effects() {
    let fixture = Fixture::new();
    let git = Git::default();
    let repository = git.discover(&fixture.source).unwrap();
    let base = git.resolve_commit(&repository, "HEAD").unwrap();
    let plan = git
        .plan_worktree(
            &repository,
            &fixture.worktrees,
            "locked",
            new_branch("coco/locked"),
            &base,
        )
        .unwrap();
    let binding = git.create_worktree(&repository, &plan).unwrap();
    run(
        &fixture.source,
        [
            "worktree",
            "lock",
            "--reason",
            "test lock",
            binding.path.to_str().unwrap(),
        ],
    );
    let observation = git
        .observe_worktree_retirement(
            &repository,
            &binding.path,
            WorktreeMode::NewBranch,
            Some("coco/locked"),
        )
        .unwrap();
    assert_eq!(observation.lock_reason.as_deref(), Some("test lock"));
    assert!(matches!(
        git.remove_worktree(&repository, &binding, true),
        Err(GitError::WorktreeLocked { .. })
    ));
    run(
        &fixture.source,
        ["worktree", "unlock", binding.path.to_str().unwrap()],
    );
    git.remove_worktree(&repository, &binding, false).unwrap();
    let other = fixture.worktrees.join("other-checkout");
    run(
        &fixture.source,
        [
            "worktree",
            "add",
            "--",
            other.to_str().unwrap(),
            "coco/locked",
        ],
    );
    assert!(matches!(
        git.delete_created_branch(&repository, "coco/locked", &base, false),
        Err(GitError::BranchAlreadyCheckedOut { .. })
    ));
    run(
        &fixture.source,
        ["worktree", "remove", "--", other.to_str().unwrap()],
    );
    run(&fixture.source, ["branch", "-f", "coco/locked", "HEAD~0"]);
    let moved = git
        .resolve_local_branch(&repository, "coco/locked")
        .unwrap();
    assert_eq!(moved, base);
    // A different expected snapshot is rejected even when the branch itself is valid.
    assert!(matches!(
        git.delete_created_branch(&repository, "coco/locked", &"0".repeat(40), false),
        Err(GitError::BranchMoved { .. })
    ));
}

#[test]
fn retirement_reopens_an_exact_detached_binding() {
    let fixture = Fixture::new();
    let git = Git::default();
    let repository = git.discover(&fixture.source).unwrap();
    let base = git.resolve_commit(&repository, "HEAD").unwrap();
    let plan = git
        .plan_worktree(
            &repository,
            &fixture.worktrees,
            "detached-retire",
            WorktreeTarget::Detached,
            &base,
        )
        .unwrap();
    let binding = git.create_worktree(&repository, &plan).unwrap();
    git.remove_worktree(&repository, &binding, false).unwrap();

    let restored = git
        .restore_worktree(
            &repository,
            &fixture.worktrees,
            "detached-retire",
            &binding.path,
            WorktreeMode::Detached,
            None,
            &base,
            &binding.head_sha,
        )
        .unwrap();

    assert_eq!(restored.mode, WorktreeMode::Detached);
    assert_eq!(restored.branch_name, None);
    assert_eq!(restored.head_sha, base);
}

#[test]
fn removal_rechecks_ignored_state_immediately_before_git_is_allowed_to_remove() {
    let fixture = Fixture::new();
    let git = Git::default();
    let repository = git.discover(&fixture.source).unwrap();
    let base = git.resolve_commit(&repository, "HEAD").unwrap();
    fs::create_dir_all(repository.git_common_dir.join("info")).unwrap();
    fs::write(
        repository.git_common_dir.join("info/exclude"),
        "late-secret\n",
    )
    .unwrap();
    let plan = git
        .plan_worktree(
            &repository,
            &fixture.worktrees,
            "late-change",
            new_branch("coco/late-change"),
            &base,
        )
        .unwrap();
    let binding = git.create_worktree(&repository, &plan).unwrap();
    fs::write(binding.path.join("late-secret"), "keep\n").unwrap();

    assert!(matches!(
        git.remove_worktree(&repository, &binding, false),
        Err(GitError::DirtyWorktree(_))
    ));
    assert!(binding.path.join("late-secret").is_file());
}

#[cfg(unix)]
#[test]
fn removal_rejects_a_path_redirected_after_its_binding_was_observed() {
    let fixture = Fixture::new();
    let git = Git::default();
    let repository = git.discover(&fixture.source).unwrap();
    let base = git.resolve_commit(&repository, "HEAD").unwrap();
    let plan = git
        .plan_worktree(
            &repository,
            &fixture.worktrees,
            "retire-detached",
            WorktreeTarget::Detached,
            &base,
        )
        .unwrap();
    let binding = git.create_worktree(&repository, &plan).unwrap();
    let moved = fixture.worktrees.join("moved-original");
    let other = fixture.worktrees.join("unrelated");
    run(
        &fixture.source,
        [
            "worktree",
            "move",
            binding.path.to_str().unwrap(),
            moved.to_str().unwrap(),
        ],
    );
    run(
        &fixture.source,
        [
            "worktree",
            "add",
            "--detach",
            other.to_str().unwrap(),
            "HEAD",
        ],
    );
    fs::write(other.join("local.txt"), "unrelated local state\n").unwrap();
    std::os::unix::fs::symlink(&other, &binding.path).unwrap();

    assert!(matches!(
        git.remove_worktree(&repository, &binding, true),
        Err(GitError::BindingMismatch(_))
    ));
    assert!(moved.is_dir());
    assert_eq!(
        fs::read_to_string(other.join("local.txt")).unwrap(),
        "unrelated local state\n"
    );
}
