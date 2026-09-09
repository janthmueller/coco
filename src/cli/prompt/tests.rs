use super::*;

#[test]
#[ignore = "manual terminal regression probe; requires an interactive PTY"]
fn interactive_terminal_probe() {
    assert!(io::stdin().is_terminal() && io::stderr().is_terminal());
    for index in 0..30 {
        eprintln!("history-{index:02}");
    }
    let choices = (1..=12)
        .map(|index| Choice::new(format!("workspace-{index:02}"), Some("Ready".to_owned())))
        .collect::<Vec<_>>();
    let outcome = run_picker("Choose a workspace", &choices);
    assert!(!crossterm::terminal::is_raw_mode_enabled().unwrap());
    eprintln!("picker-result={outcome:?}");
}

#[test]
#[ignore = "manual confirmation regression probe; requires an interactive PTY"]
fn interactive_confirmation_probe() {
    let mut interaction = TerminalInteraction::detect(false);
    eprintln!("confirmation-result={:?}", interaction.confirm("Proceed?"));
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn navigates_with_arrows_and_vim_keys_and_wraps() {
    let mut state = PickerState::new();
    assert_eq!(
        state.handle_key(key(KeyCode::Up), 3),
        PickerAction::Continue
    );
    assert_eq!(state.selected, 2);
    assert_eq!(
        state.handle_key(key(KeyCode::Char('j')), 3),
        PickerAction::Continue
    );
    assert_eq!(state.selected, 0);
    assert_eq!(
        state.handle_key(key(KeyCode::Char('k')), 3),
        PickerAction::Continue
    );
    assert_eq!(state.selected, 2);
}

#[test]
fn number_keys_select_the_first_nine_choices_immediately() {
    let mut state = PickerState::new();
    assert_eq!(
        state.handle_key(key(KeyCode::Char('3')), 12),
        PickerAction::Selected(2)
    );
    assert_eq!(
        state.handle_key(key(KeyCode::Char('9')), 4),
        PickerAction::Continue
    );
}

#[test]
fn enter_confirms_the_cursor_and_escape_or_q_cancel() {
    let mut state = PickerState {
        selected: 2,
        ..PickerState::new()
    };
    assert_eq!(
        state.handle_key(key(KeyCode::Enter), 4),
        PickerAction::Selected(2)
    );
    assert_eq!(
        state.handle_key(key(KeyCode::Esc), 4),
        PickerAction::Cancelled
    );
    assert_eq!(
        state.handle_key(key(KeyCode::Char('q')), 4),
        PickerAction::Cancelled
    );
}

#[test]
fn long_picker_windows_follow_the_cursor() {
    assert_eq!(visible_start(0, 20, 9), 0);
    assert_eq!(visible_start(8, 20, 9), 4);
    assert_eq!(visible_start(19, 20, 9), 11);
}

#[test]
fn labels_are_kept_on_one_bounded_terminal_line() {
    assert_eq!(single_line("repo\npath\tname"), "repo path name");
    assert_eq!(truncate_line("abcdefgh", 5), "abcd…");
    assert_eq!(truncate_line("abcdefgh", 0), "");
}

#[test]
fn picker_keeps_only_the_title_and_choices() {
    let choices = vec![
        Choice::new("feat/login", Some("Ready".to_owned())),
        Choice::new("fix/oauth", Some("Working".to_owned())),
    ];
    let lines = picker_lines(
        "Choose a workspace",
        &choices,
        &PickerState::new(),
        80,
        Palette::plain(),
    );

    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0], "Choose a workspace (1/2)");
    assert!(lines[1].starts_with("›  1  feat/login"));
    assert!(lines[2].starts_with("   2  fix/oauth"));
    assert!(!lines.join("\n").contains("Enter selects"));
}

#[test]
fn only_the_selected_row_has_a_marker_and_labels_stay_aligned() {
    let choices = vec![Choice::new("workspace", Some("Ready".to_owned())); 3];
    for selected in 0..choices.len() {
        let lines = picker_lines(
            "Choose a workspace",
            &choices,
            &PickerState {
                selected,
                ..PickerState::new()
            },
            80,
            Palette::plain(),
        );
        let rows = &lines[1..];
        assert_eq!(rows.iter().filter(|row| row.starts_with('›')).count(), 1);
        for (index, row) in rows.iter().enumerate() {
            let marker = if index == selected { '›' } else { ' ' };
            assert_eq!(row, &format!("{marker} {:>2}  workspace  Ready", index + 1));
            let label_start = row.find("workspace").unwrap();
            assert_eq!(row[..label_start].width(), 6);
        }
    }
}

