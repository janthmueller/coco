use std::env;
use std::io::{self, IsTerminal};

use crossterm::style::Stylize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Tone {
    Primary,
    Bold,
    Dim,
    Cyan,
    CyanBold,
    Green,
    GreenBold,
    Red,
    RedBold,
    Magenta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Palette {
    enabled: bool,
}

impl Palette {
    pub(super) fn stdout() -> Self {
        Self::detect(io::stdout().is_terminal())
    }

    pub(super) fn stderr() -> Self {
        Self::detect(io::stderr().is_terminal())
    }

    #[cfg(test)]
    pub(super) const fn plain() -> Self {
        Self { enabled: false }
    }

    pub(super) fn paint(self, tone: Tone, value: impl ToString) -> String {
        let value = value.to_string();
        if !self.enabled || tone == Tone::Primary {
            return value;
        }
        match tone {
            Tone::Primary => value,
            Tone::Bold => value.bold().to_string(),
            Tone::Dim => value.dim().to_string(),
            Tone::Cyan => value.cyan().to_string(),
            Tone::CyanBold => value.cyan().bold().to_string(),
            Tone::Green => value.green().to_string(),
            Tone::GreenBold => value.green().bold().to_string(),
            Tone::Red => value.red().to_string(),
            Tone::RedBold => value.red().bold().to_string(),
            Tone::Magenta => value.magenta().to_string(),
        }
    }

    fn detect(is_terminal: bool) -> Self {
        Self::from_capabilities(
            is_terminal,
            env::var_os("NO_COLOR").is_some(),
            env::var_os("TERM").is_some_and(|term| term == "dumb"),
        )
    }

    const fn from_capabilities(is_terminal: bool, no_color: bool, dumb_terminal: bool) -> Self {
        Self {
            enabled: is_terminal && !no_color && !dumb_terminal,
        }
    }

    #[cfg(test)]
    pub(super) const fn colored() -> Self {
        Self { enabled: true }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_palette_never_emits_terminal_sequences() {
        assert_eq!(
            Palette::plain().paint(Tone::CyanBold, "selected"),
            "selected"
        );
    }

    #[test]
    fn colored_palette_uses_standard_ansi_styles() {
        let rendered = Palette::colored().paint(Tone::GreenBold, "done");
        assert!(rendered.contains("\u{1b}["));
        assert!(rendered.contains("done"));
    }

    #[test]
    fn style_requires_a_capable_terminal_and_honors_no_color() {
        assert!(Palette::from_capabilities(true, false, false).enabled);
        assert!(!Palette::from_capabilities(false, false, false).enabled);
        assert!(!Palette::from_capabilities(true, true, false).enabled);
        assert!(!Palette::from_capabilities(true, false, true).enabled);
    }
}
