use super::*;

#[test]
fn creates_and_observes_a_detached_worktree() {
    let fixture = Fixture::new();
    let git = Git::default();
    let repository = git.discover(&fixture.source).unwrap();
    let base = git.resolve_commit(&repository, "HEAD").unwrap();
    let plan = git
        .plan_worktree(
            &repository,
            &fixture.worktrees,
            "detached",
            WorktreeTarget::Detached,
            &base,
        )
        .unwrap();

    let binding = git.create_worktree(&repository, &plan).unwrap();

    assert_eq!(binding.mode, WorktreeMode::Detached);
    assert_eq!(binding.branch_name, None);
    assert_eq!(binding.head_sha, base);
    let observation = git
        .observe(
            &repository,
            &binding.path,
            WorktreeMode::Detached,
            None,
            &base,
        )
        .unwrap();
    assert_eq!(observation.branch_name, None);
    assert_eq!(observation.base_relation, BaseRelation::AtBase);
}

#[test]
fn checks_out_an_existing_branch_with_the_same_workspace_name() {
    let fixture = Fixture::new();
    let git = Git::default();
    let repository = git.discover(&fixture.source).unwrap();
    run(&fixture.source, ["branch", "feat/existing"]);
    let base = git
        .resolve_commit(&repository, "refs/heads/feat/existing")
        .unwrap();
    let plan = git
        .plan_worktree(
            &repository,
            &fixture.worktrees,
            "feat/existing",
            WorktreeTarget::ExistingBranch {
                branch_name: "feat/existing".to_owned(),
            },
            &base,
        )
        .unwrap();

    let binding = git.create_worktree(&repository, &plan).unwrap();

    assert_eq!(binding.mode, WorktreeMode::ExistingBranch);
    assert_eq!(binding.branch_name.as_deref(), Some("feat/existing"));
    assert_eq!(binding.head_sha, base);
    assert_eq!(
        git_output(&binding.path, ["symbolic-ref", "--short", "HEAD"]),
        "feat/existing"
    );
}

#[test]
fn refuses_to_share_an_existing_branch_between_worktrees() {
    let fixture = Fixture::new();
    let git = Git::default();
    let repository = git.discover(&fixture.source).unwrap();
    let base = git.resolve_commit(&repository, "refs/heads/main").unwrap();

    let error = git
        .plan_worktree(
            &repository,
            &fixture.worktrees,
            "also-main",
            WorktreeTarget::ExistingBranch {
                branch_name: "main".to_owned(),
            },
            &base,
        )
        .unwrap_err();

    assert!(matches!(
        error,
        GitError::BranchAlreadyCheckedOut { branch, path }
            if branch == "main" && path == fixture.source
    ));
}

#[test]
fn rejects_a_missing_existing_branch() {
    let fixture = Fixture::new();
    let git = Git::default();
    let repository = git.discover(&fixture.source).unwrap();
    let base = git.resolve_commit(&repository, "HEAD").unwrap();

    let error = git
        .plan_worktree(
            &repository,
            &fixture.worktrees,
            "missing",
            WorktreeTarget::ExistingBranch {
                branch_name: "feat/missing".to_owned(),
            },
            &base,
        )
        .unwrap_err();

    assert!(matches!(error, GitError::BranchNotFound(branch) if branch == "feat/missing"));
}

fn git_output<const N: usize>(cwd: &Path, args: [&str; N]) -> String {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
