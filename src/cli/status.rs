use std::collections::HashMap;
use std::env;
use std::io::{self, IsTerminal, Write};
use std::time::Duration;

use anyhow::Result;
use crossterm::cursor::{Hide, Show};
use crossterm::queue;
use crossterm::terminal::{DisableLineWrap, EnableLineWrap, size};

use crate::protocol::{
    RepositoryScope, WorkspaceGetParams, WorkspaceListItem, WorkspaceListParams,
};
use crate::rpc::RpcClient;

use super::output::{
    render_follow_status, render_status_for_stdout, render_workspace_list_for_stdout,
    render_workspace_update,
};
use super::prompt::terminal::InlineFrame;

const FOLLOW_POLL_INTERVAL: Duration = Duration::from_millis(500);
const COLLECTION_POLL_INTERVAL: Duration = Duration::from_secs(1);
const SPINNER_FRAME_INTERVAL: Duration = Duration::from_millis(125);
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub(super) async fn follow_status(
    client: &RpcClient,
    scope: RepositoryScope,
    workspace: &str,
    include_resources: bool,
) -> Result<()> {
    let interactive = stdout_supports_live_updates();
    let mut output = FollowOutput::new(io::stdout(), interactive)?;
    let mut previous_log_frame: Option<String> = None;
    let mut spinner_index = 0_usize;
    loop {
        let response = client
            .request(WorkspaceGetParams {
                scope: scope.clone(),
                workspace: workspace.to_owned(),
                include_resources,
            })
            .await?;
        if interactive {
            for _ in 0..spinner_frames_per_poll() {
                let frame = render_follow_status(
                    &response,
                    SPINNER[spinner_index % SPINNER.len()],
                    include_resources,
                );
                output.write_frame(&frame)?;
                spinner_index += 1;
                if wait_or_interrupt(SPINNER_FRAME_INTERVAL).await {
                    return Ok(());
                }
            }
        } else {
            let frame = render_status_for_stdout(&response, include_resources);
            if previous_log_frame.as_deref() != Some(&frame) {
                output.write_frame(&frame)?;
                previous_log_frame = Some(frame);
            }
            if wait_or_interrupt(FOLLOW_POLL_INTERVAL).await {
                return Ok(());
            }
        }
    }
}

pub(super) async fn follow_status_collection(
    client: &RpcClient,
    scope: RepositoryScope,
    include_resources: bool,
) -> Result<()> {
    let include_repository = matches!(scope, RepositoryScope::AllRepositories);
    let interactive = stdout_supports_live_updates();
    let mut output = FollowOutput::new(io::stdout(), interactive)?;
    let mut previous_phases: Option<HashMap<String, String>> = None;
    let mut previous_terminal_frame: Option<String> = None;
    loop {
        let workspaces = client
            .request(WorkspaceListParams {
                scope: scope.clone(),
                phases: None,
                include_resources,
            })
            .await?;
        if interactive {
            let frame = render_workspace_list_for_stdout(
                &workspaces,
                include_repository,
                include_resources,
            );
            if previous_terminal_frame.as_deref() != Some(&frame) {
                output.write_frame(&frame)?;
                previous_terminal_frame = Some(frame);
            }
        } else if let Some(previous) = &previous_phases {
            let changes = render_collection_changes(previous, &workspaces, include_repository);
            if !changes.is_empty() {
                output.write_frame(&changes)?;
            }
        } else {
            output.write_frame(&render_workspace_list_for_stdout(
                &workspaces,
                include_repository,
                include_resources,
            ))?;
        }
        previous_phases = Some(
            workspaces
                .iter()
                .map(|item| {
                    (
                        item.workspace.id.clone(),
                        item.workspace.phase.as_str().to_owned(),
                    )
                })
                .collect(),
        );
        if wait_or_interrupt(COLLECTION_POLL_INTERVAL).await {
            return Ok(());
        }
    }
}

fn render_collection_changes(
    previous: &HashMap<String, String>,
    workspaces: &[WorkspaceListItem],
    include_repository: bool,
) -> String {
    let mut output = String::new();
    for item in workspaces {
        let phase = item.workspace.phase.as_str();
        if previous.get(&item.workspace.id).map(String::as_str) == Some(phase) {
            continue;
        }
        output.push_str(&render_workspace_update(item, include_repository));
    }
    output
}

const fn spinner_frames_per_poll() -> usize {
    (FOLLOW_POLL_INTERVAL.as_millis() / SPINNER_FRAME_INTERVAL.as_millis()) as usize
}

fn stdout_supports_live_updates() -> bool {
    live_updates_supported(
        io::stdout().is_terminal(),
        env::var_os("TERM").is_some_and(|term| term == "dumb"),
    )
}

const fn live_updates_supported(is_terminal: bool, dumb_terminal: bool) -> bool {
    is_terminal && !dumb_terminal
}

async fn wait_or_interrupt(duration: Duration) -> bool {
    tokio::select! {
        _ = tokio::signal::ctrl_c() => true,
        _ = tokio::time::sleep(duration) => false,
    }
}

struct FollowOutput<W: Write> {
    output: W,
    interactive: bool,
    frame: InlineFrame,
    terminal_active: bool,
}

impl<W: Write> FollowOutput<W> {
    fn new(output: W, interactive: bool) -> io::Result<Self> {
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

    fn write_frame(&mut self, frame: &str) -> io::Result<()> {
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
