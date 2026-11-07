use super::*;
use crate::cli::output::safe_line;
use crate::cli::style::{Palette, Tone};
use crate::protocol::{WorkspaceAttachFinishParams, WorkspaceCleanupResult};

pub(super) fn notify_preserved() {
    let palette = Palette::stderr();
    eprintln!(
        "{} Workspace kept: another conversation was used. Automatic cleanup disabled.",
        palette.paint(Tone::YellowBold, "!")
    );
}

pub(super) async fn finish(client: &RpcClient, workspace_id: &str, lease_id: &str, name: &str) {
    let result = client
        .request(WorkspaceAttachFinishParams {
            workspace_id: workspace_id.to_owned(),
            lease_id: lease_id.to_owned(),
        })
        .await;
    let palette = Palette::stderr();
    let name = safe_line(name);
    match result {
        Ok(result) => match result.cleanup {
            None => {}
            Some(WorkspaceCleanupResult::Deleted) => {
                eprintln!("{} Removed {name}", palette.paint(Tone::Green, "✓"));
            }
            Some(WorkspaceCleanupResult::Retained { reason }) => {
                eprintln!(
                    "{} Kept {name}: {}. Automatic cleanup disabled.",
                    palette.paint(Tone::YellowBold, "!"),
                    safe_line(&reason)
                );
            }
            Some(WorkspaceCleanupResult::Incomplete) => {
                eprintln!(
                    "{} Cleanup incomplete for {name}; inspect status and retry delete.",
                    palette.paint(Tone::YellowBold, "!")
                );
            }
        },
        Err(_) => {
            // Do not turn a successful TUI session into a launch error or
            // blindly resend a possibly accepted destructive request.
            let _ = client
                .request(WorkspaceAttachReleaseParams {
                    workspace_id: workspace_id.to_owned(),
                    lease_id: lease_id.to_owned(),
                })
                .await;
            eprintln!(
                "{} Cleanup not confirmed for {name}; check status before deleting.",
                palette.paint(Tone::YellowBold, "!")
            );
        }
    }
}
