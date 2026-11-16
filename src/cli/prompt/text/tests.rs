use super::*;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn text_editing_preserves_unicode_and_normal_navigation_keys() {
    let mut editor = Editor::new("a界é");
    editor.key(key(KeyCode::Left));
    editor.key(key(KeyCode::Backspace));
    editor.insert("🙂");
    assert_eq!(editor.chars.iter().collect::<String>(), "a🙂é");
    editor.key(key(KeyCode::Home));
    editor.key(key(KeyCode::Delete));
    editor.key(key(KeyCode::End));
    editor.key(key(KeyCode::Char('q')));
    assert_eq!(editor.chars.iter().collect::<String>(), "🙂éq");
}

#[test]
fn paste_is_data_and_does_not_submit_or_execute_controls() {
    let mut editor = Editor::new("");
    editor.insert("first\nsecond\r\n\u{1b}[31m\u{3}\tlast");
    let value = editor.chars.iter().collect::<String>();
    assert_eq!(value, "first second  [31m last");
    assert!(value.chars().all(|ch| !ch.is_control()));
}

#[test]
fn prefilled_messages_keep_their_exact_contents_without_rendering_controls() {
    let initial = "  first\nsecond\t\u{1b}[31m  ";
    let editor = Editor::new(initial);
    assert_eq!(editor.chars.iter().collect::<String>(), initial);
    let (displayed, _) = editor.viewport(80);
    assert!(displayed.contains("first↵second⇥�[31m"));
    assert!(displayed.chars().all(|ch| !ch.is_control()));
}

#[test]
fn narrow_viewport_keeps_cursor_and_full_width_characters_inside() {
    let mut editor = Editor::new("abcdef界🙂é");
    for width in 2..9 {
        let (value, cursor) = editor.viewport(width);
        assert!(value.width() <= width);
        assert!(cursor < width);
    }
    editor.key(key(KeyCode::Home));
    let (value, cursor) = editor.viewport(5);
    assert_eq!(value, "abcde");
    assert_eq!(cursor, 0);
}

#[test]
fn back_and_cancel_are_typed_and_shift_tab_at_root_is_not_cancel() {
    assert!(matches!(
        navigation::navigate_key::<()>(key(KeyCode::Esc), true),
        Some(Navigation::Back)
    ));
    assert!(matches!(
        navigation::navigate_key::<()>(key(KeyCode::Esc), false),
        Some(Navigation::Cancel)
    ));
    assert!(navigation::navigate_key::<()>(key(KeyCode::BackTab), false).is_none());
    assert!(matches!(
        navigation::navigate_key::<()>(key(KeyCode::BackTab), true),
        Some(Navigation::Back)
    ));
    let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert!(matches!(
        navigation::navigate_key::<()>(ctrl_c, true),
        Some(Navigation::Cancel)
    ));
}
