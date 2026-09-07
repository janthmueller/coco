use std::io::{self, IsTerminal, Write};

use anyhow::{Context, Result, bail};
use crossterm::cursor::{MoveToColumn, MoveToNextLine, MoveUp};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::queue;
use crossterm::terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode, size};

const MAX_VISIBLE_ROWS: usize = 9;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Choice {
    pub(super) label: String,
    pub(super) detail: Option<String>,
}

impl Choice {
    pub(super) fn new(label: impl Into<String>, detail: Option<String>) -> Self {
        Self {
            label: label.into(),
            detail,
        }
    }
}

pub(super) trait Interaction {
    fn is_interactive(&self) -> bool;
    fn select(&mut self, title: &str, choices: &[Choice]) -> Result<usize>;
    fn text(&mut self, label: &str) -> Result<String>;
}

pub(super) struct TerminalInteraction {
    interactive: bool,
}

impl TerminalInteraction {
    pub(super) fn detect(no_input: bool) -> Self {
        Self {
            interactive: !no_input && io::stdin().is_terminal() && io::stderr().is_terminal(),
        }
    }
}

impl Interaction for TerminalInteraction {
    fn is_interactive(&self) -> bool {
        self.interactive
    }

    fn select(&mut self, title: &str, choices: &[Choice]) -> Result<usize> {
        if !self.interactive {
            bail!("interactive selection is unavailable");
        }
        match choices {
            [] => bail!("{title} has no available choices"),
            [choice] => {
                eprintln!("Using {}.", choice.label);
                Ok(0)
            }
            _ => run_picker(title, choices),
        }
    }

    fn text(&mut self, label: &str) -> Result<String> {
        if !self.interactive {
            bail!("interactive input is unavailable");
        }
        loop {
            eprint!("{label}: ");
            io::stderr().flush()?;
            let mut value = String::new();
            if io::stdin().read_line(&mut value)? == 0 {
                bail!("input ended before a value was provided");
            }
            let value = value.trim();
            if !value.is_empty() {
                return Ok(value.to_owned());
            }
            eprintln!("A value is required.");
        }
    }
}

fn run_picker(title: &str, choices: &[Choice]) -> Result<usize> {
    let raw_mode = RawModeGuard::enter().context("could not enter terminal input mode")?;
    let mut output = io::stderr().lock();
    let mut state = PickerState::new();
    let mut rendered_lines = 0;
    let outcome = loop {
        let lines = picker_lines(title, choices, &state);
        redraw(&mut output, rendered_lines, &lines)?;
        rendered_lines = lines.len();
        match event::read().context("could not read terminal input")? {
            Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
                match state.handle_key(key, choices.len()) {
                    PickerAction::Continue => {}
                    action => break action,
                }
            }
            Event::Resize(_, _) => {}
            _ => {}
        }
    };
    clear_rendered(&mut output, rendered_lines)?;
    raw_mode
        .restore()
        .context("could not restore terminal input mode")?;
    match outcome {
        PickerAction::Selected(index) => {
            writeln!(output, "Selected: {}", choices[index].label)?;
            Ok(index)
        }
        PickerAction::Cancelled => bail!("selection cancelled"),
        PickerAction::Interrupted => bail!("interrupted"),
        PickerAction::Continue => unreachable!("the picker loop only exits with a terminal action"),
    }
}

struct RawModeGuard {
    active: bool,
}

impl RawModeGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        Ok(Self { active: true })
    }

    fn restore(mut self) -> io::Result<()> {
        disable_raw_mode()?;
        self.active = false;
        Ok(())
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = disable_raw_mode();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PickerAction {
    Continue,
    Selected(usize),
    Cancelled,
    Interrupted,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct PickerState {
    selected: usize,
}

impl PickerState {
    const fn new() -> Self {
        Self { selected: 0 }
    }

    fn handle_key(&mut self, key: KeyEvent, choice_count: usize) -> PickerAction {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return PickerAction::Interrupted;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.checked_sub(1).unwrap_or(choice_count - 1);
                PickerAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = (self.selected + 1) % choice_count;
                PickerAction::Continue
            }
            KeyCode::Home => {
                self.selected = 0;
                PickerAction::Continue
            }
            KeyCode::End => {
                self.selected = choice_count - 1;
                PickerAction::Continue
            }
            KeyCode::PageUp => {
                self.selected = self.selected.saturating_sub(MAX_VISIBLE_ROWS);
                PickerAction::Continue
            }
            KeyCode::PageDown => {
                self.selected = (self.selected + MAX_VISIBLE_ROWS).min(choice_count - 1);
                PickerAction::Continue
            }
            KeyCode::Char(digit @ '1'..='9') if key.modifiers.is_empty() => {
                let index = usize::from(digit as u8 - b'1');
                if index < choice_count {
                    PickerAction::Selected(index)
                } else {
                    PickerAction::Continue
                }
            }
            KeyCode::Enter => PickerAction::Selected(self.selected),
            KeyCode::Esc | KeyCode::Char('q') => PickerAction::Cancelled,
            _ => PickerAction::Continue,
        }
    }
}

