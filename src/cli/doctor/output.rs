use std::fmt::Write;

use crate::protocol::{DiagnosticCheck, DiagnosticStatus};

use super::DoctorReport;
use crate::cli::style::{Palette, Tone};

pub(super) fn render(report: &DoctorReport, palette: Palette) -> String {
    let mut output = String::new();
    for check in report
        .checks
        .iter()
        .filter(|check| check.id != "installation.live" || check.status != DiagnosticStatus::Ok)
        .filter(|check| !matches!(check.id.as_str(), "repository" | "worktree" | "thread"))
    {
        render_check(&mut output, check, palette);
    }
    for (id, label) in [
        ("repository", "Repositories"),
        ("worktree", "Worktrees"),
        ("thread", "Conversations"),
    ] {
        let checks: Vec<_> = report
            .checks
            .iter()
            .filter(|check| check.id == id)
            .collect();
        if checks.is_empty() {
            continue;
        }
        let ok = checks
            .iter()
            .filter(|check| check.status == DiagnosticStatus::Ok)
            .count();
        let skipped = checks
            .iter()
            .filter(|check| check.status == DiagnosticStatus::Skipped)
            .count();
        let errors = checks.len() - ok - skipped;
        let status = if checks
            .iter()
            .any(|check| check.status == DiagnosticStatus::Error)
        {
            DiagnosticStatus::Error
        } else if errors > 0 {
            DiagnosticStatus::Warning
        } else if ok == 0 {
            DiagnosticStatus::Skipped
        } else {
            DiagnosticStatus::Ok
        };
        render_check(
            &mut output,
            &DiagnosticCheck::new(id, label, status, summary(ok, skipped, errors)),
            palette,
        );
        for check in checks.into_iter().filter(|check| {
            matches!(
                check.status,
                DiagnosticStatus::Warning | DiagnosticStatus::Error
            )
        }) {
            render_check(&mut output, check, palette);
        }
    }
    let errors = report
        .checks
        .iter()
        .filter(|check| check.status == DiagnosticStatus::Error)
        .count();
    let warnings = report
        .checks
        .iter()
        .filter(|check| check.status == DiagnosticStatus::Warning)
        .count();
    let _ = writeln!(
        output,
        "\n{errors} errors, {warnings} warnings{}.",
        if report.complete {
            ""
        } else {
            "; some checks incomplete"
        }
    );
    output
}

fn summary(ok: usize, skipped: usize, issues: usize) -> String {
    let mut parts = Vec::new();
    if ok > 0 {
        parts.push(format!("{ok} checked"));
    }
    if skipped > 0 {
        parts.push(format!("{skipped} skipped"));
    }
    if issues > 0 {
        parts.push(format!(
            "{issues} {}",
            if issues == 1 { "issue" } else { "issues" }
        ));
    }
    parts.join(", ")
}

fn render_check(output: &mut String, check: &DiagnosticCheck, palette: Palette) {
    let (symbol, tone) = match check.status {
        DiagnosticStatus::Ok => ("✓", Tone::Green),
        DiagnosticStatus::Warning => ("!", Tone::YellowBold),
        DiagnosticStatus::Error => ("✗", Tone::Red),
        DiagnosticStatus::Skipped => ("–", Tone::Dim),
    };
    let _ = writeln!(
        output,
        "{} {}{} · {}",
        palette.paint(tone, symbol),
        clean(&check.label),
        check
            .subject
            .as_ref()
            .map(|subject| format!(" [{}]", clean(subject)))
            .unwrap_or_default(),
        clean(&check.message)
    );
    if let Some(hint) = &check.hint {
        let _ = writeln!(output, "  {}", palette.paint(Tone::Dim, clean(hint)));
    }
}

fn clean(value: &str) -> String {
    value
        .chars()
        .take(512)
        .flat_map(|character| {
            if character.is_control() {
                character.escape_default().collect::<Vec<_>>()
            } else {
                vec![character]
            }
        })
        .collect()
}
