use super::*;
use crate::cli::style::Palette;

fn tracking(state: GitTrackingState, ahead: Option<u64>, behind: Option<u64>) -> GitTracking {
    GitTracking {
        state,
        upstream: Some("refs/remotes/origin/main".into()),
        remote: Some("origin".into()),
        ahead_by: ahead,
        behind_by: behind,
        observed_at_ms: Some(42),
    }
}

#[test]
fn compact_badges_distinguish_each_relationship_and_unknown_counts() {
    use GitTrackingState::*;
    for (state, ahead, behind, label) in [
        (InSync, Some(0), Some(0), "="),
        (Ahead, Some(2), Some(0), "↑2"),
        (Behind, Some(0), Some(3), "↓3"),
        (Diverged, Some(2), Some(3), "↑2 ↓3"),
        (NoUpstream, None, None, "no upstream"),
        (UpstreamMissing, None, None, "upstream missing"),
        (Unknown, None, None, "?"),
        (Detached, None, None, ""),
        (Ahead, None, None, "?"),
        (InSync, None, None, "?"),
    ] {
        assert_eq!(
            badge(Some(&tracking(state, ahead, behind)), false).as_str(),
            label
        );
    }
    assert!(badge(None, true).is_empty());
}

#[test]
fn exceptional_and_detailed_targets_are_named_without_guessing() {
    let mut value = tracking(GitTrackingState::InSync, Some(0), Some(0));
    assert_eq!(
        branch_text("main", Some(&value), false).as_str(),
        "main · ="
    );
    assert_eq!(
        branch_text("main", Some(&value), true).as_str(),
        "main · = · origin/main"
    );
    value.upstream = Some("refs/remotes/upstream/release".into());
    assert_eq!(
        branch_text("main", Some(&value), false).as_str(),
        "main · = · upstream/release"
    );
    value.upstream = Some("refs/heads/local-base".into());
    assert_eq!(
        branch_text("main", Some(&value), false).as_str(),
        "main · = · local-base"
    );
}

#[test]
fn semantic_colors_do_not_change_text_or_break_unicode_width() {
    use unicode_width::UnicodeWidthStr;
    let value = tracking(GitTrackingState::Diverged, Some(2), Some(3));
    let label = branch_text("特別/main", Some(&value), true);
    assert!(!label.as_str().contains('\x1b'));
    assert!(label.render(80, Palette::colored()).contains('\x1b'));
    assert_eq!(label.render(80, Palette::plain()), label.as_str());
    for width in 0..35 {
        let rendered = label.truncated(width);
        assert!(rendered.width() <= width);
        assert!(!rendered.render(width, Palette::plain()).contains('\x1b'));
    }
}

#[test]
fn control_characters_never_become_terminal_sequences() {
    let text = StyledText::new("branch\x1b[31m\nname", Tone::Primary);
    assert!(!text.render(80, Palette::plain()).contains(['\x1b', '\n']));
}
