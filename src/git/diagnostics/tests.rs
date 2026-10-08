use std::fs;

use super::*;

fn repository() -> tempfile::TempDir {
    let temporary = unborn_repository();
    run_git(
        temporary.path(),
        &[
            "-c",
            "user.name=CoCo Tests",
            "-c",
            "user.email=coco@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "--allow-empty",
            "-m",
            "diagnostic fixture",
        ],
    );
    temporary
}

fn unborn_repository() -> tempfile::TempDir {
    let temporary = tempfile::tempdir().unwrap();
    run_git(
        temporary.path(),
        &["init", "--quiet", "--initial-branch=main"],
    );
    temporary
}

fn run_git(path: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .current_dir(path)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn diagnostic_command_preserves_global_trust_without_bypassing_ownership() {
    let temporary = repository();
    let configuration = tempfile::tempdir().unwrap();
    let git = Git::default();
    let command = || {
        let mut command = git.diagnostic_command(temporary.path());
        // Isolate only this child's global config. Never mutate the test
        // process environment or require changing actual file ownership.
        command
            .env_remove("HOME")
            .env("XDG_CONFIG_HOME", configuration.path())
            .env("GIT_TEST_ASSUME_DIFFERENT_OWNER", "1");
        command
    };
    assert_eq!(capture_command(command()).await, Err(ProbeError::Failed));

    let config = configuration.path().join("git/config");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    let output = std::process::Command::new("git")
        .args(["config", "--file"])
        .arg(&config)
        .args(["--add", "safe.directory"])
        .arg(fs::canonicalize(temporary.path()).unwrap())
        .output()
        .unwrap();
    assert!(output.status.success());
    let trusted = capture_command(command()).await;
    assert!(trusted.is_ok(), "trusted repository rejected: {trusted:?}");

    fs::write(&config, "").unwrap();
    assert_eq!(capture_command(command()).await, Err(ProbeError::Failed));
}

#[tokio::test]
async fn unborn_repository_identity_does_not_imply_a_valid_checkout() {
    let temporary = unborn_repository();
    let git = Git::default();
    let repository = git.discover(temporary.path()).unwrap();
    let inspected = git.diagnostic_repository(temporary.path()).await.unwrap();
    assert_eq!(inspected.root_path, repository.root_path);
    assert_eq!(inspected.common_dir, repository.git_common_dir);
    assert!(matches!(
        git.diagnostic_binding(temporary.path()).await,
        Err(ProbeError::Failed)
    ));
}

#[tokio::test]
async fn checkout_binding_keeps_branch_and_detached_metadata() {
    let temporary = repository();
    let git = Git::default();
    let repository = git.discover(temporary.path()).unwrap();
    let binding = git.diagnostic_binding(temporary.path()).await.unwrap();
    assert_eq!(binding.root_path, repository.root_path);
    assert_eq!(binding.common_dir, repository.git_common_dir);
    assert_eq!(binding.branch_name.as_deref(), Some("main"));

    run_git(temporary.path(), &["checkout", "--quiet", "--detach"]);
    let binding = git.diagnostic_binding(temporary.path()).await.unwrap();
    assert_eq!(binding.root_path, repository.root_path);
    assert_eq!(binding.common_dir, repository.git_common_dir);
    assert_eq!(binding.branch_name, None);
}
