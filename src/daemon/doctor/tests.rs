use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};

#[tokio::test]
async fn doctor_checks_private_file_metadata_without_returning_its_contents() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("credentials");
    std::fs::write(&path, "sensitive-capability-value").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let check = private_file_check(
        "daemon.token",
        "Jump credentials",
        &path,
        Instant::now() + PROBE_TIMEOUT,
    )
    .await;
    assert_eq!(check.status, DiagnosticStatus::Ok);
    assert!(
        !serde_json::to_string(&check)
            .unwrap()
            .contains("sensitive-capability")
    );
}

#[tokio::test]
async fn readable_by_other_users_and_symlinked_runtime_files_are_errors() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("credentials");
    std::fs::write(&path, "test-credential").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    let check = private_file_check(
        "daemon.token",
        "Jump credentials",
        &path,
        Instant::now() + PROBE_TIMEOUT,
    )
    .await;
    assert_eq!(check.status, DiagnosticStatus::Error);
    let link = temporary.path().join("link");
    symlink(&path, &link).unwrap();
    let check = private_file_check(
        "daemon.token",
        "Jump credentials",
        &link,
        Instant::now() + PROBE_TIMEOUT,
    )
    .await;
    assert_eq!(check.status, DiagnosticStatus::Error);
}

#[tokio::test]
async fn missing_runtime_files_have_an_actionable_hint() {
    let temporary = tempfile::tempdir().unwrap();
    let check = private_file_check(
        "daemon.token",
        "Jump credentials",
        &temporary.path().join("missing"),
        Instant::now() + PROBE_TIMEOUT,
    )
    .await;
    assert_eq!(check.status, DiagnosticStatus::Error);
    assert!(check.hint.as_ref().unwrap().contains("Stop active work"));
}
