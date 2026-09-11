use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Instant;

use serde_json::json;
use tokio::sync::Mutex as AsyncMutex;
use tracing::error;

use crate::domain::{
    DecisionFileChange, EventKind, EventSource, Repository, Workspace, WorkspaceLifecycle,
};
use crate::git::{Git, GitRepository};
use crate::hooks::HookRegistry;
use crate::protocol::{
    RepositoryScope, RepositorySummary, WorkspaceCostEstimate, WorkspaceListItem, WorkspaceResult,
};
use crate::store::{EventDraft, Store, StoreError};

const MAX_OPERATION_ID_BYTES: usize = 256;

mod codex_events;
mod context;
mod decision;
mod error;
mod hooks;
mod jump;
mod recovery;
mod resources;
mod retirement;
mod signals;
mod turn;
mod usage;
mod worker;
mod workspace;

pub(crate) use error::{CoordinatorError, WorkspaceReferenceCandidate};
pub(crate) use worker::{
    LocatedNativeThread, NativeThread, StartedThread, StartedTurn, WorkerError,
    WorkerExecutionEnvironment, WorkerRuntime,
};

pub(crate) struct Coordinator {
    store: Arc<Store>,
    git: Git,
    worker: Arc<dyn WorkerRuntime>,
    worktrees_dir: PathBuf,
    codex_home: PathBuf,
    hooks: Arc<HookRegistry>,
    runtime_generation: String,
    repository_locks: AsyncMutex<HashMap<String, Arc<AsyncMutex<()>>>>,
    context_dependencies: AsyncMutex<()>,
    decisions: StdMutex<decision::DecisionRegistry>,
    subscribed_threads: StdMutex<HashSet<String>>,
    active_turn_operations: StdMutex<HashMap<String, turn::RuntimeTurnOperation>>,
    completed_turn_results: StdMutex<turn::TurnResultRegistry>,
    pending_compactions: StdMutex<HashMap<String, context::PendingCompaction>>,
    file_change_previews: StdMutex<HashMap<(String, String), Vec<DecisionFileChange>>>,
    jump_leases: StdMutex<jump::JumpLeaseRegistry>,
    usage_costs: StdMutex<HashMap<String, (Instant, WorkspaceCostEstimate)>>,
}

