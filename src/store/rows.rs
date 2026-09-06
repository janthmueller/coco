use std::path::{Path, PathBuf};

use rusqlite::types::Type;
use rusqlite::{Connection, OptionalExtension, Row};
use serde_json::Value;

use super::{StoreError, path_text};
#[cfg(test)]
use crate::domain::{Audit, AuditOutcome};
use crate::domain::{
    CodexThreadStatus, ContextMode, EventKind, EventSource, NormalizedEvent, Repository, Task,
    TaskLifecycle, ThreadRuntimeSnapshot, Turn, TurnPhase, derive_task_runtime,
};

pub(super) const TASK_SELECT: &str = "SELECT id, create_operation_id, repository_id, name,
    context_mode, context_json, profile_json, lifecycle, thread_status_json,
    thread_status_generation, thread_status_observed_at_ms, thread_status_is_fresh,
    branch_name, base_sha, worktree_path, codex_thread_id, parent_thread_id, active_turn_id,
    last_error_code, last_error_message, created_at_ms, updated_at_ms, completed_at_ms FROM tasks";

pub(super) const TURN_SELECT: &str = "SELECT id, task_id, operation_id, client_message_id,
    codex_turn_id, phase, requested_at_ms, started_at_ms, completed_at_ms, error_json FROM turns";

pub(super) const EVENT_SELECT: &str = "SELECT sequence, id, task_id, turn_id, kind, source,
    source_method, occurred_at_ms, recorded_at_ms, payload_json FROM events";

#[cfg(test)]
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
    let lifecycle: String = row.get(7)?;
    let lifecycle = TaskLifecycle::parse(&lifecycle)
        .ok_or_else(|| invalid_value(7, "task lifecycle", &lifecycle))?;
    let thread_runtime = map_thread_status(row)?;
    let active_turn_id: Option<String> = row.get(17)?;
    let (phase, wait_reasons) =
        derive_task_runtime(lifecycle, thread_runtime.as_ref(), active_turn_id.is_some());
    Ok(Task {
        id: row.get(0)?,
        create_operation_id: row.get(1)?,
        repository_id: row.get(2)?,
        name: row.get(3)?,
        context_mode: ContextMode::parse(&context_mode)
            .ok_or_else(|| invalid_value(4, "context_mode", &context_mode))?,
        context: json_from_column(row, 5)?,
        profile: json_from_column(row, 6)?,
        lifecycle,
        thread_runtime,
        phase,
        wait_reasons,
        branch_name: row.get(12)?,
        base_sha: row.get(13)?,
        worktree_path: row.get::<_, Option<String>>(14)?.map(PathBuf::from),
        codex_thread_id: row.get(15)?,
        parent_thread_id: row.get(16)?,
        active_turn_id,
        last_error_code: row.get(18)?,
        last_error_message: row.get(19)?,
        created_at_ms: row.get(20)?,
        updated_at_ms: row.get(21)?,
        completed_at_ms: row.get(22)?,
    })
}

fn map_thread_status(row: &Row<'_>) -> rusqlite::Result<Option<ThreadRuntimeSnapshot>> {
    let encoded: Option<String> = row.get(8)?;
    let Some(encoded) = encoded else {
        return Ok(None);
    };
    let status = serde_json::from_str::<CodexThreadStatus>(&encoded)
        .map(CodexThreadStatus::canonicalized)
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(8, Type::Text, Box::new(error))
        })?;
    Ok(Some(ThreadRuntimeSnapshot {
        status,
        runtime_generation: row.get(9)?,
        observed_at_ms: row.get(10)?,
        is_fresh: row.get(11)?,
    }))
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

#[cfg(test)]
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

pub(super) fn get_repository_by_root(
    connection: &Connection,
    path: &Path,
) -> Result<Option<Repository>, StoreError> {
    connection
        .query_row(
            "SELECT id, root_path, git_common_dir, display_name, is_linked_worktree,
                created_at_ms, updated_at_ms FROM repositories WHERE root_path = ?1",
            [path_text(path)?],
            map_repository,
        )
        .optional()
        .map_err(StoreError::from)
}

pub(super) fn get_repository_by_common_dir(
    connection: &Connection,
    path: &Path,
) -> Result<Option<Repository>, StoreError> {
    connection
        .query_row(
            "SELECT id, root_path, git_common_dir, display_name, is_linked_worktree,
                created_at_ms, updated_at_ms FROM repositories WHERE git_common_dir = ?1",
            [path_text(path)?],
            map_repository,
        )
        .optional()
        .map_err(StoreError::from)
}

pub(super) fn get_task_by_id(
    connection: &Connection,
    id: &str,
) -> Result<Option<Task>, StoreError> {
    connection
        .query_row(&format!("{} WHERE id = ?1", TASK_SELECT), [id], map_task)
        .optional()
        .map_err(StoreError::from)
}

pub(super) fn require_task(connection: &Connection, id: &str) -> Result<Task, StoreError> {
    get_task_by_id(connection, id)?.ok_or_else(|| StoreError::NotFound {
        entity: "task",
        id: id.to_owned(),
    })
}

pub(super) fn get_turn_by_id(
    connection: &Connection,
    id: &str,
) -> Result<Option<Turn>, StoreError> {
    connection
        .query_row(&format!("{} WHERE id = ?1", TURN_SELECT), [id], map_turn)
        .optional()
        .map_err(StoreError::from)
}

pub(super) fn require_turn(connection: &Connection, id: &str) -> Result<Turn, StoreError> {
    get_turn_by_id(connection, id)?.ok_or_else(|| StoreError::NotFound {
        entity: "turn",
        id: id.to_owned(),
    })
}
