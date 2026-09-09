use std::io::{self, IsTerminal, Write};
use std::time::Duration;

use anyhow::{Result, bail};

use crate::protocol::{TurnResult, TurnResultParams, TurnTerminalStatus};
use crate::rpc::RpcClient;

use super::style::{Palette, Tone};

pub(super) async fn wait_for_turn(client: &RpcClient, operation_id: &str) -> Result<()> {
    let interactive = io::stderr().is_terminal();
    let palette = Palette::stderr();
    let spinner = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let mut spinner_index = 0_usize;
    loop {
        match client
            .request(TurnResultParams {
                operation_id: operation_id.to_owned(),
            })
            .await?
        {
            TurnResult::Pending { .. } => {}
            TurnResult::Finished {
                status,
                response,
                response_truncated,
                ..
            } => {
                if interactive {
                    clear_wait_line()?;
                }
                if let Some(response) = response {
                    print!("{response}");
                    if !response.ends_with('\n') {
                        println!();
                    }
                } else if status == TurnTerminalStatus::Completed {
                    eprintln!(
                        "{} Codex turn completed without a final agent response.",
                        palette.paint(Tone::Dim, "○")
                    );
                }
                if response_truncated {
                    eprintln!(
                        "{} Codex response was truncated by CoCo.",
                        palette.paint(Tone::RedBold, "Warning:")
                    );
                }
                return match status {
                    TurnTerminalStatus::Completed => Ok(()),
                    TurnTerminalStatus::Interrupted | TurnTerminalStatus::Failed => {
                        bail!("Codex turn was {}", status.as_str())
                    }
                };
            }
            TurnResult::Unavailable { reason } => {
                if interactive {
                    clear_wait_line()?;
                }
                bail!("cannot wait for this Codex turn: {reason}");
            }
        }

        if interactive {
            eprint!(
                "\r\x1b[2K{} Waiting for {}",
                palette.paint(Tone::CyanBold, spinner[spinner_index % spinner.len()]),
                palette.paint(Tone::Magenta, "Codex"),
            );
            io::stderr().flush()?;
            spinner_index += 1;
        }
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                if interactive {
                    clear_wait_line()?;
                }
                eprintln!(
                    "{} Stopped waiting; the Codex turn continues.",
                    palette.paint(Tone::CyanBold, "→")
                );
                eprintln!(
                    "{}",
                    palette.paint(
                        Tone::Dim,
                        format!(
                            "Retry the same send with --operation-id {operation_id} --wait."
                        )
                    )
                );
                return Ok(());
            },
            _ = tokio::time::sleep(Duration::from_millis(250)) => {}
        }
    }
}

fn clear_wait_line() -> Result<()> {
    eprint!("\r\x1b[2K");
    io::stderr().flush()?;
    Ok(())
}
