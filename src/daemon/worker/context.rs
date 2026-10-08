use std::collections::HashSet;
use std::future::Future;

use serde::Deserialize;
use serde_json::{Value, json};

use super::{CodexError, CodexWorker, WorkerError};

const TURN_PAGE_LIMIT: usize = 50;
const MAX_TURN_PAGES: usize = 4;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TurnPage {
    data: Vec<TurnBoundary>,
    next_cursor: Option<String>,
}

#[derive(Deserialize)]
struct TurnBoundary {
    id: String,
    status: String,
}

impl CodexWorker {
    pub(super) async fn read_last_completed_turn_id(
        &self,
        thread_id: &str,
    ) -> Result<Option<String>, WorkerError> {
        read_completed_boundary(thread_id, |params| async {
            self.client
                .request("thread/turns/list", params)
                .await
                .map_err(WorkerError::runtime)
        })
        .await
    }
}

async fn read_completed_boundary<F, R>(
    thread_id: &str,
    mut read_page: F,
) -> Result<Option<String>, WorkerError>
where
    F: FnMut(Value) -> R,
    R: Future<Output = Result<Value, WorkerError>>,
{
    let mut cursor: Option<String> = None;
    let mut seen_cursors = HashSet::new();
    for _ in 0..MAX_TURN_PAGES {
        let response = read_page(json!({
            "threadId": thread_id,
            "limit": TURN_PAGE_LIMIT,
            "sortDirection": "desc",
            "itemsView": "notLoaded",
            "cursor": cursor,
        }))
        .await?;
        let page = decode_turn_page(response)?;
        if let Some(boundary) = completed_boundary(&page)? {
            return Ok(Some(boundary.to_owned()));
        }
        let Some(next) = page.next_cursor else {
            return Ok(None);
        };
        validate_next_cursor(&next, &mut seen_cursors)?;
        cursor = Some(next);
    }
    Err(invalid_boundary(
        "completed-turn lookup exceeded its page limit",
    ))
}

pub(super) fn fork_error(error: CodexError, has_cutoff: bool) -> WorkerError {
    if has_cutoff
        && matches!(&error, CodexError::Rpc { code: -32600, message, .. }
            if message.starts_with("lastTurnId '")
                && message.ends_with("' is not a persisted canonical turn in the source thread"))
    {
        return WorkerError::ContextBoundaryUnsupported;
    }
    WorkerError::runtime(error)
}

fn decode_turn_page(response: Value) -> Result<TurnPage, WorkerError> {
    let page: TurnPage = serde_json::from_value(response)
        .map_err(|error| WorkerError::InvalidThreadRead(error.to_string()))?;
    if page.data.len() > TURN_PAGE_LIMIT {
        return Err(invalid_boundary("turn page exceeds its requested limit"));
    }
    Ok(page)
}

fn completed_boundary(page: &TurnPage) -> Result<Option<&str>, WorkerError> {
    for turn in &page.data {
        if turn.id.trim().is_empty() {
            return Err(invalid_boundary("turn ID is empty"));
        }
        match turn.status.as_str() {
            "completed" | "interrupted" | "failed" => {
                // Codex 0.160.1 projects pre-TurnStarted history with these
                // synthetic IDs, but cannot use them as lastTurnId anchors.
                // Do not skip this turn and silently capture an older prefix.
                if turn.id.strip_prefix("rollout-").is_some_and(|suffix| {
                    !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
                }) {
                    return Err(WorkerError::ContextBoundaryUnsupported);
                }
                return Ok(Some(&turn.id));
            }
            "inProgress" => {}
            _ => return Err(invalid_boundary("turn status is unsupported")),
        }
    }
    Ok(None)
}

fn validate_next_cursor(next: &str, seen: &mut HashSet<String>) -> Result<(), WorkerError> {
    if next.trim().is_empty() || !seen.insert(next.to_owned()) {
        return Err(invalid_boundary("turn pagination cursor did not advance"));
    }
    Ok(())
}

fn invalid_boundary(message: &str) -> WorkerError {
    WorkerError::InvalidThreadRead(format!("context boundary: {message}"))
}

#[cfg(test)]
mod tests;
