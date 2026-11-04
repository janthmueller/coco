use std::collections::HashSet;
use std::future::Future;

use serde::Deserialize;
use serde_json::{Value, json};

use super::{CodexWorker, NativeTurnOutcome, WorkerError};
use crate::codex::CodexError;
use crate::domain::turns::TurnTerminalStatus;

const PAGE_SIZE: usize = 50;
const MAX_PAGES: usize = 4;

// This verifies only indexed, item-free history lookup. Codex 0.166.0 does NOT
// provide atomic exact-turn interruption; the worker keeps that capability off.
fn supports_indexed_lookup(version: Option<&str>) -> bool {
    version == Some("0.166.0")
}

pub(super) fn supports_native_interrupt(version: Option<&str>) -> bool {
    // Verified request/rejection shape only, NOT an atomicity guarantee.
    version == Some("0.166.0")
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    data: Vec<Turn>,
    next_cursor: Option<String>,
}

#[derive(Deserialize)]
struct Turn {
    id: String,
    status: String,
}

impl CodexWorker {
    pub(super) async fn request_best_effort_interrupt(
        &self,
        thread: &str,
        turn: &str,
    ) -> Result<(), WorkerError> {
        if !supports_native_interrupt(self.client.server_version()) {
            return Err(WorkerError::BestEffortInterruptUnsupported);
        }
        native_interrupt(thread, turn, |params| {
            self.client.request("turn/interrupt", params)
        })
        .await
    }

    pub(super) async fn lookup_turn_outcome(
        &self,
        thread_id: &str,
        turn_id: &str,
    ) -> Result<NativeTurnOutcome, WorkerError> {
        if !supports_indexed_lookup(self.client.server_version()) {
            return Ok(NativeTurnOutcome::HistoryUnavailable);
        }
        read_indexed_outcome(thread_id, turn_id, |method, params| async move {
            self.client
                .request(method, params)
                .await
                .map_err(WorkerError::runtime)
        })
        .await
    }
}

async fn native_interrupt<F, R>(thread: &str, turn: &str, mut request: F) -> Result<(), WorkerError>
where
    F: FnMut(Value) -> R,
    R: Future<Output = Result<Value, CodexError>>,
{
    // In Codex an empty turnId is a startup/current-turn interrupt. Never use it.
    if thread.is_empty() || turn.is_empty() {
        return Err(WorkerError::InvalidResponse(
            "nonempty interrupt threadId/turnId",
        ));
    }
    match request(json!({"threadId": thread, "turnId": turn})).await {
        Ok(response) if response.is_object() => Ok(()),
        Ok(_) => Err(WorkerError::InvalidResponse(
            "turn/interrupt acknowledgement",
        )),
        Err(CodexError::Rpc {
            code: -32600,
            message,
            ..
        }) if message == "no active turn to interrupt"
            || (message.starts_with("expected active turn id ")
                && message.contains(" but found ")) =>
        {
            // Exact 0.166.0 validation failures occur BEFORE queuing Op::Interrupt.
            Err(WorkerError::TurnInterruptRejected)
        }
        Err(error) => Err(WorkerError::runtime(error)),
    }
}

async fn read_indexed_outcome<F, R>(
    thread: &str,
    turn: &str,
    mut read: F,
) -> Result<NativeTurnOutcome, WorkerError>
where
    F: FnMut(&'static str, Value) -> R,
    R: Future<Output = Result<Value, WorkerError>>,
{
    let response = read(
        "thread/read",
        json!({"threadId": thread, "includeTurns": false}),
    )
    .await?;
    let id = response
        .pointer("/thread/id")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("thread metadata omitted its identity"))?;
    if id != thread {
        return Err(WorkerError::ThreadIdMismatch {
            expected: thread.to_owned(),
            actual: id.to_owned(),
        });
    }
    if response
        .pointer("/thread/historyMode")
        .and_then(Value::as_str)
        != Some("paginated")
    {
        // On legacy history, notLoaded only trims the response AFTER a complete
        // rollout replay. Never issue that request or guess missing mode safe.
        return Ok(NativeTurnOutcome::HistoryUnavailable);
    }
    read_outcome(thread, turn, |params| read("thread/turns/list", params)).await
}

async fn read_outcome<F, R>(
    thread: &str,
    turn: &str,
    mut read: F,
) -> Result<NativeTurnOutcome, WorkerError>
where
    F: FnMut(Value) -> R,
    R: Future<Output = Result<Value, WorkerError>>,
{
    let mut cursor = None;
    let mut seen = HashSet::new();
    for _ in 0..MAX_PAGES {
        let response = read(json!({"threadId": thread, "itemsView": "notLoaded",
            "limit": PAGE_SIZE, "sortDirection": "desc", "cursor": cursor}))
        .await?;
        let page: Page =
            serde_json::from_value(response).map_err(|_| invalid("invalid turn metadata page"))?;
        if page.data.len() > PAGE_SIZE {
            return Err(invalid("oversized turn metadata page"));
        }
        for item in page.data {
            if item.id == turn {
                return match item.status.as_str() {
                    "inProgress" => Ok(NativeTurnOutcome::InProgress),
                    status => TurnTerminalStatus::parse(status)
                        .map(NativeTurnOutcome::Finished)
                        .ok_or_else(|| invalid("unknown native turn status")),
                };
            }
        }
        let Some(next) = page.next_cursor else {
            return Ok(NativeTurnOutcome::Unavailable);
        };
        if next.is_empty() || !seen.insert(next.clone()) {
            return Err(invalid("turn metadata cursor did not advance"));
        }
        cursor = Some(next);
    }
    // A bounded search not finding this ID is not proof of absence.
    Ok(NativeTurnOutcome::Unavailable)
}

fn invalid(message: &str) -> WorkerError {
    WorkerError::InvalidThreadRead(message.to_owned())
}

#[cfg(test)]
mod tests;
