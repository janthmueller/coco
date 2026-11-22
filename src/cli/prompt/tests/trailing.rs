use super::*;

#[test]
fn relative_times_align_at_the_right_edge_without_highlighting_the_suffix() {
    let choices = [
        Choice::new("fix/login", Some("Ready".into())).with_trailing_detail(Some("5m ago".into())),
        Choice::new("feat/search", Some("Working · main".into()))
            .with_trailing_detail(Some("2h ago".into())),
    ];
    for (index, choice) in choices.iter().enumerate() {
        let plain = picker_row(index, choice, index == 0, 70, Palette::plain());
        assert_eq!(plain.width(), 70);
        assert!(plain.ends_with(choice.trailing_detail.as_ref().unwrap()));
        let colored = picker_row(index, choice, index == 0, 70, Palette::colored());
        assert!(colored.contains(&Palette::colored().paint(
            Tone::Dim,
            "  · ".to_owned() + choice.detail.as_ref().unwrap()
        )));
        assert!(!colored.contains("\x1b[1m5m ago"));
    }
}

#[test]
fn the_optional_time_disappears_before_existing_label_detail_or_default_is_shortened() {
    let original = Choice::new("workspace/important", Some("Ready · other-branch".into()));
    let timed = original.clone().with_trailing_detail(Some("5m ago".into()));
    for width in 0..60 {
        let plain = picker_row(0, &timed, true, width, Palette::plain());
        assert!(plain.width() <= width);
        if !plain.contains("5m ago") {
            assert_eq!(
                plain,
                picker_row(0, &original, true, width, Palette::plain())
            );
        } else {
            assert!(plain.contains("workspace/important"));
            assert!(plain.contains("Ready · other-branch"));
        }
    }
    let default = original.with_default_marker();
    let timed = default.clone().with_trailing_detail(Some("5m ago".into()));
    assert_eq!(
        picker_row(0, &timed, true, 40, Palette::plain()),
        picker_row(0, &default, true, 40, Palette::plain())
    );
}

#[test]
fn unicode_and_control_characters_never_overflow_a_timed_row() {
    let choice = Choice::new("界面/修正\n\x1b[31m", Some("Ready\t· 工作".into()))
        .with_trailing_detail(Some("5m ago\n\x1b[2J".into()));
    for width in 0..120 {
        let row = picker_row(0, &choice, true, width, Palette::plain());
        assert!(row.width() <= width, "row overflow at {width}: {row:?}");
        assert!(!row.contains(['\n', '\r', '\x1b']));
    }
}

#[test]
fn empty_or_missing_trailing_metadata_keeps_existing_picker_output_exact() {
    let original = Choice::new("work", Some("Ready".into()));
    for trailing in [None, Some(String::new()), Some("\n\t".into())] {
        let choice = original.clone().with_trailing_detail(trailing);
        assert_eq!(
            picker_row(1, &choice, false, 70, Palette::plain()),
            picker_row(1, &original, false, 70, Palette::plain())
        );
    }
}

#[test]
fn styled_git_details_preserve_colors_defaults_and_recency_without_width_overflow() {
    let mut detail = StyledText::new("Ready · ", Tone::Dim);
    detail.push("↑2 ↓3", Tone::YellowBold);
    let choice = Choice::new("feat/login", None)
        .with_styled_detail(detail)
        .with_default_marker()
        .with_trailing_detail(Some("5m ago".into()));
    for width in 0..100 {
        let row = picker_row(0, &choice, true, width, Palette::plain());
        assert!(row.width() <= width);
        assert!(!row.contains('\x1b'));
    }
    let row = picker_row(0, &choice, true, 90, Palette::colored());
    assert!(row.contains(&Palette::colored().paint(Tone::YellowBold, "↑2 ↓3")));
    let plain = picker_row(0, &choice, true, 90, Palette::plain());
    assert!(plain.contains("Ready · ↑2 ↓3 · default"));
    assert!(plain.ends_with("5m ago"));
}

#[test]
#[ignore = "manual Git badge picker probe; requires an interactive PTY, no daemon or model"]
fn interactive_tracking_probe() {
    let tracking = crate::protocol::GitTracking {
        state: crate::protocol::GitTrackingState::Diverged,
        upstream: Some("refs/remotes/origin/feat/login".into()),
        remote: Some("origin".into()),
        ahead_by: Some(2),
        behind_by: Some(1),
        observed_at_ms: Some(42),
    };
    let mut detail = StyledText::new("Working", Tone::Dim);
    crate::cli::git_tracking::append_badge(&mut detail, Some(&tracking), Some("feat/login"), false);
    let choices = [
        Choice::new("feat/login", None)
            .with_styled_detail(detail)
            .with_trailing_detail(Some("5m ago".into())),
        Choice::new("main", Some("Ready · no upstream".into())),
    ];
    let mut interaction = TerminalInteraction::detect(false);
    let selected = interaction.select("Git tracking", &choices).unwrap();
    println!("tracking-result={selected}");
}
