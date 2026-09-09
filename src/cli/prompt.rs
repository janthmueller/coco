use std::io::{self, BufRead, IsTerminal, Write};

use anyhow::{Context, Result, bail};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal::size;
use unicode_width::UnicodeWidthStr;

use super::style::{Palette, Tone};
use terminal::PickerTerminal;

mod terminal;

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
    fn confirm(&mut self, title: &str) -> Result<bool>;
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
            _ => run_picker(title, choices),
        }
    }

    fn text(&mut self, label: &str) -> Result<String> {
        if !self.interactive {
            bail!("interactive input is unavailable");
        }
        let palette = Palette::stderr();
        loop {
            eprint!("{} ", palette.paint(Tone::Bold, format!("{label}:")));
            io::stderr().flush()?;
            let mut value = String::new();
            if io::stdin().read_line(&mut value)? == 0 {
                bail!("input ended before a value was provided");
            }
            let value = value.trim();
            if !value.is_empty() {
                return Ok(value.to_owned());
            }
            eprintln!("{}", palette.paint(Tone::Red, "A value is required."));
        }
    }

    fn confirm(&mut self, title: &str) -> Result<bool> {
        if !self.interactive {
            bail!("interactive confirmation is unavailable");
        }
        run_confirmation(
            &mut io::stdin().lock(),
            &mut io::stderr().lock(),
            title,
            Palette::stderr(),
        )
    }
}

fn run_picker(title: &str, choices: &[Choice]) -> Result<usize> {
    let mut terminal = PickerTerminal::enter(io::stderr().lock())
        .context("could not enter terminal selection mode")?;
    let mut state = PickerState::new();
    let palette = Palette::stderr();
    let outcome = loop {
        let (columns, rows) = size().unwrap_or((100, 24));
        let layout = PickerLayout::new(columns, rows)?;
        state.visible_rows = layout.visible_rows;
        let lines = picker_lines(title, choices, &state, layout.width, palette);
        terminal.draw(&lines, rows)?;
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
    terminal
        .finish()
        .context("could not restore the terminal")?;
    match outcome {
        PickerAction::Selected(index) => Ok(index),
        PickerAction::Cancelled => bail!("selection cancelled"),
        PickerAction::Interrupted => bail!("interrupted"),
        PickerAction::Continue => unreachable!("the picker loop only exits with a terminal action"),
    }
}

struct PickerLayout {
    width: usize,
    visible_rows: usize,
}

impl PickerLayout {
    fn new(columns: u16, rows: u16) -> Result<Self> {
        if columns < 12 || rows < 3 {
            bail!("terminal is too small for selection; enlarge it and retry");
        }
        Ok(Self {
            width: usize::from(columns - 1),
            // Reserve one row for the title and one for the cursor below it.
            visible_rows: usize::from(rows - 2).min(MAX_VISIBLE_ROWS),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PickerAction {
    Continue,
    Selected(usize),
    Cancelled,
    Interrupted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PickerState {
    selected: usize,
    visible_rows: usize,
}

impl PickerState {
    const fn new() -> Self {
        Self {
            selected: 0,
            visible_rows: MAX_VISIBLE_ROWS,
        }
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
                self.selected = self.selected.saturating_sub(self.visible_rows);
                PickerAction::Continue
            }
            KeyCode::PageDown => {
                self.selected = (self.selected + self.visible_rows).min(choice_count - 1);
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

fn picker_lines(
    title: &str,
    choices: &[Choice],
    state: &PickerState,
    width: usize,
    palette: Palette,
) -> Vec<String> {
    let start = visible_start(state.selected, choices.len(), state.visible_rows);
    let end = (start + state.visible_rows).min(choices.len());
    let mut lines = Vec::with_capacity(end - start + 1);
    let title = truncate_line(
        &format!(
            "{} ({}/{})",
            single_line(title),
            state.selected + 1,
            choices.len()
        ),
        width,
    );
    lines.push(palette.paint(Tone::Bold, title));
    for (index, choice) in choices.iter().enumerate().take(end).skip(start) {
        lines.push(picker_row(
            index,
            choice,
            index == state.selected,
            width,
            palette,
        ));
    }
    lines
}

fn picker_row(
    index: usize,
    choice: &Choice,
    selected: bool,
    width: usize,
    palette: Palette,
) -> String {
    let marker = if selected { '›' } else { ' ' };
    let prefix = truncate_line(&format!("{marker} {:>2}  ", index + 1), width);
    let available = width.saturating_sub(prefix.width());
    let label = single_line(&choice.label);
    let label = truncate_line(&label, available);
    let remaining = available.saturating_sub(label.width());
    let detail = choice
        .detail
        .as_deref()
        .map(single_line)
        .filter(|detail| !detail.is_empty())
        .filter(|_| remaining > 3)
        .map(|detail| format!("  {}", truncate_line(&detail, remaining - 2)))
        .unwrap_or_default();
    if selected {
        palette.paint(Tone::CyanBold, format!("{prefix}{label}{detail}"))
    } else {
        format!(
            "{}{}{}",
            palette.paint(Tone::Dim, prefix),
            label,
            palette.paint(Tone::Dim, detail),
        )
    }
}

fn visible_start(selected: usize, choice_count: usize, visible_rows: usize) -> usize {
    if choice_count <= visible_rows {
        0
    } else {
        selected
            .saturating_sub(visible_rows / 2)
            .min(choice_count - visible_rows)
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
    if value.width() <= width {
        return value.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    let end = value
        .char_indices()
        .map(|(index, character)| index + character.len_utf8())
        .take_while(|end| value[..*end].width() < width)
        .last()
        .unwrap_or(0);
    format!("{}…", &value[..end])
}

fn run_confirmation(
    input: &mut impl BufRead,
    output: &mut impl Write,
    title: &str,
    palette: Palette,
) -> Result<bool> {
    loop {
        write!(
            output,
            "{} {} ",
            palette.paint(Tone::Bold, single_line(title)),
            palette.paint(Tone::Dim, "[y/N]")
        )?;
        output.flush()?;
        let mut answer = String::new();
        if input.read_line(&mut answer)? == 0 {
            bail!("input ended before confirmation");
        }
        match answer.trim().to_ascii_lowercase().as_str() {
            "y" | "yes" => return Ok(true),
            "" | "n" | "no" => return Ok(false),
            _ => writeln!(
                output,
                "{}",
                palette.paint(Tone::Red, "Please enter y or n.")
            )?,
        }
    }
}

#[cfg(test)]
mod tests;
