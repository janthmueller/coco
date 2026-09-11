use super::*;
use crate::git::LocalStatePolicy;
use std::fs::File;

#[test]
fn carries_staged_and_unstaged_tracked_changes_without_mutating_the_source() {
    let fixture = Fixture::new();
    let git = Git::default();
    let repository = git.discover(&fixture.source).unwrap();
    let base = git.resolve_commit(&repository, "HEAD").unwrap();
    fs::write(fixture.source.join("README.md"), "staged\n").unwrap();
    run(&fixture.source, ["add", "README.md"]);
    fs::write(fixture.source.join("README.md"), "unstaged\n").unwrap();
    let source_status = git_output(&fixture.source, ["status", "--porcelain=v1"]);

    let snapshot = git
        .snapshot_local_state(&repository, &base, LocalStatePolicy::CarryTracked)
        .unwrap();
    let plan = git
        .plan_worktree(
            &repository,
            &fixture.worktrees,
            "dirty",
            WorktreeTarget::Detached,
            &base,
        )
        .unwrap();
    let binding = git.create_worktree(&repository, &plan).unwrap();
    git.apply_local_state(&binding.path, &snapshot).unwrap();

    assert_eq!(git_output(&binding.path, ["show", ":README.md"]), "staged");
    assert_eq!(
        fs::read_to_string(binding.path.join("README.md")).unwrap(),
        "unstaged\n"
    );
    assert_eq!(
        git_output(&fixture.source, ["status", "--porcelain=v1"]),
        source_status
    );
    assert_eq!(snapshot.manifest().source_path, fixture.source);
    assert_eq!(snapshot.manifest().source_head, base);
    assert!(snapshot.manifest().carried_tracked_changes);
    assert!(snapshot.manifest().staged_patch_bytes > 0);
    assert!(snapshot.manifest().unstaged_patch_bytes > 0);
}

#[test]
fn rejects_untracked_files_and_a_different_dirty_base() {
    let fixture = Fixture::new();
    let git = Git::default();
    let repository = git.discover(&fixture.source).unwrap();
    let base = git.resolve_commit(&repository, "HEAD").unwrap();
    fs::write(fixture.source.join("new.txt"), "untracked\n").unwrap();
    assert!(matches!(
        git.snapshot_local_state(&repository, &base, LocalStatePolicy::CarryTracked),
        Err(GitError::UntrackedChanges(path)) if path.as_path() == Path::new("new.txt")
    ));

    fs::remove_file(fixture.source.join("new.txt")).unwrap();
    fs::write(fixture.source.join("README.md"), "changed\n").unwrap();
    assert!(matches!(
        git.snapshot_local_state(&repository, &"0".repeat(40), LocalStatePolicy::CarryTracked,),
        Err(GitError::LocalChangesBaseMismatch { .. })
    ));
}

#[test]
fn ignores_source_changes_independently_from_the_selected_base() {
    let fixture = Fixture::new();
    let git = Git::default();
    run(&fixture.source, ["branch", "alternate-base"]);
    fs::write(fixture.source.join("main-only.txt"), "committed on main\n").unwrap();
    run(&fixture.source, ["add", "main-only.txt"]);
    run(&fixture.source, ["commit", "-m", "advance main"]);
    fs::write(fixture.source.join("README.md"), "local tracked change\n").unwrap();
    fs::write(
        fixture.source.join("local-only.txt"),
        "local untracked file\n",
    )
    .unwrap();

    let repository = git.discover(&fixture.source).unwrap();
    let base = git.resolve_commit(&repository, "alternate-base").unwrap();
    let source_status = git_output(&fixture.source, ["status", "--porcelain=v1"]);
    let snapshot = git
        .snapshot_local_state(&repository, &base, LocalStatePolicy::IgnoreChanges)
        .unwrap();
    let plan = git
        .plan_worktree(
            &repository,
            &fixture.worktrees,
            "ignore-source",
            WorktreeTarget::Detached,
            &base,
        )
        .unwrap();
    let binding = git.create_worktree(&repository, &plan).unwrap();
    git.apply_local_state(&binding.path, &snapshot).unwrap();

    assert_eq!(
        fs::read_to_string(binding.path.join("README.md")).unwrap(),
        "fixture\n"
    );
    assert!(!binding.path.join("main-only.txt").exists());
    assert!(!binding.path.join("local-only.txt").exists());
    assert!(git_output(&binding.path, ["status", "--porcelain=v1"]).is_empty());
    assert_eq!(
        git_output(&fixture.source, ["status", "--porcelain=v1"]),
        source_status
    );
    assert!(!snapshot.manifest().carried_tracked_changes);
    assert_eq!(snapshot.manifest().untracked_file_count, 0);
}

