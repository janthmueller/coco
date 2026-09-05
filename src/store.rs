use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use chrono::Utc;
use rusqlite::types::Type;
use rusqlite::{
    Connection, OpenFlags, OptionalExtension, Row, Transaction, TransactionBehavior, params,
};
use serde_json::{Value, json};
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{
    Audit, AuditOutcome, ContextMode, EventKind, EventSource, NormalizedEvent, ProfileSnapshot,
    Repository, Task, TaskPhase, Turn, TurnPhase,
};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("database filesystem error for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("refusing database symlink or non-regular file: {0}")]
    UnsafeDatabasePath(PathBuf),
    #[error("path is not valid UTF-8 and cannot be represented in the wire/storage model: {0:?}")]
    NonUtf8Path(PathBuf),
    #[error("database schema version {0} is newer than this CoCo build supports")]
    UnsupportedSchema(i64),
    #[error("database mutex is poisoned")]
    Poisoned,
    #[error("{entity} not found: {id}")]
    NotFound { entity: &'static str, id: String },
    #[error("invalid {field} value in database: {value}")]
    InvalidStoredValue { field: &'static str, value: String },
    #[error("invalid transition for task {task_id}: expected {expected}, observed {actual}")]
    InvalidTaskTransition {
        task_id: String,
        expected: String,
        actual: String,
    },
    #[error("turn {turn_id} cannot complete with phase {phase}")]
    InvalidTurnCompletion { turn_id: String, phase: String },
    #[error("event turn {turn_id} belongs to task {turn_task_id}, not {event_task_id}")]
    EventCorrelation {
        turn_id: String,
        turn_task_id: String,
        event_task_id: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewTask {
    pub create_operation_id: Option<String>,
    pub repository_id: String,
    pub name: String,
    pub context_mode: ContextMode,
    pub context: Value,
    pub profile: ProfileSnapshot,
    pub branch_name: Option<String>,
    pub base_sha: Option<String>,
    pub worktree_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventDraft {
    pub task_id: Option<String>,
    pub turn_id: Option<String>,
    pub kind: EventKind,
    pub source: EventSource,
    pub source_method: Option<String>,
    pub occurred_at_ms: Option<i64>,
    pub payload: Value,
}

impl EventDraft {
    pub fn task(kind: EventKind, source: EventSource, payload: Value) -> Self {
        Self {
            task_id: None,
            turn_id: None,
            kind,
            source,
            source_method: None,
            occurred_at_ms: None,
            payload,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTurn {
    pub operation_id: Option<String>,
    pub client_message_id: String,
    pub codex_turn_id: Option<String>,
    pub started_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TurnCompletion {
    pub phase: TurnPhase,
    pub error: Option<Value>,
    pub completed_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AuditDraft {
    pub source: String,
    pub action: String,
    pub task_id: Option<String>,
    pub operation_id: Option<String>,
    pub outcome: AuditOutcome,
    pub details: Value,
    pub occurred_at_ms: Option<i64>,
}

pub struct Store {
    connection: Mutex<Connection>,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref();
        secure_database_path(path)?;
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        secure_file(path, 0o600)?;
        Self::from_connection(connection)
    }

    pub fn in_memory() -> Result<Self, StoreError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(connection: Connection) -> Result<Self, StoreError> {
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;",
        )?;
        migrate(&connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn register_repository(&self, repository: &Repository) -> Result<Repository, StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(existing) =
            get_repository_by_common_dir(&transaction, &repository.git_common_dir)?
        {
            transaction.commit()?;
            return Ok(existing);
        }
        let root_path = path_text(&repository.root_path)?;
        let git_common_dir = path_text(&repository.git_common_dir)?;
        transaction.execute(
            "INSERT INTO repositories (
                id, root_path, git_common_dir, display_name, is_linked_worktree,
                created_at_ms, updated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(root_path) DO UPDATE SET
                git_common_dir = excluded.git_common_dir,
                display_name = excluded.display_name,
                is_linked_worktree = excluded.is_linked_worktree,
                updated_at_ms = excluded.updated_at_ms",
            params![
                repository.id,
                root_path,
                git_common_dir,
                repository.display_name,
                repository.is_linked_worktree,
                repository.created_at_ms,
                repository.updated_at_ms,
            ],
        )?;
        let stored =
            get_repository_by_root(&transaction, &repository.root_path)?.ok_or_else(|| {
                StoreError::NotFound {
                    entity: "repository",
                    id: repository.id.clone(),
                }
            })?;
        transaction.commit()?;
        Ok(stored)
    }

    pub fn repository_by_id(&self, id: &str) -> Result<Option<Repository>, StoreError> {
        let connection = self.lock()?;
        get_repository_by_id(&connection, id)
    }

    pub fn repository_by_root(&self, path: &Path) -> Result<Option<Repository>, StoreError> {
        let connection = self.lock()?;
        get_repository_by_root(&connection, path)
    }

    pub fn repository_by_common_dir(&self, path: &Path) -> Result<Option<Repository>, StoreError> {
        let connection = self.lock()?;
        get_repository_by_common_dir(&connection, path)
    }

    pub fn create_task_with_event(
        &self,
        input: NewTask,
        mut event: EventDraft,
    ) -> Result<(Task, NormalizedEvent), StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let now = now_ms();
        let task_id = new_id();
        let profile_json = serde_json::to_string(&input.profile).map_err(json_to_sql_error)?;
        let context_json = serde_json::to_string(&input.context).map_err(json_to_sql_error)?;
        let worktree_path = input
            .worktree_path
            .as_deref()
            .map(path_text)
            .transpose()?
            .map(str::to_owned);
        transaction.execute(
            "INSERT INTO tasks (
                id, create_operation_id, repository_id, name, context_mode,
                context_json, profile_json, phase, branch_name, base_sha, worktree_path,
                codex_thread_id, parent_thread_id, active_turn_id, last_error_code,
                last_error_message, created_at_ms, updated_at_ms, completed_at_ms
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, 'provisioning', ?8, ?9, ?10,
                NULL, NULL, NULL, NULL, NULL, ?11, ?11, NULL
             )",
            params![
                task_id,
                input.create_operation_id,
                input.repository_id,
                input.name,
                input.context_mode.as_str(),
                context_json,
                profile_json,
                input.branch_name,
                input.base_sha,
                worktree_path,
                now,
            ],
        )?;
        event.task_id = Some(task_id.clone());
        let event = insert_event(&transaction, event)?;
        let task = get_task_by_id(&transaction, &task_id)?.ok_or_else(|| StoreError::NotFound {
            entity: "task",
            id: task_id.clone(),
        })?;
        transaction.commit()?;
        Ok((task, event))
    }

    pub fn transition_task_with_event(
        &self,
        task_id: &str,
        expected: TaskPhase,
        next: TaskPhase,
        last_error: Option<(&str, &str)>,
        event: EventDraft,
    ) -> Result<(Task, NormalizedEvent), StoreError> {
        self.transition_task_from_with_event(task_id, &[expected], next, last_error, event)
    }

    pub fn transition_task_from_with_event(
        &self,
        task_id: &str,
        expected: &[TaskPhase],
        next: TaskPhase,
        last_error: Option<(&str, &str)>,
        mut event: EventDraft,
    ) -> Result<(Task, NormalizedEvent), StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        assert_task_phase(&transaction, task_id, expected)?;
        let now = now_ms();
        let (error_code, error_message) = last_error
            .map(|(code, message)| (Some(code), Some(message)))
            .unwrap_or((None, None));
        let completed_at = (next == TaskPhase::Completed).then_some(now);
        transaction.execute(
            "UPDATE tasks SET phase = ?1, last_error_code = ?2, last_error_message = ?3,
                completed_at_ms = ?4, updated_at_ms = ?5 WHERE id = ?6",
            params![
                next.as_str(),
                error_code,
                error_message,
                completed_at,
                now,
                task_id
            ],
        )?;
        event.task_id = Some(task_id.to_owned());
        let event = insert_event(&transaction, event)?;
        let task = require_task(&transaction, task_id)?;
        transaction.commit()?;
        Ok((task, event))
    }

    pub fn bind_thread_with_event(
        &self,
        task_id: &str,
        expected: TaskPhase,
        next: TaskPhase,
        thread_id: &str,
        parent_thread_id: Option<&str>,
        mut event: EventDraft,
    ) -> Result<(Task, NormalizedEvent), StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        assert_task_phase(&transaction, task_id, &[expected])?;
        transaction.execute(
            "UPDATE tasks SET codex_thread_id = ?1, parent_thread_id = ?2, phase = ?3,
                updated_at_ms = ?4 WHERE id = ?5",
            params![
                thread_id,
                parent_thread_id,
                next.as_str(),
                now_ms(),
                task_id
            ],
        )?;
        event.task_id = Some(task_id.to_owned());
        let event = insert_event(&transaction, event)?;
        let task = require_task(&transaction, task_id)?;
        transaction.commit()?;
        Ok((task, event))
    }

    pub fn start_turn_with_event(
        &self,
        task_id: &str,
        allowed_task_phases: &[TaskPhase],
        input: NewTurn,
        mut event: EventDraft,
    ) -> Result<(Task, Turn, NormalizedEvent), StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        assert_task_phase(&transaction, task_id, allowed_task_phases)?;
        let now = now_ms();
        let turn_id = new_id();
        let turn_phase = if input.codex_turn_id.is_some() {
            TurnPhase::InProgress
        } else {
            TurnPhase::Starting
        };
        transaction.execute(
            "INSERT INTO turns (
                id, task_id, operation_id, client_message_id, codex_turn_id, phase,
                requested_at_ms, started_at_ms, completed_at_ms, error_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, NULL)",
            params![
                turn_id,
                task_id,
                input.operation_id,
                input.client_message_id,
                input.codex_turn_id,
                turn_phase.as_str(),
                now,
                if input.codex_turn_id.is_some() {
                    input.started_at_ms.or(Some(now))
                } else {
                    input.started_at_ms
                },
            ],
        )?;
        transaction.execute(
            "UPDATE tasks SET phase = 'active', active_turn_id = ?1, updated_at_ms = ?2
             WHERE id = ?3",
            params![turn_id, now, task_id],
        )?;
        event.task_id = Some(task_id.to_owned());
        event.turn_id = Some(turn_id.clone());
        let event = insert_event(&transaction, event)?;
        let task = require_task(&transaction, task_id)?;
        let turn = require_turn(&transaction, &turn_id)?;
        transaction.commit()?;
        Ok((task, turn, event))
    }

    pub fn complete_turn_with_event(
        &self,
        task_id: &str,
        turn_id: &str,
        completion: TurnCompletion,
        mut event: EventDraft,
    ) -> Result<(Task, Turn, NormalizedEvent), StoreError> {
        let task_phase = match completion.phase {
            TurnPhase::Completed => TaskPhase::Idle,
            TurnPhase::Failed => TaskPhase::Failed,
            TurnPhase::Interrupted => TaskPhase::Interrupted,
            phase => {
                return Err(StoreError::InvalidTurnCompletion {
                    turn_id: turn_id.to_owned(),
                    phase: phase.as_str().to_owned(),
                });
            }
        };
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        assert_task_phase(
            &transaction,
            task_id,
            &[
                TaskPhase::Active,
                TaskPhase::WaitingForApproval,
                TaskPhase::WaitingForInput,
            ],
        )?;
        let current = require_task(&transaction, task_id)?;
        if current.active_turn_id.as_deref() != Some(turn_id) {
            return Err(StoreError::InvalidTaskTransition {
                task_id: task_id.to_owned(),
                expected: format!("active turn {turn_id}"),
                actual: current
                    .active_turn_id
                    .as_deref()
                    .unwrap_or("no active turn")
                    .to_owned(),
            });
        }
        let now = completion.completed_at_ms.unwrap_or_else(now_ms);
        let error_json = completion
            .error
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(json_to_sql_error)?;
        let updated = transaction.execute(
            "UPDATE turns SET phase = ?1, completed_at_ms = ?2, error_json = ?3
             WHERE id = ?4 AND task_id = ?5 AND phase IN ('starting', 'in_progress')",
            params![completion.phase.as_str(), now, error_json, turn_id, task_id],
        )?;
        if updated != 1 {
            return Err(StoreError::InvalidTaskTransition {
                task_id: task_id.to_owned(),
                expected: format!("unfinished turn {turn_id}"),
                actual: "turn missing or already terminal".to_owned(),
            });
        }
        let (error_code, error_message) = sanitized_error_columns(completion.error.as_ref());
        transaction.execute(
            "UPDATE tasks SET phase = ?1, active_turn_id = NULL, last_error_code = ?2,
                last_error_message = ?3, updated_at_ms = ?4 WHERE id = ?5",
            params![task_phase.as_str(), error_code, error_message, now, task_id],
        )?;
        event.task_id = Some(task_id.to_owned());
        event.turn_id = Some(turn_id.to_owned());
        let event = insert_event(&transaction, event)?;
        let task = require_task(&transaction, task_id)?;
        let turn = require_turn(&transaction, turn_id)?;
        transaction.commit()?;
        Ok((task, turn, event))
    }

    pub fn append_event(&self, event: EventDraft) -> Result<NormalizedEvent, StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let event = insert_event(&transaction, event)?;
        transaction.commit()?;
        Ok(event)
    }

    pub fn append_audit(&self, input: AuditDraft) -> Result<Audit, StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let audit = insert_audit(&transaction, input)?;
        transaction.commit()?;
        Ok(audit)
    }

    pub fn task_by_id(&self, id: &str) -> Result<Option<Task>, StoreError> {
        let connection = self.lock()?;
        get_task_by_id(&connection, id)
    }

    pub fn task_by_create_operation_id(
        &self,
        operation_id: &str,
    ) -> Result<Option<Task>, StoreError> {
        let connection = self.lock()?;
        connection
            .query_row(
                &format!("{} WHERE create_operation_id = ?1", TASK_SELECT),
                [operation_id],
                map_task,
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn task_by_thread_id(&self, thread_id: &str) -> Result<Option<Task>, StoreError> {
        let connection = self.lock()?;
        connection
            .query_row(
                &format!("{} WHERE codex_thread_id = ?1", TASK_SELECT),
                [thread_id],
                map_task,
            )
            .optional()
            .map_err(StoreError::from)
    }

    /// Replaces a provisional profile with the effective, non-secret App Server settings.
    /// Profiles become immutable once the first turn is active.
    pub fn update_task_profile(
        &self,
        task_id: &str,
        profile: &ProfileSnapshot,
    ) -> Result<Task, StoreError> {
        let profile_json = serde_json::to_string(profile).map_err(json_to_sql_error)?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        assert_task_phase(
            &transaction,
            task_id,
            &[TaskPhase::Provisioning, TaskPhase::Starting],
        )?;
        transaction.execute(
            "UPDATE tasks SET profile_json = ?1, updated_at_ms = ?2 WHERE id = ?3",
            params![profile_json, now_ms(), task_id],
        )?;
        let task = require_task(&transaction, task_id)?;
        transaction.commit()?;
        Ok(task)
    }

    pub fn task_by_name(
        &self,
        repository_id: &str,
        name: &str,
    ) -> Result<Option<Task>, StoreError> {
        let connection = self.lock()?;
        connection
            .query_row(
                &format!("{} WHERE repository_id = ?1 AND name = ?2", TASK_SELECT),
                params![repository_id, name],
                map_task,
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn list_tasks(&self, repository_id: Option<&str>) -> Result<Vec<Task>, StoreError> {
        let connection = self.lock()?;
        if let Some(repository_id) = repository_id {
            let mut statement = connection.prepare(&format!(
                "{} WHERE repository_id = ?1 ORDER BY updated_at_ms DESC, id",
                TASK_SELECT
            ))?;
            statement
                .query_map([repository_id], map_task)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(StoreError::from)
        } else {
            let mut statement =
                connection.prepare(&format!("{} ORDER BY updated_at_ms DESC, id", TASK_SELECT))?;
            statement
                .query_map([], map_task)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(StoreError::from)
        }
    }

    pub fn turn_by_id(&self, id: &str) -> Result<Option<Turn>, StoreError> {
        let connection = self.lock()?;
        get_turn_by_id(&connection, id)
    }

    pub fn turn_by_operation_id(&self, operation_id: &str) -> Result<Option<Turn>, StoreError> {
        let connection = self.lock()?;
        connection
            .query_row(
                &format!("{} WHERE operation_id = ?1", TURN_SELECT),
                [operation_id],
                map_turn,
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn turn_by_codex_id(&self, codex_turn_id: &str) -> Result<Option<Turn>, StoreError> {
        let connection = self.lock()?;
        connection
            .query_row(
                &format!("{} WHERE codex_turn_id = ?1", TURN_SELECT),
                [codex_turn_id],
                map_turn,
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn events_after(
        &self,
        task_id: Option<&str>,
        after_sequence: i64,
    ) -> Result<Vec<NormalizedEvent>, StoreError> {
        let connection = self.lock()?;
        let mut statement = if task_id.is_some() {
            connection.prepare(&format!(
                "{} WHERE task_id = ?1 AND sequence > ?2 ORDER BY sequence",
                EVENT_SELECT
            ))?
        } else {
            connection.prepare(&format!(
                "{} WHERE sequence > ?1 ORDER BY sequence",
                EVENT_SELECT
            ))?
        };
        let rows = match task_id {
            Some(task_id) => statement.query_map(params![task_id, after_sequence], map_event)?,
            None => statement.query_map(params![after_sequence], map_event)?,
        };
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn audits_after(
        &self,
        task_id: Option<&str>,
        after_sequence: i64,
    ) -> Result<Vec<Audit>, StoreError> {
        let connection = self.lock()?;
        let mut statement = if task_id.is_some() {
            connection.prepare(&format!(
                "{} WHERE task_id = ?1 AND sequence > ?2 ORDER BY sequence",
                AUDIT_SELECT
            ))?
        } else {
            connection.prepare(&format!(
                "{} WHERE sequence > ?1 ORDER BY sequence",
                AUDIT_SELECT
            ))?
        };
        let rows = match task_id {
            Some(task_id) => statement.query_map(params![task_id, after_sequence], map_audit)?,
            None => statement.query_map(params![after_sequence], map_audit)?,
        };
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    /// Marks state that cannot safely be assumed successful after daemon loss.
    pub fn reconcile_unfinished(&self) -> Result<Vec<NormalizedEvent>, StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let unfinished = [
            TaskPhase::Provisioning,
            TaskPhase::Starting,
            TaskPhase::Active,
            TaskPhase::WaitingForApproval,
            TaskPhase::WaitingForInput,
        ];
        let mut statement = transaction.prepare(&format!(
            "{} WHERE phase IN ('provisioning', 'starting', 'active',
                'waiting_for_approval', 'waiting_for_input') ORDER BY id",
            TASK_SELECT
        ))?;
        let tasks = statement
            .query_map([], map_task)?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);

        let now = now_ms();
        let mut events = Vec::with_capacity(tasks.len());
        for task in tasks {
            debug_assert!(unfinished.contains(&task.phase));
            transaction.execute(
                "UPDATE turns SET phase = 'interrupted', completed_at_ms = ?1,
                    error_json = ?2
                 WHERE task_id = ?3 AND phase IN ('starting', 'in_progress')",
                params![
                    now,
                    serde_json::to_string(&json!({
                        "code": "DAEMON_RESTART",
                        "message": "Turn state was unfinished when cocod restarted"
                    }))
                    .map_err(json_to_sql_error)?,
                    task.id,
                ],
            )?;
            transaction.execute(
                "UPDATE tasks SET phase = 'interrupted', active_turn_id = NULL,
                    last_error_code = 'DAEMON_RESTART',
                    last_error_message = 'State was unfinished when cocod restarted',
                    updated_at_ms = ?1 WHERE id = ?2",
                params![now, task.id],
            )?;
            let event_kind = if task.active_turn_id.is_some() {
                EventKind::TurnCompleted
            } else {
                EventKind::AgentFailed
            };
            events.push(insert_event(
                &transaction,
                EventDraft {
                    task_id: Some(task.id),
                    turn_id: task.active_turn_id,
                    kind: event_kind,
                    source: EventSource::Coco,
                    source_method: Some("startup.reconcile".to_owned()),
                    occurred_at_ms: None,
                    payload: json!({
                        "status": "interrupted",
                        "reason": "daemon_restart"
                    }),
                },
            )?);
        }
        transaction.commit()?;
        Ok(events)
    }

    fn lock(&self) -> Result<MutexGuard<'_, Connection>, StoreError> {
        self.connection.lock().map_err(|_| StoreError::Poisoned)
    }
}

const TASK_SELECT: &str = "SELECT id, create_operation_id, repository_id, name,
    context_mode, context_json, profile_json, phase, branch_name, base_sha, worktree_path,
    codex_thread_id, parent_thread_id, active_turn_id, last_error_code, last_error_message,
    created_at_ms, updated_at_ms, completed_at_ms FROM tasks";

const TURN_SELECT: &str = "SELECT id, task_id, operation_id, client_message_id,
    codex_turn_id, phase, requested_at_ms, started_at_ms, completed_at_ms, error_json FROM turns";

const EVENT_SELECT: &str = "SELECT sequence, id, task_id, turn_id, kind, source,
    source_method, occurred_at_ms, recorded_at_ms, payload_json FROM events";

const AUDIT_SELECT: &str = "SELECT sequence, id, source, action, task_id, operation_id,
    outcome, details_json, occurred_at_ms FROM audit_events";

fn migrate(connection: &Connection) -> Result<(), StoreError> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version > 2 {
        return Err(StoreError::UnsupportedSchema(version));
    }
    if version == 2 {
        return Ok(());
    }
    if version == 1 {
        return migrate_retired_task_goal(connection);
    }
    connection.execute_batch(
        "BEGIN IMMEDIATE;
         CREATE TABLE IF NOT EXISTS repositories (
            id TEXT PRIMARY KEY,
            root_path TEXT NOT NULL UNIQUE,
            git_common_dir TEXT NOT NULL UNIQUE,
            display_name TEXT NOT NULL,
            is_linked_worktree INTEGER NOT NULL CHECK (is_linked_worktree IN (0, 1)),
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS tasks (
            id TEXT PRIMARY KEY,
            create_operation_id TEXT UNIQUE,
            repository_id TEXT NOT NULL REFERENCES repositories(id),
            name TEXT NOT NULL,
            legacy_goal TEXT,
            context_mode TEXT NOT NULL CHECK (context_mode IN ('fresh', 'fork', 'handoff')),
            context_json TEXT NOT NULL CHECK (json_valid(context_json)),
            profile_json TEXT NOT NULL CHECK (json_valid(profile_json)),
            phase TEXT NOT NULL CHECK (phase IN (
                'provisioning', 'starting', 'active', 'waiting_for_approval',
                'waiting_for_input', 'idle', 'completed', 'failed', 'interrupted'
            )),
            branch_name TEXT,
            base_sha TEXT,
            worktree_path TEXT UNIQUE,
            codex_thread_id TEXT UNIQUE,
            parent_thread_id TEXT,
            active_turn_id TEXT,
            last_error_code TEXT,
            last_error_message TEXT,
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            completed_at_ms INTEGER,
            UNIQUE(repository_id, name),
            UNIQUE(repository_id, branch_name)
         );
         CREATE TABLE IF NOT EXISTS turns (
            id TEXT PRIMARY KEY,
            task_id TEXT NOT NULL REFERENCES tasks(id),
            operation_id TEXT UNIQUE,
            client_message_id TEXT NOT NULL UNIQUE,
            codex_turn_id TEXT UNIQUE,
            phase TEXT NOT NULL CHECK (phase IN (
                'starting', 'in_progress', 'completed', 'failed', 'interrupted'
            )),
            requested_at_ms INTEGER NOT NULL,
            started_at_ms INTEGER,
            completed_at_ms INTEGER,
            error_json TEXT CHECK (error_json IS NULL OR json_valid(error_json))
         );
         CREATE INDEX IF NOT EXISTS turns_task_idx ON turns(task_id, requested_at_ms);
         CREATE TABLE IF NOT EXISTS events (
            sequence INTEGER PRIMARY KEY AUTOINCREMENT,
            id TEXT NOT NULL UNIQUE,
            task_id TEXT REFERENCES tasks(id),
            turn_id TEXT REFERENCES turns(id),
            kind TEXT NOT NULL,
            source TEXT NOT NULL CHECK (source IN ('coco', 'git', 'codex')),
            source_method TEXT,
            occurred_at_ms INTEGER,
            recorded_at_ms INTEGER NOT NULL,
            payload_json TEXT NOT NULL CHECK (json_valid(payload_json))
         );
         CREATE INDEX IF NOT EXISTS events_task_sequence_idx ON events(task_id, sequence);
         CREATE TABLE IF NOT EXISTS audit_events (
            sequence INTEGER PRIMARY KEY AUTOINCREMENT,
            id TEXT NOT NULL UNIQUE,
            source TEXT NOT NULL,
            action TEXT NOT NULL,
            task_id TEXT REFERENCES tasks(id),
            operation_id TEXT,
            outcome TEXT NOT NULL CHECK (outcome IN ('succeeded', 'failed')),
            details_json TEXT NOT NULL CHECK (json_valid(details_json)),
            occurred_at_ms INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS audit_task_sequence_idx
            ON audit_events(task_id, sequence);
         PRAGMA user_version = 2;
         COMMIT;",
    )?;
    Ok(())
}

fn migrate_retired_task_goal(connection: &Connection) -> Result<(), StoreError> {
    connection.pragma_update(None, "foreign_keys", false)?;
    let migration = connection.execute_batch(
        "BEGIN IMMEDIATE;
         CREATE TABLE tasks_v2 (
            id TEXT PRIMARY KEY,
            create_operation_id TEXT UNIQUE,
            repository_id TEXT NOT NULL REFERENCES repositories(id),
            name TEXT NOT NULL,
            legacy_goal TEXT,
            context_mode TEXT NOT NULL CHECK (context_mode IN ('fresh', 'fork', 'handoff')),
            context_json TEXT NOT NULL CHECK (json_valid(context_json)),
            profile_json TEXT NOT NULL CHECK (json_valid(profile_json)),
            phase TEXT NOT NULL CHECK (phase IN (
                'provisioning', 'starting', 'active', 'waiting_for_approval',
                'waiting_for_input', 'idle', 'completed', 'failed', 'interrupted'
            )),
            branch_name TEXT,
            base_sha TEXT,
            worktree_path TEXT UNIQUE,
            codex_thread_id TEXT UNIQUE,
            parent_thread_id TEXT,
            active_turn_id TEXT,
            last_error_code TEXT,
            last_error_message TEXT,
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            completed_at_ms INTEGER,
            UNIQUE(repository_id, name),
            UNIQUE(repository_id, branch_name)
         );
         INSERT INTO tasks_v2 (
            id, create_operation_id, repository_id, name, legacy_goal, context_mode,
            context_json, profile_json, phase, branch_name, base_sha, worktree_path,
            codex_thread_id, parent_thread_id, active_turn_id, last_error_code,
            last_error_message, created_at_ms, updated_at_ms, completed_at_ms
         ) SELECT
            id, create_operation_id, repository_id, name, goal, context_mode,
            context_json, profile_json, phase, branch_name, base_sha, worktree_path,
            codex_thread_id, parent_thread_id, active_turn_id, last_error_code,
            last_error_message, created_at_ms, updated_at_ms, completed_at_ms
         FROM tasks;
         DROP TABLE tasks;
         ALTER TABLE tasks_v2 RENAME TO tasks;
         PRAGMA user_version = 2;
         COMMIT;",
    );
    if migration.is_err() {
        let _ = connection.execute_batch("ROLLBACK;");
    }
    let foreign_keys = connection.pragma_update(None, "foreign_keys", true);
    migration?;
    foreign_keys?;
    Ok(())
}

fn insert_event(
    transaction: &Transaction<'_>,
    mut event: EventDraft,
) -> Result<NormalizedEvent, StoreError> {
    if let Some(turn_id) = event.turn_id.as_deref() {
        let turn = require_turn(transaction, turn_id)?;
        if let Some(task_id) = event.task_id.as_deref() {
            if task_id != turn.task_id {
                return Err(StoreError::EventCorrelation {
                    turn_id: turn_id.to_owned(),
                    turn_task_id: turn.task_id,
                    event_task_id: task_id.to_owned(),
                });
            }
        } else {
            event.task_id = Some(turn.task_id);
        }
    }
    let id = new_id();
    let recorded_at_ms = now_ms();
    let payload_json = serde_json::to_string(&event.payload).map_err(json_to_sql_error)?;
    transaction.execute(
        "INSERT INTO events (
            id, task_id, turn_id, kind, source, source_method, occurred_at_ms,
            recorded_at_ms, payload_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            id,
            event.task_id,
            event.turn_id,
            event.kind.as_str(),
            event.source.as_str(),
            event.source_method,
            event.occurred_at_ms,
            recorded_at_ms,
            payload_json,
        ],
    )?;
    Ok(NormalizedEvent {
        sequence: transaction.last_insert_rowid(),
        id,
        task_id: event.task_id,
        turn_id: event.turn_id,
        kind: event.kind,
        source: event.source,
        source_method: event.source_method,
        occurred_at_ms: event.occurred_at_ms,
        recorded_at_ms,
        payload: event.payload,
    })
}

fn insert_audit(transaction: &Transaction<'_>, input: AuditDraft) -> Result<Audit, StoreError> {
    let id = new_id();
    let occurred_at_ms = input.occurred_at_ms.unwrap_or_else(now_ms);
    let details_json = serde_json::to_string(&input.details).map_err(json_to_sql_error)?;
    transaction.execute(
        "INSERT INTO audit_events (
            id, source, action, task_id, operation_id, outcome, details_json, occurred_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            id,
            input.source,
            input.action,
            input.task_id,
            input.operation_id,
            input.outcome.as_str(),
            details_json,
            occurred_at_ms,
        ],
    )?;
    Ok(Audit {
        sequence: transaction.last_insert_rowid(),
        id,
        source: input.source,
        action: input.action,
        task_id: input.task_id,
        operation_id: input.operation_id,
        outcome: input.outcome,
        details: input.details,
        occurred_at_ms,
    })
}

fn get_repository_by_id(
    connection: &Connection,
    id: &str,
) -> Result<Option<Repository>, StoreError> {
    connection
        .query_row(
            "SELECT id, root_path, git_common_dir, display_name, is_linked_worktree,
                created_at_ms, updated_at_ms FROM repositories WHERE id = ?1",
            [id],
            map_repository,
        )
        .optional()
        .map_err(StoreError::from)
}

fn get_repository_by_root(
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

fn get_repository_by_common_dir(
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

fn get_task_by_id(connection: &Connection, id: &str) -> Result<Option<Task>, StoreError> {
    connection
        .query_row(&format!("{} WHERE id = ?1", TASK_SELECT), [id], map_task)
        .optional()
        .map_err(StoreError::from)
}

fn require_task(connection: &Connection, id: &str) -> Result<Task, StoreError> {
    get_task_by_id(connection, id)?.ok_or_else(|| StoreError::NotFound {
        entity: "task",
        id: id.to_owned(),
    })
}

fn get_turn_by_id(connection: &Connection, id: &str) -> Result<Option<Turn>, StoreError> {
    connection
        .query_row(&format!("{} WHERE id = ?1", TURN_SELECT), [id], map_turn)
        .optional()
        .map_err(StoreError::from)
}

fn require_turn(connection: &Connection, id: &str) -> Result<Turn, StoreError> {
    get_turn_by_id(connection, id)?.ok_or_else(|| StoreError::NotFound {
        entity: "turn",
        id: id.to_owned(),
    })
}

fn assert_task_phase(
    connection: &Connection,
    task_id: &str,
    expected: &[TaskPhase],
) -> Result<(), StoreError> {
    let task = require_task(connection, task_id)?;
    if expected.contains(&task.phase) {
        return Ok(());
    }
    Err(StoreError::InvalidTaskTransition {
        task_id: task_id.to_owned(),
        expected: expected
            .iter()
            .map(|phase| phase.as_str())
            .collect::<Vec<_>>()
            .join(" or "),
        actual: task.phase.as_str().to_owned(),
    })
}

fn map_repository(row: &Row<'_>) -> rusqlite::Result<Repository> {
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

fn map_task(row: &Row<'_>) -> rusqlite::Result<Task> {
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

fn map_turn(row: &Row<'_>) -> rusqlite::Result<Turn> {
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

fn map_event(row: &Row<'_>) -> rusqlite::Result<NormalizedEvent> {
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

fn map_audit(row: &Row<'_>) -> rusqlite::Result<Audit> {
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

fn json_to_sql_error(error: serde_json::Error) -> StoreError {
    StoreError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
}

fn sanitized_error_columns(error: Option<&Value>) -> (Option<&str>, Option<&str>) {
    let Some(Value::Object(error)) = error else {
        return (None, None);
    };
    (
        error.get("code").and_then(Value::as_str),
        error.get("message").and_then(Value::as_str),
    )
}

fn secure_database_path(path: &Path) -> Result<(), StoreError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| StoreError::Io {
        path: parent.to_owned(),
        source,
    })?;
    if parent != Path::new(".") {
        let metadata = fs::symlink_metadata(parent).map_err(|source| StoreError::Io {
            path: parent.to_owned(),
            source,
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(StoreError::UnsafeDatabasePath(parent.to_owned()));
        }
    }
    if parent != Path::new(".") {
        secure_file(parent, 0o700)?;
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.file_type().is_file() => {
            return Err(StoreError::UnsafeDatabasePath(path.to_owned()));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let mut options = OpenOptions::new();
            options.create_new(true).write(true).read(true);
            #[cfg(unix)]
            options.mode(0o600);
            options.open(path).map_err(|source| StoreError::Io {
                path: path.to_owned(),
                source,
            })?;
        }
        Err(source) => {
            return Err(StoreError::Io {
                path: path.to_owned(),
                source,
            });
        }
    }
    Ok(())
}

fn secure_file(path: &Path, mode: u32) -> Result<(), StoreError> {
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|source| {
        StoreError::Io {
            path: path.to_owned(),
            source,
        }
    })?;
    #[cfg(not(unix))]
    let _ = (path, mode);
    Ok(())
}

fn path_text(path: &Path) -> Result<&str, StoreError> {
    path.to_str()
        .ok_or_else(|| StoreError::NonUtf8Path(path.to_owned()))
}

fn new_id() -> String {
    Uuid::new_v4().to_string()
}

fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repository(root: &Path) -> Repository {
        Repository {
            id: "repo-test".to_owned(),
            root_path: root.to_owned(),
            git_common_dir: root.join(".git"),
            display_name: "fixture".to_owned(),
            is_linked_worktree: false,
            created_at_ms: 1,
            updated_at_ms: 1,
        }
    }

    fn new_task(repository_id: &str, name: &str) -> NewTask {
        NewTask {
            create_operation_id: Some(format!("create-{name}")),
            repository_id: repository_id.to_owned(),
            name: name.to_owned(),
            context_mode: ContextMode::Fresh,
            context: json!({"version": 1, "mode": "fresh"}),
            profile: ProfileSnapshot {
                name: "default".to_owned(),
                source_path: None,
                source_hash: "sha256:test".to_owned(),
                effective_settings: json!({"network_access": false}),
            },
            branch_name: Some(format!("coco/{name}")),
            base_sha: Some("0123456789abcdef".to_owned()),
            worktree_path: Some(PathBuf::from(format!("/tmp/worktrees/{name}"))),
        }
    }

    #[test]
    fn retires_v1_goal_from_task_projection_without_losing_legacy_data() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                r#"PRAGMA foreign_keys = ON;
                 CREATE TABLE repositories (
                    id TEXT PRIMARY KEY,
                    root_path TEXT NOT NULL UNIQUE,
                    git_common_dir TEXT NOT NULL UNIQUE,
                    display_name TEXT NOT NULL,
                    is_linked_worktree INTEGER NOT NULL,
                    created_at_ms INTEGER NOT NULL,
                    updated_at_ms INTEGER NOT NULL
                 );
                 CREATE TABLE tasks (
                    id TEXT PRIMARY KEY,
                    create_operation_id TEXT UNIQUE,
                    repository_id TEXT NOT NULL REFERENCES repositories(id),
                    name TEXT NOT NULL,
                    goal TEXT NOT NULL CHECK (length(trim(goal)) > 0),
                    context_mode TEXT NOT NULL,
                    context_json TEXT NOT NULL,
                    profile_json TEXT NOT NULL,
                    phase TEXT NOT NULL,
                    branch_name TEXT,
                    base_sha TEXT,
                    worktree_path TEXT UNIQUE,
                    codex_thread_id TEXT UNIQUE,
                    parent_thread_id TEXT,
                    active_turn_id TEXT,
                    last_error_code TEXT,
                    last_error_message TEXT,
                    created_at_ms INTEGER NOT NULL,
                    updated_at_ms INTEGER NOT NULL,
                    completed_at_ms INTEGER,
                    UNIQUE(repository_id, name),
                    UNIQUE(repository_id, branch_name)
                 );
                 CREATE TABLE child_reference (
                    id TEXT PRIMARY KEY,
                    task_id TEXT NOT NULL REFERENCES tasks(id)
                 );
                 INSERT INTO repositories VALUES (
                    'repo-v1', '/tmp/source', '/tmp/source/.git', 'source', 0, 1, 1
                 );
                 INSERT INTO tasks (
                    id, repository_id, name, goal, context_mode, context_json,
                    profile_json, phase, created_at_ms, updated_at_ms
                 ) VALUES (
                    'task-v1', 'repo-v1', 'legacy', 'legacy goal', 'fresh', '{}',
                    '{"name":"default","sourcePath":null,"sourceHash":"sha256:test","effectiveSettings":{}}',
                    'idle', 1, 1
                 );
                 INSERT INTO child_reference VALUES ('child-v1', 'task-v1');
                 PRAGMA user_version = 1;"#,
            )
            .unwrap();

        let store = Store::from_connection(connection).unwrap();
        let task = store.task_by_id("task-v1").unwrap().unwrap();
        assert_eq!(task.name, "legacy");
        assert!(serde_json::to_value(task).unwrap().get("goal").is_none());
        let connection = store.lock().unwrap();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 2);
        let legacy_goal: Option<String> = connection
            .query_row(
                "SELECT legacy_goal FROM tasks WHERE id = 'task-v1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(legacy_goal.as_deref(), Some("legacy goal"));
        connection
            .execute(
                "UPDATE tasks SET legacy_goal = NULL WHERE id = 'task-v1'",
                [],
            )
            .unwrap();
        let violations: i64 = connection
            .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(violations, 0);
    }

    #[test]
    fn state_and_events_change_atomically() {
        let store = Store::in_memory().unwrap();
        let repo = repository(Path::new("/tmp/source"));
        store.register_repository(&repo).unwrap();
        let (task, created) = store
            .create_task_with_event(
                new_task(&repo.id, "atomic"),
                EventDraft::task(
                    EventKind::TaskCreated,
                    EventSource::Coco,
                    json!({"version": 1}),
                ),
            )
            .unwrap();
        assert_eq!(task.phase, TaskPhase::Provisioning);
        assert_eq!(created.task_id.as_deref(), Some(task.id.as_str()));

        let (task, _) = store
            .transition_task_with_event(
                &task.id,
                TaskPhase::Provisioning,
                TaskPhase::Starting,
                None,
                EventDraft::task(EventKind::WorktreeCreated, EventSource::Git, json!({})),
            )
            .unwrap();
        let effective_profile = ProfileSnapshot {
            name: "effective".to_owned(),
            source_path: Some(PathBuf::from("/tmp/profile.toml")),
            source_hash: "sha256:effective".to_owned(),
            effective_settings: json!({"approvalPolicy": "on-request"}),
        };
        let task = store
            .update_task_profile(&task.id, &effective_profile)
            .unwrap();
        assert_eq!(task.profile, effective_profile);

        let (task, _) = store
            .bind_thread_with_event(
                &task.id,
                TaskPhase::Starting,
                TaskPhase::Idle,
                "thread-1",
                None,
                EventDraft::task(EventKind::AgentStarted, EventSource::Codex, json!({})),
            )
            .unwrap();
        assert_eq!(task.codex_thread_id.as_deref(), Some("thread-1"));
        assert_eq!(task.phase, TaskPhase::Idle);
        assert_eq!(
            store.task_by_thread_id("thread-1").unwrap().unwrap().id,
            task.id
        );

        let (task, turn, _) = store
            .start_turn_with_event(
                &task.id,
                &[TaskPhase::Idle],
                NewTurn {
                    operation_id: Some("turn-operation".to_owned()),
                    client_message_id: "client-message".to_owned(),
                    codex_turn_id: Some("codex-turn".to_owned()),
                    started_at_ms: Some(20),
                },
                EventDraft::task(EventKind::TurnStarted, EventSource::Codex, json!({})),
            )
            .unwrap();
        assert_eq!(task.phase, TaskPhase::Active);
        assert_eq!(task.active_turn_id.as_deref(), Some(turn.id.as_str()));
        assert_eq!(
            store
                .turn_by_operation_id("turn-operation")
                .unwrap()
                .unwrap()
                .id,
            turn.id
        );

        let (task, turn, _) = store
            .complete_turn_with_event(
                &task.id,
                &turn.id,
                TurnCompletion {
                    phase: TurnPhase::Completed,
                    error: None,
                    completed_at_ms: Some(30),
                },
                EventDraft::task(EventKind::TurnCompleted, EventSource::Codex, json!({})),
            )
            .unwrap();
        assert_eq!(task.phase, TaskPhase::Idle);
        assert_eq!(task.active_turn_id, None);
        assert_eq!(turn.phase, TurnPhase::Completed);
        assert_eq!(turn.completed_at_ms, Some(30));
        assert_eq!(store.events_after(Some(&task.id), 0).unwrap().len(), 5);
    }

    #[test]
    fn failed_compare_and_set_does_not_append_an_event() {
        let store = Store::in_memory().unwrap();
        let repo = repository(Path::new("/tmp/source-cas"));
        store.register_repository(&repo).unwrap();
        let (task, _) = store
            .create_task_with_event(
                new_task(&repo.id, "cas"),
                EventDraft::task(EventKind::TaskCreated, EventSource::Coco, json!({})),
            )
            .unwrap();

        assert!(matches!(
            store.transition_task_with_event(
                &task.id,
                TaskPhase::Idle,
                TaskPhase::Active,
                None,
                EventDraft::task(EventKind::TurnStarted, EventSource::Coco, json!({})),
            ),
            Err(StoreError::InvalidTaskTransition { .. })
        ));
        assert_eq!(store.events_after(Some(&task.id), 0).unwrap().len(), 1);
        assert_eq!(
            store.task_by_id(&task.id).unwrap().unwrap().phase,
            TaskPhase::Provisioning
        );
    }

    #[test]
    fn audit_round_trips_sanitized_metadata() {
        let store = Store::in_memory().unwrap();
        let audit = store
            .append_audit(AuditDraft {
                source: "mcp:test".to_owned(),
                action: "tasks.list".to_owned(),
                task_id: None,
                operation_id: None,
                outcome: AuditOutcome::Succeeded,
                details: json!({"count": 2}),
                occurred_at_ms: Some(42),
            })
            .unwrap();
        assert_eq!(audit.details, json!({"count": 2}));
        assert_eq!(store.audits_after(None, 0).unwrap(), [audit]);
    }

    #[test]
    fn restart_reconciliation_marks_inflight_state_interrupted() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("coco.sqlite3");
        let task_id;
        let turn_id;
        {
            let store = Store::open(&path).unwrap();
            let repo = repository(&temp.path().join("source"));
            store.register_repository(&repo).unwrap();
            let (task, _) = store
                .create_task_with_event(
                    new_task(&repo.id, "restart"),
                    EventDraft::task(EventKind::TaskCreated, EventSource::Coco, json!({})),
                )
                .unwrap();
            let (task, _) = store
                .transition_task_with_event(
                    &task.id,
                    TaskPhase::Provisioning,
                    TaskPhase::Starting,
                    None,
                    EventDraft::task(EventKind::WorktreeCreated, EventSource::Git, json!({})),
                )
                .unwrap();
            let (_, turn, _) = store
                .start_turn_with_event(
                    &task.id,
                    &[TaskPhase::Starting],
                    NewTurn {
                        operation_id: Some("restart-operation".to_owned()),
                        client_message_id: "restart-message".to_owned(),
                        codex_turn_id: Some("codex-restart-turn".to_owned()),
                        started_at_ms: None,
                    },
                    EventDraft::task(EventKind::TurnStarted, EventSource::Codex, json!({})),
                )
                .unwrap();
            task_id = task.id;
            turn_id = turn.id;
        }

        let store = Store::open(&path).unwrap();
        let reconciled = store.reconcile_unfinished().unwrap();
        assert_eq!(reconciled.len(), 1);
        assert_eq!(
            store.task_by_id(&task_id).unwrap().unwrap().phase,
            TaskPhase::Interrupted
        );
        assert_eq!(
            store.turn_by_id(&turn_id).unwrap().unwrap().phase,
            TurnPhase::Interrupted
        );

        #[cfg(unix)]
        {
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
            let parent_mode = fs::metadata(temp.path()).unwrap().permissions().mode() & 0o777;
            assert_eq!(parent_mode, 0o700);
        }
    }
}
