use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use chrono::Utc;
use rusqlite::{Connection, OpenFlags, TransactionBehavior, params};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{
    AuditOutcome, ContextMode, EventKind, EventSource, ProfileSnapshot, Repository, WorktreeMode,
};
#[cfg(test)]
use crate::domain::{Decision, DecisionKind, DecisionPrompt, TurnPhase};

#[cfg(test)]
mod decisions;
mod events;
mod migrations;
mod operations;
mod rows;
mod workspaces;

use migrations::migrate;
use rows::{
    get_repository_by_common_dir, get_repository_by_id, get_repository_by_root, map_repository,
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
    #[error(
        "invalid transition for workspace {workspace_id}: expected {expected}, observed {actual}"
    )]
    InvalidWorkspaceTransition {
        workspace_id: String,
        expected: String,
        actual: String,
    },
    #[cfg(test)]
    #[error("turn {turn_id} cannot complete with phase {phase}")]
    InvalidTurnCompletion { turn_id: String, phase: String },
    #[error("operation {operation_id} must be {expected}, but is {actual}")]
    InvalidOperationState {
        operation_id: String,
        expected: &'static str,
        actual: String,
    },
    #[error("operation {operation_id} already resolved to another native result")]
    OperationResultConflict { operation_id: String },
    #[error(
        "event turn {turn_id} belongs to workspace {turn_workspace_id}, not {event_workspace_id}"
    )]
    EventCorrelation {
        turn_id: String,
        turn_workspace_id: String,
        event_workspace_id: String,
    },
    #[error("decision {decision_id} must be pending, but is {actual}")]
    InvalidDecisionState { decision_id: String, actual: String },
    #[error("decision {decision_id} belongs to another App Server generation")]
    DecisionGenerationMismatch { decision_id: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewWorkspace {
    pub create_operation_id: Option<String>,
    pub repository_id: String,
    pub name: String,
    pub context_mode: ContextMode,
    pub context: Value,
    pub profile: ProfileSnapshot,
    pub worktree_mode: WorktreeMode,
    pub branch_name: Option<String>,
    pub base_sha: Option<String>,
    pub worktree_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventDraft {
    pub workspace_id: Option<String>,
    pub turn_id: Option<String>,
    pub kind: EventKind,
    pub source: EventSource,
    pub source_method: Option<String>,
    pub occurred_at_ms: Option<i64>,
    pub payload: Value,
}

impl EventDraft {
    pub fn workspace(kind: EventKind, source: EventSource, payload: Value) -> Self {
        Self {
            workspace_id: None,
            turn_id: None,
            kind,
            source,
            source_method: None,
            occurred_at_ms: None,
            payload,
        }
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTurn {
    pub operation_id: Option<String>,
    pub client_message_id: String,
    pub codex_turn_id: Option<String>,
    pub started_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationKind {
    TurnStart,
}

impl OperationKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TurnStart => "turn_start",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "turn_start" => Some(Self::TurnStart),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationState {
    Prepared,
    Dispatching,
    Accepted,
    Uncertain,
}

impl OperationState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::Dispatching => "dispatching",
            Self::Accepted => "accepted",
            Self::Uncertain => "uncertain",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "prepared" => Some(Self::Prepared),
            "dispatching" => Some(Self::Dispatching),
            "accepted" => Some(Self::Accepted),
            "uncertain" => Some(Self::Uncertain),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operation {
    pub id: String,
    pub operation_id: String,
    pub workspace_id: String,
    pub kind: OperationKind,
    pub request_fingerprint: String,
    pub native_result_id: Option<String>,
    pub state: OperationState,
    pub created_at_ms: i64,
    pub dispatch_started_at_ms: Option<i64>,
    pub result_recorded_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewOperation {
    pub operation_id: String,
    pub workspace_id: String,
    pub kind: OperationKind,
    pub request_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewThreadBinding {
    pub thread_id: String,
    pub parent_thread_id: Option<String>,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub struct TurnCompletion {
    pub phase: TurnPhase,
    pub error: Option<Value>,
    pub completed_at_ms: Option<i64>,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub struct NewDecision {
    pub workspace_id: String,
    pub turn_id: Option<String>,
    pub codex_thread_id: String,
    pub codex_turn_id: Option<String>,
    pub runtime_generation: String,
    pub native_request_id: Value,
    pub method: String,
    pub kind: DecisionKind,
    pub prompt: DecisionPrompt,
    pub native_options: Vec<Value>,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub struct StoredDecision {
    pub decision: Decision,
    pub codex_thread_id: String,
    pub codex_turn_id: Option<String>,
    pub runtime_generation: String,
    pub native_request_id: Value,
    pub method: String,
    pub native_options: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AuditDraft {
    pub source: String,
    pub action: String,
    pub workspace_id: Option<String>,
    pub operation_id: Option<String>,
    pub outcome: AuditOutcome,
    pub details: Value,
    pub occurred_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReconciliationSummary {
    pub failed_workspace_preparations: usize,
    pub uncertain_operations: usize,
    pub stale_thread_snapshots: usize,
}

impl ReconciliationSummary {
    pub const fn total(self) -> usize {
        self.failed_workspace_preparations + self.uncertain_operations + self.stale_thread_snapshots
    }
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

    #[cfg(test)]
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

    pub fn repository_by_common_dir(&self, path: &Path) -> Result<Option<Repository>, StoreError> {
        let connection = self.lock()?;
        get_repository_by_common_dir(&connection, path)
    }

    pub fn repository_by_id(&self, id: &str) -> Result<Option<Repository>, StoreError> {
        let connection = self.lock()?;
        get_repository_by_id(&connection, id)
    }

    pub fn list_repositories(&self) -> Result<Vec<Repository>, StoreError> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT id, root_path, git_common_dir, display_name, is_linked_worktree,
                created_at_ms, updated_at_ms FROM repositories
             ORDER BY display_name, root_path, id",
        )?;
        statement
            .query_map([], map_repository)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    fn lock(&self) -> Result<MutexGuard<'_, Connection>, StoreError> {
        self.connection.lock().map_err(|_| StoreError::Poisoned)
    }
}

fn json_to_sql_error(error: serde_json::Error) -> StoreError {
    StoreError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
}

#[cfg(test)]
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
mod tests;
