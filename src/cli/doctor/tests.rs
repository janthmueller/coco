use super::*;
use crate::cli::style::Palette;

mod connection;

fn report(checks: Vec<DiagnosticCheck>) -> DoctorReport {
    DoctorReport {
        cli_version: env!("CARGO_PKG_VERSION"),
        cli_executable: None,
        cocod_executable: None,
        codex_executable: None,
        daemon: None,
        checks,
        complete: true,
    }
}

#[test]
fn human_output_groups_healthy_bindings_without_duplicate_paths() {
    let report = report(vec![
        DiagnosticCheck::new("repository", "Repository", DiagnosticStatus::Ok, "valid")
            .subject("/private/one"),
        DiagnosticCheck::new("worktree", "Worktree", DiagnosticStatus::Ok, "valid")
            .subject("/private/two"),
        DiagnosticCheck::new(
            "thread",
            "Conversation",
            DiagnosticStatus::Skipped,
            "Prepared",
        )
        .subject("/private/three"),
    ]);
    let output = output::render(&report, Palette::plain());
    assert!(output.contains("Repositories · 1 checked"));
    assert!(output.contains("Conversations · 1 skipped"));
    assert!(!output.contains("/private/"));
    assert!(!output.contains('\u{1b}'));
}

#[test]
fn issues_keep_their_subject_hint_and_escape_terminal_controls() {
    let report = report(vec![
        DiagnosticCheck::new("worktree", "Worktree", DiagnosticStatus::Error, "mismatch")
            .subject("/repo: name\u{1b}[2J\nextra")
            .hint("Inspect the branch"),
    ]);
    let output = output::render(&report, Palette::plain());
    assert!(output.contains("mismatch"));
    assert!(output.contains("Inspect the branch"));
    assert!(output.contains("\\u{1b}[2J\\nextra"));
    assert!(!output.contains('\u{1b}'));
    assert!(output.contains("1 errors, 0 warnings"));
}

#[test]
fn warnings_and_incomplete_coverage_are_visible_without_claiming_success() {
    let mut report = report(vec![DiagnosticCheck::new(
        "coverage",
        "Coverage",
        DiagnosticStatus::Warning,
        "limit reached",
    )]);
    report.complete = false;
    let output = output::render(&report, Palette::plain());
    assert!(output.contains("limit reached"));
    assert!(output.contains("some checks incomplete"));
    assert!(output::render(&report, Palette::colored()).contains('\u{1b}'));
}

#[cfg(unix)]
#[tokio::test]
async fn invalid_version_output_and_stderr_never_reach_the_report() {
    for success in [false, true] {
        let mut command = Command::new("/bin/sh");
        command.args([
            "-c",
            if success {
                "printf 'sensitive-output'; printf 'sensitive-error' >&2; exit 0"
            } else {
                "printf 'sensitive-output'; printf 'sensitive-error' >&2; exit 1"
            },
        ]);
        let result = capture_command(command).await;
        assert_eq!(
            result,
            if success {
                Ok("sensitive-output".into())
            } else {
                Err(ProbeError::Failed)
            }
        );
        let mut checks = Vec::new();
        assert!(record_version_result("codex", "codex-cli ", result, &mut checks).is_none());
        let serialized = serde_json::to_string(&checks).unwrap();
        assert!(!serialized.contains("sensitive-"));
        assert_eq!(checks[0].status, DiagnosticStatus::Error);
    }
}

#[tokio::test]
async fn a_missing_binary_is_a_reported_error_not_an_early_exit() {
    let mut checks = Vec::new();
    assert!(
        version_check("codex", None, "codex-cli ", &mut checks)
            .await
            .is_none()
    );
    assert_eq!(checks.len(), 1);
    assert_eq!(checks[0].status, DiagnosticStatus::Error);
    assert!(
        checks[0]
            .hint
            .as_ref()
            .unwrap()
            .contains("COCO_CODEX_BINARY")
    );
}
