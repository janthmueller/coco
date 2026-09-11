use std::collections::HashMap;
use std::io;
use std::time::Duration;

use anyhow::Result;

use crate::protocol::{
    RepositoryScope, WorkspaceGetParams, WorkspaceListItem, WorkspaceListParams,
};
use crate::rpc::RpcClient;

use super::follow::{FollowOutput, stdout_supports_live_updates, wait_or_interrupt};
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
