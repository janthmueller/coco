//! Optional, best-effort client-location adapters. Never used by status polling.

use std::path::Path;
use std::time::Duration;

use sha2::{Digest, Sha256};
use tokio::process::Command;

use crate::diagnostics::capture_command_with_timeout;
use crate::domain::clients::{ClientIntegration, ClientMetadata};

const TMUX_TIMEOUT: Duration = Duration::from_millis(300);
const TMUX_FORMAT: &str = "#{socket_path}\t#{pid}\t#{start_time}\t#{pane_id}\t#{session_name}\t#{window_index}\t#{pane_index}";

pub(super) async fn native_tui_metadata() -> ClientMetadata {
    let context = std::env::var("TMUX")
        .ok()
        .zip(std::env::var("TMUX_PANE").ok())
        .and_then(|(server, pane)| TmuxContext::parse(&server, &pane));
    let integration = match context {
        Some(context) => inspect_tmux(Command::new("tmux"), &context).await,
        None => None,
    };
    ClientMetadata {
        kind: "native_tui".to_owned(),
        integration,
    }
}

struct TmuxContext {
    socket: String,
    pid: String,
    pane: String,
}

impl TmuxContext {
    fn parse(server: &str, pane: &str) -> Option<Self> {
        if server.len() > 1152 || pane.len() > 32 || !valid_pane(pane) {
            return None;
        }
        let mut parts = server.rsplitn(3, ',');
        let session = parts.next()?;
        let pid = parts.next()?;
        let socket = parts.next()?;
        if !valid_number(session)
            || !valid_number(pid)
            || socket.len() > 1024
            || !Path::new(socket).is_absolute()
            || socket.chars().any(char::is_control)
        {
            return None;
        }
        Some(Self {
            socket: socket.to_owned(),
            pid: pid.to_owned(),
            pane: pane.to_owned(),
        })
    }
}

async fn inspect_tmux(mut command: Command, context: &TmuxContext) -> Option<ClientIntegration> {
    command.args([
        "-N",
        "-S",
        &context.socket,
        "display-message",
        "-p",
        "-t",
        &context.pane,
        TMUX_FORMAT,
    ]);
    let output = capture_command_with_timeout(command, TMUX_TIMEOUT)
        .await
        .ok()?;
    parse_tmux_location(&output, context)
}

fn parse_tmux_location(output: &str, context: &TmuxContext) -> Option<ClientIntegration> {
    if output.len() > 2048 {
        return None;
    }
    let fields: Vec<_> = output
        .strip_suffix('\n')
        .unwrap_or(output)
        .split('\t')
        .collect();
    let [socket, pid, started, pane, session, window, index] = fields.as_slice() else {
        return None;
    };
    if *socket != context.socket
        || *pid != context.pid
        || *pane != context.pane
        || !valid_number(started)
        || !valid_number(window)
        || !valid_number(index)
    {
        return None;
    }
    let mut identity = Sha256::new();
    for field in [socket, pid, started] {
        identity.update((field.len() as u64).to_be_bytes());
        identity.update(field.as_bytes());
    }
    let integration = ClientIntegration {
        kind: "tmux".to_owned(),
        scope: hex::encode(identity.finalize()),
        locator: (*pane).to_owned(),
        label: Some(format!("{session}:{window}.{index}")),
    };
    ClientMetadata {
        kind: "native_tui".to_owned(),
        integration: Some(integration.clone()),
    }
    .is_valid()
    .then_some(integration)
}

fn valid_pane(value: &str) -> bool {
    value.strip_prefix('%').is_some_and(valid_number)
}

fn valid_number(value: &str) -> bool {
    !value.is_empty() && value.len() <= 20 && value.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
mod tests;