#[test]
fn a_single_choice_still_has_an_explicit_picker_view() {
    let choices = vec![Choice::new("test/1", Some("Prepared".to_owned()))];
    let lines = picker_lines(
        "Choose a workspace to open",
        &choices,
        &PickerState::new(),
        80,
        Palette::plain(),
    );

    assert_eq!(
        lines,
        ["Choose a workspace to open (1/1)", "›  1  test/1  Prepared"]
    );
}

#[test]
fn picker_colors_only_when_the_palette_allows_it() {
    let choice = Choice::new("fix/oauth", Some("Working".to_owned()));
    let plain = picker_row(0, &choice, true, 80, Palette::plain());
    let colored = picker_row(0, &choice, true, 80, Palette::colored());

    assert_eq!(plain, "›  1  fix/oauth  Working");
    assert_eq!(colored, Palette::colored().paint(Tone::CyanBold, &plain));
}

#[test]
fn short_terminals_keep_the_selected_row_visible_and_page_by_visible_rows() {
    let layout = PickerLayout::new(40, 5).unwrap();
    let choices = (1..=20)
        .map(|index| Choice::new(format!("workspace-{index}"), None))
        .collect::<Vec<_>>();
    let mut state = PickerState {
        selected: 8,
        visible_rows: layout.visible_rows,
    };
    let lines = picker_lines("Select", &choices, &state, layout.width, Palette::plain());
    assert_eq!(state.visible_rows, 3);
    assert_eq!(lines.len(), 4);
    assert!(lines.iter().any(|line| line == "›  9  workspace-9"));
    state.handle_key(key(KeyCode::PageDown), choices.len());
    assert_eq!(state.selected, 11);
    state.handle_key(key(KeyCode::PageUp), choices.len());
    assert_eq!(state.selected, 8);
    assert!(PickerLayout::new(80, 2).is_err());
    assert!(PickerLayout::new(5, 24).is_err());
}

#[test]
fn wide_labels_titles_and_narrow_prefixes_never_wrap() {
    let choices = vec![Choice::new(
        "修复/登录界面😀",
        Some("测试进行中".to_owned()),
    )];
    for width in 0..40 {
        let lines = picker_lines(
            "Choose\n中文\x1b[2J",
            &choices,
            &PickerState::new(),
            width,
            Palette::plain(),
        );
        assert!(lines.iter().all(|line| line.width() <= width));
        assert!(lines.iter().all(|line| !line.chars().any(char::is_control)));
    }
    assert_eq!(truncate_line("中文abc", 4), "中…");
    assert_eq!(truncate_line("e\u{301}abc", 3), "e\u{301}a…");
}

#[test]
fn confirmations_default_to_no_and_accept_only_explicit_yes_or_no() {
    for (answer, expected) in [
        ("\n", false),
        (" \n", false),
        ("n\n", false),
        ("NO\n", false),
        ("y\n", true),
        (" YES \n", true),
    ] {
        let mut output = Vec::new();
        let actual = run_confirmation(
            &mut io::Cursor::new(answer),
            &mut output,
            "Proceed?",
            Palette::plain(),
        )
        .unwrap();
        assert_eq!(actual, expected);
        assert_eq!(String::from_utf8(output).unwrap(), "Proceed? [y/N] ");
    }
}

#[test]
fn invalid_confirmation_reprompts_and_eof_does_not_authorize_anything() {
    let mut output = Vec::new();
    assert!(
        run_confirmation(
            &mut io::Cursor::new("1\nyesterday\nyes\n"),
            &mut output,
            "Proceed?",
            Palette::plain(),
        )
        .unwrap()
    );
    let output = String::from_utf8(output).unwrap();
    assert_eq!(output.matches("Please enter y or n.").count(), 2);
    assert_eq!(output.matches("[y/N]").count(), 3);
    assert!(!output.contains('\x1b'));
    assert!(
        run_confirmation(
            &mut io::Cursor::new(""),
            &mut Vec::new(),
            "Proceed?",
            Palette::plain(),
        )
        .is_err()
    );
    assert!(
        TerminalInteraction { interactive: false }
            .confirm("Proceed?")
            .is_err()
    );
}
