use std::io::{self, Write};

use crossterm::cursor::{Hide, MoveToColumn, MoveUp, Show};
use crossterm::queue;
use crossterm::terminal::{
    Clear, ClearType, DisableLineWrap, EnableLineWrap, disable_raw_mode, enable_raw_mode, size,
};

/// Own the temporary inline display, restoring it even on an input/output error.
pub(super) struct PickerTerminal<W: Write> {
    output: W,
    frame: InlineFrame,
    rows: u16,
    active: bool,
}

impl<W: Write> PickerTerminal<W> {
    pub(super) fn enter(output: W) -> io::Result<Self> {
        enable_raw_mode()?;
        let mut terminal = Self {
            output,
            frame: InlineFrame::default(),
            rows: 24,
            active: true,
        };
        queue!(terminal.output, Hide, DisableLineWrap)?;
        terminal.output.flush()?;
        Ok(terminal)
    }

    pub(super) fn draw(&mut self, lines: &[String], rows: u16) -> io::Result<()> {
        self.rows = rows;
        self.frame.draw(&mut self.output, lines, rows)
    }

    pub(super) fn finish(mut self) -> io::Result<()> {
        self.restore()
    }

    fn restore(&mut self) -> io::Result<()> {
        let rows = size().map_or(self.rows, |(_, rows)| rows);
        let cleared = self.frame.clear(&mut self.output, rows);
        // Do not let a failed cleanup skip restoring cursor visibility or input.
        let shown = queue!(self.output, EnableLineWrap, Show).and_then(|()| self.output.flush());
        let raw_mode = disable_raw_mode();
        self.active = raw_mode.is_err();
        cleared.and(shown).and(raw_mode)
    }
}

impl<W: Write> Drop for PickerTerminal<W> {
    fn drop(&mut self) {
        if self.active {
            let _ = self.restore();
        }
    }
}

#[derive(Default)]
pub(in crate::cli) struct InlineFrame {
    rendered_lines: usize,
}

impl InlineFrame {
    pub(in crate::cli) fn draw(
        &mut self,
        output: &mut impl Write,
        lines: &[String],
        rows: u16,
    ) -> io::Result<()> {
        let mut frame = Vec::new();
        self.rewind(&mut frame, rows)?;
        for line in lines {
            queue!(frame, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
            // A cursor-next-line command does not scroll at the bottom margin.
            // CRLF allocates real rows, including the blank row below the frame.
            write!(frame, "{line}\r\n")?;
        }
        self.rendered_lines = lines.len();
        output.write_all(&frame)?;
        output.flush()
    }

    fn clear(&mut self, output: &mut impl Write, rows: u16) -> io::Result<()> {
        let mut frame = Vec::new();
        self.rewind(&mut frame, rows)?;
        output.write_all(&frame)?;
        output.flush()?;
        self.rendered_lines = 0;
        Ok(())
    }

    fn rewind(&self, output: &mut impl Write, rows: u16) -> io::Result<()> {
        if self.rendered_lines == 0 {
            return Ok(());
        }
        let lines = self.rendered_lines.min(usize::from(rows.saturating_sub(1))) as u16;
        if lines > 0 {
            queue!(output, MoveUp(lines))?;
        }
        queue!(output, MoveToColumn(0), Clear(ClearType::FromCursorDown))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_draw_allocates_real_lines_instead_of_bottom_clamped_cursor_moves() {
        let mut frame = InlineFrame::default();
        let mut output = Vec::new();
        frame
            .draw(
                &mut output,
                &["title".into(), "first".into(), "last".into()],
                8,
            )
            .unwrap();
        let output = String::from_utf8(output).unwrap();
        assert_eq!(output.matches("\r\n").count(), 3);
        assert!(!output.contains("\x1b[1E"));
        assert!(output.ends_with("last\r\n"));
    }

    #[test]
    fn redraw_and_cleanup_erase_the_previous_frame_without_adding_lines() {
        let mut frame = InlineFrame::default();
        let lines = ["first".into(), "second".into(), "third".into()];
        frame.draw(&mut Vec::new(), &lines, 8).unwrap();
        let mut output = Vec::new();
        frame.draw(&mut output, &["replacement".into()], 8).unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(output.starts_with("\x1b[3A\x1b[1G\x1b[J"));
        assert_eq!(output.matches("\r\n").count(), 1);
        let mut output = Vec::new();
        frame.clear(&mut output, 8).unwrap();
        assert_eq!(String::from_utf8(output).unwrap(), "\x1b[1A\x1b[1G\x1b[J");
        assert_eq!(frame.rendered_lines, 0);
    }

    #[test]
    fn shrinking_terminal_never_rewinds_beyond_the_visible_screen() {
        let mut frame = InlineFrame { rendered_lines: 10 };
        let mut output = Vec::new();
        frame
            .draw(&mut output, &["title".into(), "selected".into()], 4)
            .unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(output.starts_with("\x1b[3A"));
        assert!(!output.contains("\x1b[10A"));
    }
}
