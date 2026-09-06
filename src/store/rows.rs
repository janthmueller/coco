use std::path::PathBuf;

use rusqlite::Row;
use rusqlite::types::Type;
use serde_json::Value;

use super::StoreError;
use crate::domain::{
    Audit, AuditOutcome, ContextMode, EventKind, EventSource, NormalizedEvent, Repository, Task,
    TaskPhase, Turn, TurnPhase,
};

pub(super) const TASK_SELECT: &str = "SELECT id, create_operation_id, repository_id, name,
    context_mode, context_json, profile_json, phase, branch_name, base_sha, worktree_path,
    codex_thread_id, parent_thread_id, active_turn_id, last_error_code, last_error_message,
    created_at_ms, updated_at_ms, completed_at_ms FROM tasks";

pub(super) const TURN_SELECT: &str = "SELECT id, task_id, operation_id, client_message_id,
    codex_turn_id, phase, requested_at_ms, started_at_ms, completed_at_ms, error_json FROM turns";

pub(super) const EVENT_SELECT: &str = "SELECT sequence, id, task_id, turn_id, kind, source,
    source_method, occurred_at_ms, recorded_at_ms, payload_json FROM events";

pub(super) const AUDIT_SELECT: &str = "SELECT sequence, id, source, action, task_id, operation_id,
    outcome, details_json, occurred_at_ms FROM audit_events";

pub(super) fn map_repository(row: &Row<'_>) -> rusqlite::Result<Repository> {
    Ok(Repository {
        id: row.get(0)?,
        root_path: PathBuf::from(row.get::<_, String>(1)?),
        git_common_dir: PathBuf::from(row.get::<_, String>(2)?),
        display_name: row.get(3)?,
        is_linked_worktree: row.get(4)?,
        created_at_ms: row.get(5)?,
        updated_at_ms: row.get(6)?,
    })
}

pub(super) fn map_task(row: &Row<'_>) -> rusqlite::Result<Task> {
    let context_mode: String = row.get(4)?;
    let phase: String = row.get(7)?;
    Ok(Task {
        id: row.get(0)?,
        create_operation_id: row.get(1)?,
        repository_id: row.get(2)?,
        name: row.get(3)?,
        context_mode: ContextMode::parse(&context_mode)
            .ok_or_else(|| invalid_value(4, "context_mode", &context_mode))?,
        context: json_from_column(row, 5)?,
        profile: json_from_column(row, 6)?,
        phase: TaskPhase::parse(&phase).ok_or_else(|| invalid_value(7, "phase", &phase))?,
        branch_name: row.get(8)?,
        base_sha: row.get(9)?,
        worktree_path: row.get::<_, Option<String>>(10)?.map(PathBuf::from),
        codex_thread_id: row.get(11)?,
        parent_thread_id: row.get(12)?,
        active_turn_id: row.get(13)?,
        last_error_code: row.get(14)?,
        last_error_message: row.get(15)?,
        created_at_ms: row.get(16)?,
        updated_at_ms: row.get(17)?,
        completed_at_ms: row.get(18)?,
    })
}

pub(super) fn map_turn(row: &Row<'_>) -> rusqlite::Result<Turn> {
    let phase: String = row.get(5)?;
    Ok(Turn {
        id: row.get(0)?,
        task_id: row.get(1)?,
        operation_id: row.get(2)?,
        client_message_id: row.get(3)?,
        codex_turn_id: row.get(4)?,
        phase: TurnPhase::parse(&phase).ok_or_else(|| invalid_value(5, "turn phase", &phase))?,
        requested_at_ms: row.get(6)?,
        started_at_ms: row.get(7)?,
        completed_at_ms: row.get(8)?,
        error: json_option_from_column(row, 9)?,
    })
}

pub(super) fn map_event(row: &Row<'_>) -> rusqlite::Result<NormalizedEvent> {
    let kind: String = row.get(4)?;
    let source: String = row.get(5)?;
    Ok(NormalizedEvent {
        sequence: row.get(0)?,
        id: row.get(1)?,
        task_id: row.get(2)?,
        turn_id: row.get(3)?,
        kind: EventKind::parse(&kind).ok_or_else(|| invalid_value(4, "event kind", &kind))?,
        source: EventSource::parse(&source)
            .ok_or_else(|| invalid_value(5, "event source", &source))?,
        source_method: row.get(6)?,
        occurred_at_ms: row.get(7)?,
        recorded_at_ms: row.get(8)?,
        payload: json_from_column(row, 9)?,
    })
}

pub(super) fn map_audit(row: &Row<'_>) -> rusqlite::Result<Audit> {
    let outcome: String = row.get(6)?;
    Ok(Audit {
        sequence: row.get(0)?,
        id: row.get(1)?,
        source: row.get(2)?,
        action: row.get(3)?,
        task_id: row.get(4)?,
        operation_id: row.get(5)?,
        outcome: AuditOutcome::parse(&outcome)
            .ok_or_else(|| invalid_value(6, "audit outcome", &outcome))?,
        details: json_from_column(row, 7)?,
        occurred_at_ms: row.get(8)?,
    })
}

fn json_from_column<T: serde::de::DeserializeOwned>(
    row: &Row<'_>,
    index: usize,
) -> rusqlite::Result<T> {
    let encoded: String = row.get(index)?;
    serde_json::from_str(&encoded).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
    })
}

fn json_option_from_column(row: &Row<'_>, index: usize) -> rusqlite::Result<Option<Value>> {
    row.get::<_, Option<String>>(index)?
        .map(|encoded| {
            serde_json::from_str(&encoded).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
            })
        })
        .transpose()
}

fn invalid_value(index: usize, field: &'static str, value: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        index,
        Type::Text,
        Box::new(StoreError::InvalidStoredValue {
            field,
            value: value.to_owned(),
        }),
    )
}