fn picker_lines(title: &str, choices: &[Choice], state: &PickerState) -> Vec<String> {
    let width = usize::from(size().map_or(100, |(width, _)| width)).saturating_sub(1);
    let start = visible_start(state.selected, choices.len());
    let end = (start + MAX_VISIBLE_ROWS).min(choices.len());
    let mut lines = Vec::with_capacity(end - start + 2);
    lines.push(truncate_line(
        &format!("{title} ({}/{})", state.selected + 1, choices.len()),
        width,
    ));
    for (index, choice) in choices.iter().enumerate().take(end).skip(start) {
        let marker = if index == state.selected { '>' } else { ' ' };
        let detail = choice
            .detail
            .as_deref()
            .map(single_line)
            .filter(|detail| !detail.is_empty())
            .map(|detail| format!("  {detail}"))
            .unwrap_or_default();
        lines.push(truncate_line(
            &format!(
                "{marker} {:>2}  {}{detail}",
                index + 1,
                single_line(&choice.label)
            ),
            width,
        ));
    }
    lines.push(truncate_line(
        "Up/Down or j/k move; Enter selects; 1-9 selects directly; Esc/q cancels",
        width,
    ));
    lines
}

fn visible_start(selected: usize, choice_count: usize) -> usize {
    if choice_count <= MAX_VISIBLE_ROWS {
        0
    } else {
        selected
            .saturating_sub(MAX_VISIBLE_ROWS / 2)
            .min(choice_count - MAX_VISIBLE_ROWS)
    }
}

fn single_line(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() || matches!(character, '\n' | '\r' | '\t') {
                ' '
            } else {
                character
            }
        })
        .collect()
}

fn truncate_line(value: &str, width: usize) -> String {
    let length = value.chars().count();
    if length <= width {
        return value.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    let mut truncated = value
        .chars()
        .take(width.saturating_sub(1))
        .collect::<String>();
    truncated.push('…');
    truncated
}

fn redraw(output: &mut impl Write, previous_lines: usize, lines: &[String]) -> io::Result<()> {
    if previous_lines > 0 {
        queue!(
            output,
            MoveUp(u16::try_from(previous_lines).unwrap_or(u16::MAX))
        )?;
    }
    for line in lines {
        queue!(output, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
        write!(output, "{line}")?;
        queue!(output, MoveToNextLine(1))?;
    }
    output.flush()
}

fn clear_rendered(output: &mut impl Write, lines: usize) -> io::Result<()> {
    if lines == 0 {
        return Ok(());
    }
    queue!(output, MoveUp(u16::try_from(lines).unwrap_or(u16::MAX)))?;
    for _ in 0..lines {
        queue!(
            output,
            MoveToColumn(0),
            Clear(ClearType::CurrentLine),
            MoveToNextLine(1)
        )?;
    }
    queue!(
        output,
        MoveUp(u16::try_from(lines).unwrap_or(u16::MAX)),
        MoveToColumn(0)
    )?;
    output.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let mut state = PickerState { selected: 2 };
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
        assert_eq!(visible_start(0, 20), 0);
        assert_eq!(visible_start(8, 20), 4);
        assert_eq!(visible_start(19, 20), 11);
    }

    #[test]
    fn labels_are_kept_on_one_bounded_terminal_line() {
        assert_eq!(single_line("repo\npath\tname"), "repo path name");
        assert_eq!(truncate_line("abcdefgh", 5), "abcd…");
        assert_eq!(truncate_line("abcdefgh", 0), "");
    }
}