#[test]
fn copies_only_ignored_files_selected_by_worktreeinclude() {
    let fixture = Fixture::new();
    let git = Git::default();
    fs::write(
        fixture.source.join(".gitignore"),
        ".env\ncache/\nAGENTS.override.md\n",
    )
    .unwrap();
    fs::write(
        fixture.source.join(".worktreeinclude"),
        ".env\ncache/*.json\n",
    )
    .unwrap();
    run(&fixture.source, ["add", ".gitignore", ".worktreeinclude"]);
    run(&fixture.source, ["commit", "-m", "worktree includes"]);
    fs::create_dir(fixture.source.join("cache")).unwrap();
    fs::write(fixture.source.join(".env"), "TOKEN=local\n").unwrap();
    fs::write(fixture.source.join("cache/settings.json"), "{}\n").unwrap();
    fs::write(fixture.source.join("cache/skip.txt"), "skip\n").unwrap();
    fs::write(
        fixture.source.join("AGENTS.override.md"),
        "local instructions\n",
    )
    .unwrap();
    let repository = git.discover(&fixture.source).unwrap();
    let base = git.resolve_commit(&repository, "HEAD").unwrap();

    let snapshot = git
        .snapshot_local_state(&repository, &base, LocalStatePolicy::RequireClean)
        .unwrap();
    let plan = git
        .plan_worktree(
            &repository,
            &fixture.worktrees,
            "included",
            WorktreeTarget::Detached,
            &base,
        )
        .unwrap();
    let binding = git.create_worktree(&repository, &plan).unwrap();
    git.apply_local_state(&binding.path, &snapshot).unwrap();

    assert_eq!(
        fs::read_to_string(binding.path.join(".env")).unwrap(),
        "TOKEN=local\n"
    );
    assert!(binding.path.join("cache/settings.json").is_file());
    assert!(!binding.path.join("cache/skip.txt").exists());
    assert!(binding.path.join("AGENTS.override.md").is_file());
    assert_eq!(snapshot.manifest().included_file_count, 3);
}

#[cfg(unix)]
#[test]
fn skips_source_symlinks_and_refuses_destination_overwrites() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new();
    let git = Git::default();
    fs::write(fixture.source.join(".gitignore"), "local-*\n").unwrap();
    fs::write(fixture.source.join(".worktreeinclude"), "local-*\n").unwrap();
    run(&fixture.source, ["add", ".gitignore", ".worktreeinclude"]);
    run(&fixture.source, ["commit", "-m", "worktree includes"]);
    fs::write(fixture.source.join("local-real"), "real\n").unwrap();
    symlink("local-real", fixture.source.join("local-link")).unwrap();
    let repository = git.discover(&fixture.source).unwrap();
    let base = git.resolve_commit(&repository, "HEAD").unwrap();
    let snapshot = git
        .snapshot_local_state(&repository, &base, LocalStatePolicy::RequireClean)
        .unwrap();
    assert_eq!(snapshot.manifest().included_file_count, 1);
    let plan = git
        .plan_worktree(
            &repository,
            &fixture.worktrees,
            "safe-copy",
            WorktreeTarget::Detached,
            &base,
        )
        .unwrap();
    let binding = git.create_worktree(&repository, &plan).unwrap();
    fs::write(binding.path.join("local-real"), "destination\n").unwrap();

    assert!(matches!(
        git.apply_local_state(&binding.path, &snapshot),
        Err(GitError::CarriedDestinationExists(path))
            if path.as_path() == Path::new("local-real")
    ));
    assert!(!binding.path.join("local-link").exists());
}

