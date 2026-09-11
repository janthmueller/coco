use std::env;
use std::io::{self, IsTerminal, Write};
use std::time::Duration;

use crossterm::cursor::{Hide, Show};
use crossterm::queue;
use crossterm::terminal::{DisableLineWrap, EnableLineWrap, size};

use super::prompt::terminal::InlineFrame;

pub(super) fn stdout_supports_live_updates() -> bool {
    live_updates_supported(
        io::stdout().is_terminal(),
        env::var_os("TERM").is_some_and(|term| term == "dumb"),
    )
}

const fn live_updates_supported(is_terminal: bool, dumb_terminal: bool) -> bool {
    is_terminal && !dumb_terminal
}

pub(super) async fn wait_or_interrupt(duration: Duration) -> bool {
    tokio::select! {
        _ = tokio::signal::ctrl_c() => true,
        _ = tokio::time::sleep(duration) => false,
    }
}

pub(super) struct FollowOutput<W: Write> {
    output: W,
    interactive: bool,
    frame: InlineFrame,
    terminal_active: bool,
}

impl<W: Write> FollowOutput<W> {
    pub(super) fn new(output: W, interactive: bool) -> io::Result<Self> {
        let mut renderer = Self {
            output,
            interactive,
            frame: InlineFrame::default(),
            terminal_active: interactive,
        };
        if interactive {
            queue!(renderer.output, Hide, DisableLineWrap)?;
            renderer.output.flush()?;
        }
        Ok(renderer)
    }

    pub(super) fn write_frame(&mut self, frame: &str) -> io::Result<()> {
        if self.interactive {
            let lines = frame.lines().map(str::to_owned).collect::<Vec<_>>();
            let rows = size().map_or(24, |(_, rows)| rows);
            self.frame.draw(&mut self.output, &lines, rows)
        } else {
            self.output.write_all(frame.as_bytes())?;
            if !frame.ends_with('\n') {
                self.output.write_all(b"\n")?;
            }
            self.output.flush()
        }
    }
}

impl<W: Write> Drop for FollowOutput<W> {
    fn drop(&mut self) {
        if self.terminal_active {
            let _ = queue!(self.output, EnableLineWrap, Show).and_then(|()| self.output.flush());
            self.terminal_active = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_frames_replace_the_previous_region() {
        let mut output = Vec::new();
        {
            let mut renderer = FollowOutput::new(&mut output, true).unwrap();
            renderer
                .write_frame("WORKSPACE  STATE\ntest/1     Working\n")
                .unwrap();
            renderer
                .write_frame("WORKSPACE  STATE\ntest/1     Ready\n")
                .unwrap();
        }

        let output = String::from_utf8(output).unwrap();
        assert!(output.starts_with("\u{1b}[?25l\u{1b}[?7l"));
        assert!(output.contains("\r\n\u{1b}[2A\u{1b}[1G\u{1b}[J"));
        assert!(output.contains("WORKSPACE  STATE"));
        assert!(output.contains("test/1     Ready\r\n"));
        assert!(output.ends_with("\u{1b}[?7h\u{1b}[?25h"));
    }

    #[test]
    fn redirected_frames_remain_an_append_only_plain_log() {
        let mut output = Vec::new();
        {
            let mut renderer = FollowOutput::new(&mut output, false).unwrap();
            renderer.write_frame("test/1  Working\n").unwrap();
            renderer.write_frame("test/1  Ready\n").unwrap();
        }

        assert_eq!(
            String::from_utf8(output).unwrap(),
            "test/1  Working\ntest/1  Ready\n"
        );
    }

    #[test]
    fn live_updates_require_a_capable_terminal() {
        assert!(live_updates_supported(true, false));
        assert!(!live_updates_supported(false, false));
        assert!(!live_updates_supported(true, true));
    }

    #[test]
    #[ignore = "manual follow regression probe; requires an interactive PTY"]
    fn interactive_follow_terminal_probe() {
        for index in 0..30 {
            println!("follow-history-{index:02}");
        }
        let mut renderer = FollowOutput::new(io::stdout(), true).unwrap();
        renderer
            .write_frame("WORKSPACE  STATE\nprobe/test Working\n")
            .unwrap();
        renderer
            .write_frame("WORKSPACE  STATE\nprobe/test Ready\n")
            .unwrap();
        renderer
            .write_frame("WORKSPACE  STATE\nprobe/test Waiting\n")
            .unwrap();
        drop(renderer);
        println!("follow-result=ok");
    }
}
