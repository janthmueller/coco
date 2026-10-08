use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags, params};
use tokio::task::JoinHandle;
use tokio::time::{sleep, timeout};

use crate::diagnostics::{MAX_REPOSITORIES, MAX_WORKSPACES, PROBE_TIMEOUT, ProbeError};
use crate::domain::{Repository, WorkspaceAvailability, WorkspaceLifecycle, WorktreeMode};

use super::{Store, rows::map_repository};

pub(crate) struct DiagnosticWorkspaceBinding {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) repository_path: PathBuf,
    pub(crate) git_common_dir: PathBuf,
    pub(crate) availability: WorkspaceAvailability,
    pub(crate) lifecycle: WorkspaceLifecycle,
    pub(crate) worktree_mode: WorktreeMode,
    pub(crate) branch_name: Option<String>,
    pub(crate) worktree_path: Option<PathBuf>,
    pub(crate) thread_id: Option<String>,
    pub(crate) thread_archived: bool,
}

pub(crate) struct DiagnosticSnapshot {
    pub(crate) schema_version: i64,
    pub(crate) integrity_ok: bool,
    pub(crate) repositories: Vec<Repository>,
    pub(crate) workspaces: Vec<DiagnosticWorkspaceBinding>,
    pub(crate) repository_count: usize,
    pub(crate) workspace_count: usize,
}

impl Store {
    pub(crate) async fn diagnostic_snapshot(
        self: &Arc<Self>,
    ) -> Result<DiagnosticSnapshot, ProbeError> {
        let store = Arc::clone(self);
        let runtime = tokio::runtime::Handle::current();
        let task = tokio::task::spawn_blocking(move || {
            let path = store.diagnostic_path.clone();
            let Some(path) = path else {
                // Only the in-memory test adapter uses this branch. Production
                // probes never interrupt or change the daemon's connection.
                let guard = store
                    .connection
                    .try_lock()
                    .map_err(|_| ProbeError::Unavailable)?;
                return snapshot(&guard).map_err(|_| ProbeError::Failed);
            };
            let connection = Connection::open_with_flags(
                path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
            )
            .map_err(|_| ProbeError::Unavailable)?;
            connection
                .busy_timeout(Duration::from_millis(100))
                .map_err(|_| ProbeError::Failed)?;
            let interrupt = connection.get_interrupt_handle();
            let _timer = InterruptTimer(runtime.spawn(async move {
                sleep(PROBE_TIMEOUT).await;
                interrupt.interrupt();
            }));
            snapshot(&connection).map_err(|_| ProbeError::Failed)
        });
        timeout(PROBE_TIMEOUT, task)
            .await
            .map_err(|_| ProbeError::TimedOut)?
            .map_err(|_| ProbeError::Failed)?
    }
}

struct InterruptTimer(JoinHandle<()>);

impl Drop for InterruptTimer {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn snapshot(connection: &Connection) -> rusqlite::Result<DiagnosticSnapshot> {
    let transaction = connection.unchecked_transaction()?;
    let schema_version = transaction.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let integrity: String = transaction.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
    let mut foreign_keys = transaction.prepare("PRAGMA foreign_key_check")?;
    let integrity_ok = integrity == "ok" && foreign_keys.query([])?.next()?.is_none();
    drop(foreign_keys);
    let repository_count: usize = transaction.query_row(
        "SELECT count(*) FROM repositories WHERE is_registered = 1",
        [],
        map_count,
    )?;
    let workspace_count: usize =
        transaction.query_row("SELECT count(*) FROM workspaces", [], map_count)?;
    let repositories = transaction
        .prepare(
            "SELECT id, root_path, git_common_dir, display_name, is_linked_worktree,
            created_at_ms, updated_at_ms FROM repositories WHERE is_registered = 1
         ORDER BY root_path, id LIMIT ?1",
        )?
        .query_map([MAX_REPOSITORIES as i64], map_repository)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let workspaces = transaction
        .prepare(
            "SELECT w.id, w.name, w.repository_id, r.root_path, r.git_common_dir,
            w.availability, w.lifecycle, w.worktree_mode, w.branch_name,
            w.worktree_path, w.codex_thread_id, w.thread_archived
         FROM workspaces w JOIN repositories r ON r.id = w.repository_id
         ORDER BY r.root_path, w.name, w.id LIMIT ?1",
        )?
        .query_map(params![MAX_WORKSPACES as i64], map_binding)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    transaction.commit()?;
    Ok(DiagnosticSnapshot {
        schema_version,
        integrity_ok,
        repositories,
        workspaces,
        repository_count,
        workspace_count,
    })
}

fn map_binding(row: &rusqlite::Row<'_>) -> rusqlite::Result<DiagnosticWorkspaceBinding> {
    Ok(DiagnosticWorkspaceBinding {
        id: row.get(0)?,
        name: row.get(1)?,
        repository_path: PathBuf::from(row.get::<_, String>(3)?),
        git_common_dir: PathBuf::from(row.get::<_, String>(4)?),
        availability: parse(row, 5, WorkspaceAvailability::parse)?,
        lifecycle: parse(row, 6, WorkspaceLifecycle::parse)?,
        worktree_mode: parse(row, 7, WorktreeMode::parse)?,
        branch_name: row.get(8)?,
        worktree_path: row.get::<_, Option<String>>(9)?.map(PathBuf::from),
        thread_id: row.get(10)?,
        thread_archived: row.get(11)?,
    })
}

fn parse<T>(
    row: &rusqlite::Row<'_>,
    index: usize,
    parse: impl FnOnce(&str) -> Option<T>,
) -> rusqlite::Result<T> {
    let value: String = row.get(index)?;
    parse(&value).ok_or(rusqlite::Error::InvalidQuery)
}

fn map_count(row: &rusqlite::Row<'_>) -> rusqlite::Result<usize> {
    usize::try_from(row.get::<_, i64>(0)?).map_err(|_| rusqlite::Error::InvalidQuery)
}

#[cfg(test)]
mod tests;
