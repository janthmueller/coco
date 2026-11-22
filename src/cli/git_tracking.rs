use crate::protocol::{GitTracking, GitTrackingState};

use super::style::{StyledText, Tone};

pub(super) fn badge(tracking: Option<&GitTracking>, detailed: bool) -> StyledText {
    let Some(tracking) = tracking else {
        return StyledText::default();
    };
    let (label, tone) = match (tracking.state, tracking.ahead_by, tracking.behind_by) {
        (GitTrackingState::InSync, Some(0), Some(0)) => ("=".to_owned(), Tone::Green),
        (GitTrackingState::Ahead, Some(ahead), Some(0)) if ahead > 0 => {
            (format!("↑{ahead}"), Tone::Green)
        }
        (GitTrackingState::Behind, Some(0), Some(behind)) if behind > 0 => {
            (format!("↓{behind}"), Tone::YellowBold)
        }
        (GitTrackingState::Diverged, Some(ahead), Some(behind)) if ahead > 0 && behind > 0 => {
            (format!("↑{ahead} ↓{behind}"), Tone::YellowBold)
        }
        (GitTrackingState::NoUpstream, _, _) => ("no upstream".to_owned(), Tone::Dim),
        (GitTrackingState::UpstreamMissing, _, _) => {
            ("upstream missing".to_owned(), Tone::YellowBold)
        }
        (GitTrackingState::Detached, _, _) => return StyledText::default(),
        _ => ("?".to_owned(), Tone::Dim),
    };
    let mut result = StyledText::new(label, tone);
    if detailed && let Some(upstream) = &tracking.upstream {
        result.push(" · ", Tone::Dim);
        result.push(target_name(upstream), Tone::Dim);
    }
    result
}

fn target_name(reference: &str) -> &str {
    reference
        .strip_prefix("refs/remotes/")
        .or_else(|| reference.strip_prefix("refs/heads/"))
        .unwrap_or(reference)
}

pub(super) fn append_badge(
    text: &mut StyledText,
    tracking: Option<&GitTracking>,
    branch: Option<&str>,
    detailed: bool,
) {
    let exceptional = tracking.is_some_and(|tracking| {
        tracking.upstream.as_deref().is_some_and(|upstream| {
            branch.is_none_or(|branch| upstream != format!("refs/remotes/origin/{branch}"))
        })
    });
    let badge = badge(tracking, detailed || exceptional);
    if !badge.is_empty() {
        if !text.is_empty() {
            text.push(" · ", Tone::Dim);
        }
        text.append(&badge);
    }
}

pub(super) fn branch_text(
    branch: &str,
    tracking: Option<&GitTracking>,
    detailed: bool,
) -> StyledText {
    let mut text = StyledText::new(branch, Tone::Primary);
    append_badge(&mut text, tracking, Some(branch), detailed);
    text
}

#[cfg(test)]
mod tests;
