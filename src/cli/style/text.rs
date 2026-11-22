use std::fmt;
use std::ops::Deref;

use unicode_width::UnicodeWidthStr;

use super::{Palette, Tone};

/// Plain, sanitized text with semantic styles. Layout always precedes ANSI.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(in crate::cli) struct StyledText {
    text: String,
    spans: Vec<(String, Tone)>,
}

impl StyledText {
    pub(in crate::cli) fn new(text: impl AsRef<str>, tone: Tone) -> Self {
        let mut result = Self::default();
        result.push(text, tone);
        result
    }

    pub(in crate::cli) fn push(&mut self, text: impl AsRef<str>, tone: Tone) {
        let text: String = text
            .as_ref()
            .chars()
            .map(|character| {
                if character.is_control() {
                    ' '
                } else {
                    character
                }
            })
            .collect();
        self.text.push_str(&text);
        if let Some((previous, previous_tone)) = self.spans.last_mut()
            && *previous_tone == tone
        {
            previous.push_str(&text);
        } else {
            self.spans.push((text, tone));
        }
    }

    pub(in crate::cli) fn append(&mut self, other: &Self) {
        for (text, tone) in &other.spans {
            self.push(text, *tone);
        }
    }

    pub(in crate::cli) fn as_str(&self) -> &str {
        &self.text
    }

    pub(in crate::cli) fn fragments(&self) -> impl Iterator<Item = (&str, Tone)> {
        self.spans.iter().map(|(text, tone)| (text.as_str(), *tone))
    }

    pub(in crate::cli) fn truncated(&self, width: usize) -> Self {
        if self.text.width() <= width {
            return self.clone();
        }
        if width == 0 {
            return Self::default();
        }
        let end = self
            .text
            .char_indices()
            .map(|(index, character)| index + character.len_utf8())
            .take_while(|end| self.text[..*end].width() < width)
            .last()
            .unwrap_or(0);
        let mut remaining = end;
        let mut result = Self::default();
        for (text, tone) in &self.spans {
            let count = remaining.min(text.len());
            result.push(&text[..count], *tone);
            remaining -= count;
            if remaining == 0 {
                break;
            }
        }
        result.push("…", Tone::Dim);
        result
    }

    pub(in crate::cli) fn render(&self, width: usize, palette: Palette) -> String {
        self.truncated(width)
            .spans
            .iter()
            .map(|(text, tone)| palette.paint(*tone, text))
            .collect()
    }
}

impl From<String> for StyledText {
    fn from(text: String) -> Self {
        Self::new(text, Tone::Primary)
    }
}

impl Deref for StyledText {
    type Target = str;
    fn deref(&self) -> &str {
        &self.text
    }
}

impl fmt::Display for StyledText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.text)
    }
}

impl PartialEq<str> for StyledText {
    fn eq(&self, other: &str) -> bool {
        self.text == other
    }
}

impl PartialEq<&str> for StyledText {
    fn eq(&self, other: &&str) -> bool {
        self.text == *other
    }
}
