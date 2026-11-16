use unicode_width::UnicodeWidthChar;

use super::*;

pub(super) fn run(label: &str, initial: &str, allow_back: bool) -> Result<Navigation<String>> {
    let mut terminal = PickerTerminal::enter(io::stderr().lock())?;
    terminal.enable_paste()?;
    let mut editor = Editor::new(initial);
    let palette = Palette::stderr();
    let mut required = false;
    let outcome = loop {
        let (columns, rows) = size().unwrap_or((100, 24));
        if columns < 8 || rows < 4 {
            bail!("terminal is too small for text input; enlarge it and try again");
        }
        let width = usize::from(columns - 1);
        let (value, cursor) = editor.viewport(width - 2);
        let hint = if required {
            "A value is required."
        } else if allow_back {
            "Enter continue · Esc back"
        } else {
            "Enter continue · Esc cancel"
        };
        terminal.draw(
            &[
                palette.paint(Tone::Bold, truncate_line(label, width)),
                format!("> {value}"),
                palette.paint(Tone::Dim, truncate_line(hint, width)),
            ],
            rows,
        )?;
        terminal.input_cursor((cursor + 2) as u16)?;
        match event::read().context("could not read terminal input")? {
            Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
                if let Some(action) = navigation::navigate_key(key, allow_back) {
                    break action;
                }
                if key.code == KeyCode::Enter {
                    let value: String = editor.chars.iter().collect();
                    if !value.trim().is_empty() {
                        break Navigation::Value(value);
                    }
                    required = true;
                } else {
                    editor.key(key);
                    required = false;
                }
            }
            Event::Paste(value) => editor.insert(&value),
            _ => {}
        }
    };
    terminal
        .finish()
        .context("could not restore the terminal")?;
    Ok(outcome)
}

struct Editor {
    chars: Vec<char>,
    cursor: usize,
}

impl Editor {
    fn new(initial: &str) -> Self {
        let chars: Vec<_> = initial.chars().collect();
        Self {
            cursor: chars.len(),
            chars,
        }
    }

    fn insert(&mut self, value: &str) {
        // Bracketed paste is data, never keys or terminal control sequences.
        let chars: Vec<_> = value
            .chars()
            .filter_map(|ch| match ch {
                '\r' | '\n' | '\t' => Some(' '),
                ch if !ch.is_control() => Some(ch),
                _ => None,
            })
            .collect();
        let count = chars.len();
        self.chars.splice(self.cursor..self.cursor, chars);
        self.cursor += count;
    }

    fn key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('a') => self.cursor = 0,
                KeyCode::Char('e') => self.cursor = self.chars.len(),
                KeyCode::Char('u') => {
                    self.chars.drain(..self.cursor);
                    self.cursor = 0;
                }
                KeyCode::Char('k') => {
                    self.chars.truncate(self.cursor);
                }
                _ => {}
            }
            return;
        }
        match key.code {
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.chars.len()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.chars.len(),
            KeyCode::Backspace if self.cursor > 0 => {
                self.cursor -= 1;
                self.chars.remove(self.cursor);
            }
            KeyCode::Delete if self.cursor < self.chars.len() => {
                self.chars.remove(self.cursor);
            }
            KeyCode::Char(ch) if !ch.is_control() => self.insert(&ch.to_string()),
            _ => {}
        }
    }

    fn viewport(&self, width: usize) -> (String, usize) {
        let displayed: Vec<_> = self
            .chars
            .iter()
            .map(|ch| match ch {
                '\r' | '\n' => '↵',
                '\t' => '⇥',
                ch if ch.is_control() => '�',
                ch => *ch,
            })
            .collect();
        let cell_width = |chars: &[char]| {
            chars
                .iter()
                .map(|ch| ch.width().unwrap_or(0))
                .sum::<usize>()
        };
        let mut start = self.cursor;
        while start > 0 && cell_width(&displayed[start - 1..self.cursor]) < width {
            start -= 1;
        }
        let cursor = cell_width(&displayed[start..self.cursor]);
        let mut end = self.cursor;
        while end < displayed.len() && cell_width(&displayed[start..=end]) <= width {
            end += 1;
        }
        (displayed[start..end].iter().collect(), cursor)
    }
}

#[cfg(test)]
mod tests;
