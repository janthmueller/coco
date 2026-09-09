use std::collections::HashMap;
use std::env;
use std::io::{self, IsTerminal, Write};
use std::time::Duration;

use anyhow::Result;
use crossterm::cursor::{MoveToColumn, RestorePosition, SavePosition};
use crossterm::queue;
use crossterm::terminal::{Clear, ClearType};

use crate::protocol::{
    RepositoryScope, WorkspaceGetParams, WorkspaceListItem, WorkspaceListParams,
};
use crate::rpc::RpcClient;

use super::output::{
    render_follow_status, render_status_for_stdout, render_workspace_list_for_stdout,
    render_workspace_update,
};

const FOLLOW_POLL_INTERVAL: Duration = Duration::from_millis(500);
const COLLECTION_POLL_INTERVAL: Duration = Duration::from_secs(1);
const SPINNER_FRAME_INTERVAL: Duration = Duration::from_millis(125);
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub(super) async fn follow_status(
    client: &RpcClient,
    scope: RepositoryScope,
    workspace: &str,
) -> Result<()> {
    let interactive = stdout_supports_live_updates();
    let mut output = FollowOutput::new(interactive);
    let mut previous_log_frame: Option<String> = None;
    let mut spinner_index = 0_usize;
    loop {
        let response = client
            .request(WorkspaceGetParams {
                scope: scope.clone(),
                workspace: workspace.to_owned(),
            })
            .await?;
        if interactive {
            for _ in 0..spinner_frames_per_poll() {
                let frame = render_follow_status(&response, SPINNER[spinner_index % SPINNER.len()]);
                output.write_frame(&frame)?;
                spinner_index += 1;
                if wait_or_interrupt(SPINNER_FRAME_INTERVAL).await {
                    return Ok(());
                }
            }
        } else {
            let frame = render_status_for_stdout(&response);
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
) -> Result<()> {
    let include_repository = matches!(scope, RepositoryScope::AllRepositories);
    let interactive = stdout_supports_live_updates();
    let mut output = FollowOutput::new(interactive);
    let mut previous_phases: Option<HashMap<String, String>> = None;
    let mut previous_terminal_frame: Option<String> = None;
    loop {
        let workspaces = client
            .request(WorkspaceListParams {
                scope: scope.clone(),
                phases: None,
            })
            .await?;
        if interactive {
            let frame = render_workspace_list_for_stdout(&workspaces, include_repository);
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

struct FollowOutput {
    interactive: bool,
    has_frame: bool,
}

impl FollowOutput {
    const fn new(interactive: bool) -> Self {
        Self {
            interactive,
            has_frame: false,
        }
    }

    fn write_frame(&mut self, frame: &str) -> io::Result<()> {
        let stdout = io::stdout();
        self.write_frame_to(&mut stdout.lock(), frame)
    }

    fn write_frame_to(&mut self, output: &mut impl Write, frame: &str) -> io::Result<()> {
        if self.interactive {
            if self.has_frame {
                queue!(
                    output,
                    RestorePosition,
                    MoveToColumn(0),
                    Clear(ClearType::FromCursorDown)
                )?;
            } else {
                queue!(output, MoveToColumn(0), SavePosition)?;
            }
        }
        output.write_all(frame.as_bytes())?;
        if !frame.ends_with('\n') {
            output.write_all(b"\n")?;
        }
        output.flush()?;
        if self.interactive {
            self.has_frame = true;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_frames_replace_the_previous_region() {
        let mut renderer = FollowOutput::new(true);
        let mut output = Vec::new();

        renderer
            .write_frame_to(&mut output, "WORKSPACE  STATE\ntest/1     Working\n")
            .unwrap();
        renderer
            .write_frame_to(&mut output, "WORKSPACE  STATE\ntest/1     Ready\n")
            .unwrap();

        let output = String::from_utf8(output).unwrap();
        assert!(output.starts_with("\u{1b}[1G\u{1b}7"));
        assert!(output.contains("\u{1b}8\u{1b}[1G"));
        assert!(output.contains("\u{1b}[J"));
        assert!(output.ends_with("WORKSPACE  STATE\ntest/1     Ready\n"));
    }

    #[test]
    fn redirected_frames_remain_an_append_only_plain_log() {
        let mut renderer = FollowOutput::new(false);
        let mut output = Vec::new();

        renderer
            .write_frame_to(&mut output, "test/1  Working\n")
            .unwrap();
        renderer
            .write_frame_to(&mut output, "test/1  Ready\n")
            .unwrap();

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
}
