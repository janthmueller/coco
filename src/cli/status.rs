use std::io::{self, IsTerminal, Write};
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;

use crate::domain::EventKind;
use crate::protocol::{EventListParams, RepositoryScope};
use crate::rpc::RpcClient;

use super::output::{phase_label, print_decision_hints};

pub(super) async fn follow_status(
    client: &RpcClient,
    scope: RepositoryScope,
    workspace: &str,
) -> Result<()> {
    let mut after_sequence = 0_i64;
    let mut last_phase: Option<String> = None;
    let mut last_message: Option<String> = None;
    let interactive = io::stdout().is_terminal();
    let spinner = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let mut spinner_index = 0_usize;
    loop {
        let response = client
            .request(EventListParams {
                scope: scope.clone(),
                workspace: workspace.to_owned(),
                after_sequence,
            })
            .await?;
        for event in &response.events {
            if event.kind == EventKind::TurnStarted {
                last_message = None;
            } else if event.kind == EventKind::AgentMessageCompleted
                && let Some(message) = event.payload.get("text").and_then(Value::as_str)
            {
                last_message = Some(message.to_owned());
            }
        }
        after_sequence = response.next_sequence;
        let phase = response.workspace.phase.as_str();
        let name = response.workspace.name.as_str();
        if !interactive && last_phase.as_deref() != Some(phase) {
            println!("{name}: {}", phase_label(phase));
        }
        last_phase = Some(phase.to_owned());
        if follow_stops_at(phase) || !response.open_decisions.is_empty() {
            if interactive {
                clear_status_line()?;
                println!("{name}: {}", phase_label(phase));
            }
            let decisions = serde_json::to_value(&response.open_decisions)?;
            print_decision_hints(decisions.as_array().map(Vec::as_slice).unwrap_or_default());
            if response.open_decisions.is_empty()
                && matches!(phase, "waiting_for_approval" | "waiting_for_input")
            {
                println!("blocked: this Codex request is not supported by coco decide");
            }
            if let Some(message) = last_message {
                println!("\n{message}");
            }
            return Ok(());
        }
        for _ in 0..4 {
            if interactive {
                print!(
                    "\r\x1b[2K{} {name}: {}",
                    spinner[spinner_index % spinner.len()],
                    phase_label(phase)
                );
                io::stdout().flush()?;
                spinner_index += 1;
            }
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    if interactive {
                        clear_status_line()?;
                    }
                    return Ok(());
                },
                _ = tokio::time::sleep(Duration::from_millis(125)) => {}
            }
        }
    }
}

fn clear_status_line() -> Result<()> {
    print!("\r\x1b[2K");
    io::stdout().flush()?;
    Ok(())
}

pub(super) fn follow_stops_at(phase: &str) -> bool {
    matches!(
        phase,
        "waiting_for_approval"
            | "waiting_for_input"
            | "idle"
            | "not_loaded"
            | "system_error"
            | "unavailable"
            | "completed"
            | "failed"
    )
}
