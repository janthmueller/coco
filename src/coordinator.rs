use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};

use serde_json::json;
use tokio::sync::Mutex as AsyncMutex;
use tracing::error;

use crate::domain::{EventKind, EventSource, Repository, Task, TaskLifecycle};
use crate::git::{Git, GitRepository};
use crate::protocol::TaskResult;
use crate::store::{EventDraft, Store};

const MAX_OPERATION_ID_BYTES: usize = 256;

mod codex_events;
mod error;
mod recovery;
mod task;
mod turn;
mod worker;

pub(crate) use error::CoordinatorError;
pub(crate) use worker::{StartedThread, StartedTurn, WorkerError, WorkerRuntime};

pub(crate) struct Coordinator {
    store: Arc<Store>,
    git: Git,
    worker: Arc<dyn WorkerRuntime>,
    worktrees_dir: PathBuf,
    codex_home: PathBuf,
    runtime_generation: String,
    repository_locks: AsyncMutex<HashMap<String, Arc<AsyncMutex<()>>>>,
    pending_turn_threads: StdMutex<HashSet<String>>,
}

impl Coordinator {
    pub(crate) fn new(
        store: Arc<Store>,
        git: Git,
        worker: Arc<dyn WorkerRuntime>,
        worktrees_dir: PathBuf,
        codex_home: PathBuf,
        runtime_generation: String,
    ) -> Self {
        Self {
            store,
            git,
            worker,
            worktrees_dir,
            codex_home,
            runtime_generation,
            repository_locks: AsyncMutex::new(HashMap::new()),
            pending_turn_threads: StdMutex::new(HashSet::new()),
        }
    }

    fn registered_repository_for_path(
        &self,
        path: &Path,
    ) -> Result<(Repository, GitRepository), CoordinatorError> {
        let discovered = self.git.discover(path)?;
        let repository = self
            .store
            .repository_by_common_dir(&discovered.git_common_dir)?
            .ok_or_else(|| {
                CoordinatorError::RepositoryNotRegistered(discovered.root_path.clone())
            })?;
        Ok((repository, discovered))
    }

    fn resolve_task(
        &self,
        repository: &Repository,
        reference: &str,
    ) -> Result<Task, CoordinatorError> {
        if let Some(task) = self.store.task_by_id(reference)? {
            return if task.repository_id == repository.id {
                Ok(task)
            } else {
                Err(CoordinatorError::TaskNotFound(reference.to_owned()))
            };
        }
        self.store
            .task_by_name(&repository.id, reference)?
            .ok_or_else(|| CoordinatorError::TaskNotFound(reference.to_owned()))
    }

    async fn repository_lock(&self, repository_id: &str) -> Arc<AsyncMutex<()>> {
        let mut locks = self.repository_locks.lock().await;
        Arc::clone(
            locks
                .entry(repository_id.to_owned())
                .or_insert_with(|| Arc::new(AsyncMutex::new(()))),
        )
    }

    fn task_response(&self, task: Task) -> Result<TaskResult, CoordinatorError> {
        let turn = task
            .active_turn_id
            .as_deref()
            .map(|turn_id| self.store.turn_by_id(turn_id))
            .transpose()?
            .flatten();
        match turn {
            Some(turn) => Ok(TaskResult::with_turn(task, &turn)),
            None => Ok(TaskResult::prepared(task)),
        }
    }

    fn mark_task_failed(
        &self,
        task_id: &str,
        stage: &'static str,
        source_error: &CoordinatorError,
        source: EventSource,
    ) {
        let message = source_error.to_string();
        let code = source_error.code();
        if let Err(store_error) = self.store.transition_task_lifecycle_from_with_event(
            task_id,
            &[TaskLifecycle::Provisioning, TaskLifecycle::Starting],
            TaskLifecycle::Failed,
            Some((code, &message)),
            EventDraft::task(
                EventKind::AgentFailed,
                source,
                json!({"stage": stage, "code": code, "message": message}),
            ),
        ) {
            error!(task_id, stage, %store_error, "could not persist task failure");
        }
    }
}

fn validate_non_empty(field: &str, value: &str) -> Result<(), CoordinatorError> {
    if value.trim().is_empty() {
        Err(CoordinatorError::InvalidParams(format!(
            "{field} must not be empty"
        )))
    } else {
        Ok(())
    }
}

fn validate_operation_id(operation_id: &str) -> Result<(), CoordinatorError> {
    if operation_id.is_empty() || operation_id.len() > MAX_OPERATION_ID_BYTES {
        return Err(CoordinatorError::InvalidParams(format!(
            "operationId must contain 1-{MAX_OPERATION_ID_BYTES} bytes"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