#[test]
fn carries_only_non_ignored_untracked_files_when_explicitly_requested() {
    let fixture = Fixture::new();
    let git = Git::default();
    fs::write(fixture.source.join(".gitignore"), "ignored.txt\n").unwrap();
    run(&fixture.source, ["add", ".gitignore"]);
    run(&fixture.source, ["commit", "-m", "ignore local file"]);
    fs::write(fixture.source.join("ordinary.txt"), "ordinary\n").unwrap();
    fs::write(fixture.source.join("ignored.txt"), "ignored\n").unwrap();
    let repository = git.discover(&fixture.source).unwrap();
    let base = git.resolve_commit(&repository, "HEAD").unwrap();

    let snapshot = git
        .snapshot_local_state(
            &repository,
            &base,
            LocalStatePolicy::CarryTrackedAndUntracked,
        )
        .unwrap();
    let plan = git
        .plan_worktree(
            &repository,
            &fixture.worktrees,
            "untracked",
            WorktreeTarget::Detached,
            &base,
        )
        .unwrap();
    let binding = git.create_worktree(&repository, &plan).unwrap();
    git.apply_local_state(&binding.path, &snapshot).unwrap();

    assert_eq!(
        fs::read_to_string(binding.path.join("ordinary.txt")).unwrap(),
        "ordinary\n"
    );
    assert!(!binding.path.join("ignored.txt").exists());
    assert_eq!(snapshot.manifest().untracked_file_count, 1);
    assert_eq!(snapshot.manifest().included_file_count, 0);
}

#[test]
fn rejects_worktreeinclude_selections_above_the_file_count_limit() {
    let fixture = Fixture::new();
    let git = Git::default();
    fs::write(fixture.source.join(".gitignore"), "private/\n").unwrap();
    fs::write(fixture.source.join(".worktreeinclude"), "private/\n").unwrap();
    run(&fixture.source, ["add", ".gitignore", ".worktreeinclude"]);
    run(&fixture.source, ["commit", "-m", "bound local files"]);
    fs::create_dir(fixture.source.join("private")).unwrap();
    for index in 0..257 {
        fs::write(fixture.source.join(format!("private/{index}.txt")), b"x").unwrap();
    }
    let repository = git.discover(&fixture.source).unwrap();
    let base = git.resolve_commit(&repository, "HEAD").unwrap();

    assert!(matches!(
        git.snapshot_local_state(&repository, &base, LocalStatePolicy::RequireClean),
        Err(GitError::CarriedFileLimit { limit: 256 })
    ));
}

#[test]
fn rejects_one_worktreeinclude_file_above_the_per_file_byte_limit() {
    let fixture = Fixture::new();
    let git = Git::default();
    fs::write(fixture.source.join(".gitignore"), "large.local\n").unwrap();
    fs::write(fixture.source.join(".worktreeinclude"), "large.local\n").unwrap();
    run(&fixture.source, ["add", ".gitignore", ".worktreeinclude"]);
    run(&fixture.source, ["commit", "-m", "bound local files"]);
    File::create(fixture.source.join("large.local"))
        .unwrap()
        .set_len(8 * 1024 * 1024 + 1)
        .unwrap();
    let repository = git.discover(&fixture.source).unwrap();
    let base = git.resolve_commit(&repository, "HEAD").unwrap();

    assert!(matches!(
        git.snapshot_local_state(&repository, &base, LocalStatePolicy::RequireClean),
        Err(GitError::CarriedByteLimit { limit: 8_388_608 })
    ));
}

fn git_output<const N: usize>(cwd: &Path, args: [&str; N]) -> String {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