impl Coordinator {
    pub(crate) fn new(
        store: Arc<Store>,
        git: Git,
        worker: Arc<dyn WorkerRuntime>,
        worktrees_dir: PathBuf,
        codex_home: PathBuf,
        hooks: Arc<HookRegistry>,
        runtime_generation: String,
    ) -> Self {
        Self {
            store,
            git,
            worker,
            worktrees_dir,
            codex_home,
            hooks,
            runtime_generation,
            repository_locks: AsyncMutex::new(HashMap::new()),
            context_dependencies: AsyncMutex::new(()),
            decisions: StdMutex::new(decision::DecisionRegistry::default()),
            subscribed_threads: StdMutex::new(HashSet::new()),
            active_turn_operations: StdMutex::new(HashMap::new()),
            completed_turn_results: StdMutex::new(turn::TurnResultRegistry::default()),
            pending_compactions: StdMutex::new(HashMap::new()),
            file_change_previews: StdMutex::new(HashMap::new()),
            jump_leases: StdMutex::new(jump::JumpLeaseRegistry::default()),
            usage_costs: StdMutex::new(HashMap::new()),
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

    fn resolve_workspace(
        &self,
        scope: &RepositoryScope,
        reference: &str,
    ) -> Result<Workspace, CoordinatorError> {
        match scope {
            RepositoryScope::Repository { path } => {
                let (repository, _) = self.registered_repository_for_path(path)?;
                self.resolve_workspace_in_repository(&repository, reference)
            }
            RepositoryScope::AllRepositories => self.resolve_workspace_globally(reference),
        }
    }

    fn resolve_workspace_in_repository(
        &self,
        repository: &Repository,
        reference: &str,
    ) -> Result<Workspace, CoordinatorError> {
        if let Some(workspace) = self.store.workspace_by_id(reference)? {
            return if workspace.repository_id == repository.id {
                Ok(workspace)
            } else {
                Err(CoordinatorError::WorkspaceNotFound {
                    reference: reference.to_owned(),
                    candidates: Vec::new(),
                })
            };
        }
        if let Some(workspace) = self.store.workspace_by_name(&repository.id, reference)? {
            return Ok(workspace);
        }
        let candidates = self.reference_candidates(self.store.workspaces_by_name(reference)?)?;
        Err(CoordinatorError::WorkspaceNotFound {
            reference: reference.to_owned(),
            candidates,
        })
    }

    fn resolve_workspace_globally(&self, reference: &str) -> Result<Workspace, CoordinatorError> {
        if let Some(workspace) = self.store.workspace_by_id(reference)? {
            return Ok(workspace);
        }
        let mut matches = self.store.workspaces_by_name(reference)?;
        match matches.len() {
            0 => Err(CoordinatorError::WorkspaceNotFound {
                reference: reference.to_owned(),
                candidates: Vec::new(),
            }),
            1 => Ok(matches.pop().expect("one workspace match")),
            _ => Err(CoordinatorError::WorkspaceReferenceAmbiguous {
                reference: reference.to_owned(),
                candidates: self.reference_candidates(matches)?,
            }),
        }
    }

    fn reference_candidates(
        &self,
        workspaces: Vec<Workspace>,
    ) -> Result<Vec<WorkspaceReferenceCandidate>, CoordinatorError> {
        workspaces
            .into_iter()
            .take(8)
            .map(|workspace| {
                let repository = self.repository_by_id(&workspace.repository_id)?;
                Ok(WorkspaceReferenceCandidate {
                    workspace_id: workspace.id,
                    workspace_name: workspace.name,
                    repository_path: repository.root_path,
                })
            })
            .collect()
    }

    fn repository_by_id(&self, repository_id: &str) -> Result<Repository, CoordinatorError> {
        self.store
            .repository_by_id(repository_id)?
            .ok_or_else(|| StoreError::NotFound {
                entity: "repository",
                id: repository_id.to_owned(),
            })
            .map_err(CoordinatorError::from)
    }

    fn git_repository_for_workspace(
        &self,
        workspace: &Workspace,
    ) -> Result<(Repository, GitRepository), CoordinatorError> {
        let repository = self.repository_by_id(&workspace.repository_id)?;
        let discovered = self.git.discover(&repository.root_path)?;
        Ok((repository, discovered))
    }

    fn workspace_list_item(
        &self,
        workspace: Workspace,
        runtime_resources: Option<crate::domain::runtime::WorkspaceRuntimeResources>,
    ) -> Result<WorkspaceListItem, CoordinatorError> {
        let repository = self.repository_by_id(&workspace.repository_id)?;
        Ok(WorkspaceListItem {
            workspace,
            repository: RepositorySummary::from(&repository),
            runtime_resources,
        })
    }

    async fn repository_lock(&self, repository_id: &str) -> Arc<AsyncMutex<()>> {
        let mut locks = self.repository_locks.lock().await;
        Arc::clone(
            locks
                .entry(repository_id.to_owned())
                .or_insert_with(|| Arc::new(AsyncMutex::new(()))),
        )
    }

    fn has_thread_subscription(&self, thread_id: &str) -> bool {
        self.subscribed_threads
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .contains(thread_id)
    }

    fn mark_thread_subscribed(&self, thread_id: &str) {
        self.subscribed_threads
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(thread_id.to_owned());
    }

    fn mark_thread_unsubscribed(&self, thread_id: &str) {
        self.subscribed_threads
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(thread_id);
    }

    fn workspace_response(
        &self,
        mut workspace: Workspace,
    ) -> Result<WorkspaceResult, CoordinatorError> {
        if let Some(runtime) = self.runtime_turn_for_workspace(&workspace) {
            workspace.active_turn_id = Some(runtime.id.clone());
            return Ok(WorkspaceResult::with_operation(
                workspace,
                &runtime.id,
                runtime.native_result_id.as_deref(),
            ));
        }
        Ok(WorkspaceResult::prepared(workspace))
    }

    fn mark_workspace_failed(
        &self,
        workspace_id: &str,
        stage: &'static str,
        source_error: &CoordinatorError,
        source: EventSource,
    ) {
        let message = source_error.to_string();
        let code = source_error.code();
        if let Err(store_error) = self.store.transition_workspace_lifecycle_from_with_event(
            workspace_id,
            &[
                WorkspaceLifecycle::Provisioning,
                WorkspaceLifecycle::Starting,
            ],
            WorkspaceLifecycle::Failed,
            Some((code, &message)),
            EventDraft::workspace(
                EventKind::AgentFailed,
                source,
                json!({"stage": stage, "code": code, "message": message}),
            ),
        ) {
            error!(workspace_id, stage, %store_error, "could not persist workspace failure");
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
