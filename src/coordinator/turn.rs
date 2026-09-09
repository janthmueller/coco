use serde_json::json;
use sha2::{Digest, Sha256};
use tracing::{error, warn};

use super::{Coordinator, CoordinatorError, validate_non_empty, validate_operation_id};
use crate::domain::{
    ContextMode, Workspace, WorkspaceAvailability, WorkspacePhase, derive_workspace_runtime,
};
use crate::protocol::{TurnStartParams, TurnTerminalStatus, WorkspaceResult};
use crate::store::{NewOperation, Operation, OperationKind, OperationState};

mod result;

pub(super) use result::TurnResultRegistry;
#[cfg(test)]
pub(super) use result::{MAX_TURN_RESPONSE_BYTES, bounded_turn_response};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RuntimeTurnOperation {
    pub(super) id: String,
    pub(super) operation_id: String,
    pub(super) workspace_id: String,
    pub(super) native_result_id: Option<String>,
    completed_before_response: Option<ObservedTurnCompletion>,
    agent_output: Option<ObservedAgentOutput>,
    pub(super) uncertain: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ObservedAgentOutput {
    native_turn_id: String,
    response: String,
    truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ObservedTurnCompletion {
    native_turn_id: String,
    status: TurnTerminalStatus,
}

struct PreparedTurnTarget {
    workspace: Workspace,
    thread_id: String,
    worktree: std::path::PathBuf,
    fresh: Option<super::workspace::PendingFreshThread>,
}

impl Coordinator {
    pub(crate) async fn start_turn(
        &self,
        params: TurnStartParams,
    ) -> Result<WorkspaceResult, CoordinatorError> {
        validate_non_empty("message", &params.message)?;
        validate_operation_id(&params.operation_id)?;
        let resolved = self.resolve_workspace(&params.scope, &params.workspace)?;
        let repository = self.repository_by_id(&resolved.repository_id)?;
        let repository_lock = self.repository_lock(&repository.id).await;
        let _guard = repository_lock.lock().await;
        let workspace = self.resolve_workspace(&params.scope, &params.workspace)?;
        require_open_workspace(&workspace)?;
        let request_fingerprint =
            message_fingerprint(&params.operation_id, &workspace.id, &params.message);

        if let Some(response) = self
            .replay_existing_turn_operation(
                workspace.clone(),
                &params.operation_id,
                &request_fingerprint,
            )
            .await?
        {
            return Ok(response);
        }

        self.reject_pending_adoption(&workspace.id)?;
        self.reject_runtime_turn_conflict(&workspace)?;
        let target = self.prepare_turn_target(workspace).await?;
        let PreparedTurnTarget {
            workspace,
            thread_id,
            worktree,
            fresh,
        } = target;

        let (operation, _) = self.store.prepare_operation(NewOperation {
            operation_id: params.operation_id.clone(),
            workspace_id: workspace.id.clone(),
            kind: OperationKind::TurnStart,
            request_fingerprint: request_fingerprint.clone(),
        })?;
        ensure_operation_replay_matches(&operation, &workspace.id, &request_fingerprint)?;
        if operation.state != OperationState::Prepared {
            return match operation.state {
                OperationState::Accepted => Ok(operation_response(workspace, &operation)),
                OperationState::Dispatching | OperationState::Uncertain => {
                    Err(CoordinatorError::OperationUncertain {
                        operation_id: params.operation_id,
                    })
                }
                OperationState::Prepared => unreachable!(),
            };
        }

        let operation = self.store.begin_operation_dispatch(&operation.id)?;
        self.track_runtime_turn(&thread_id, &workspace.id, &operation);
        let additional_context = workspace_transition_context(&workspace);
        let started = self
            .worker
            .start_turn(
                &thread_id,
                &worktree,
                &request_fingerprint,
                &params.message,
                additional_context,
            )
            .await;

        let started = match started {
            Ok(started) => started,
            Err(source) => {
                if let Some(pending) = fresh.as_ref() {
                    self.preserve_ambiguous_fresh_thread(&workspace, pending)
                        .await;
                }
                if let Err(store_error) = self.store.mark_operation_uncertain(&operation.id) {
                    error!(
                        operation_id = %operation.operation_id,
                        %store_error,
                        "could not mark an unconfirmed turn dispatch uncertain"
                    );
                }
                self.mark_runtime_turn_uncertain(&thread_id, &operation.id);
                warn!(
                    operation_id = %operation.operation_id,
                    %source,
                    "Codex did not confirm whether the turn was accepted"
                );
                return Err(CoordinatorError::OperationUncertain {
                    operation_id: params.operation_id,
                });
            }
        };

        let (workspace, operation) =
            match self.accept_started_turn(workspace, fresh.as_ref(), &operation, &started.id) {
                Ok(accepted) => accepted,
                Err(source) => {
                    self.mark_runtime_turn_uncertain(&thread_id, &operation.id);
                    return Err(source);
                }
            };
        self.bind_runtime_turn_result(&thread_id, &operation.id, &started.id);
        Ok(operation_response(
            self.project_current_runtime_turn(workspace),
            &operation,
        ))
    }

    fn accept_started_turn(
        &self,
        workspace: Workspace,
        fresh: Option<&super::workspace::PendingFreshThread>,
        operation: &Operation,
        native_turn_id: &str,
    ) -> Result<(Workspace, Operation), CoordinatorError> {
        match fresh {
            Some(pending) => {
                self.accept_fresh_thread_turn(&workspace, pending, &operation.id, native_turn_id)
            }
            None => self
                .store
                .accept_operation(&operation.id, native_turn_id)
                .map(|operation| (workspace, operation))
                .map_err(CoordinatorError::from),
        }
    }

    async fn prepare_turn_target(
        &self,
        workspace: Workspace,
    ) -> Result<PreparedTurnTarget, CoordinatorError> {
        if workspace.codex_thread_id.is_none() && workspace.context_mode == ContextMode::Fresh {
            if workspace.phase != WorkspacePhase::Prepared {
                return Err(CoordinatorError::InvalidWorkspaceState {
                    expected: "prepared",
                    actual: workspace.phase,
                });
            }
            let worktree = workspace
                .worktree_path
                .clone()
                .ok_or(CoordinatorError::IncompleteWorkspace("worktree"))?;
            let fresh = self.start_fresh_workspace_thread(&workspace).await?;
            return Ok(PreparedTurnTarget {
                workspace,
                thread_id: fresh.started.id.clone(),
                worktree,
                fresh: Some(fresh),
            });
        }
        let workspace = self.ensure_workspace_thread_loaded(workspace).await?;
        let (thread_id, worktree) = {
            let (thread_id, worktree) = self.validated_turn_target(&workspace)?;
            (thread_id.to_owned(), worktree.to_owned())
        };
        Ok(PreparedTurnTarget {
            workspace,
            thread_id,
            worktree,
            fresh: None,
        })
    }

    async fn preserve_ambiguous_fresh_thread(
        &self,
        workspace: &Workspace,
        pending: &super::workspace::PendingFreshThread,
    ) {
        let Some(worktree) = workspace.worktree_path.as_deref() else {
            return;
        };
        let materialized = self
            .worker
            .find_materialized_thread(&pending.started.id, worktree)
            .await;
        let Ok(Some(native)) = materialized else {
            return;
        };
        if native.id != pending.started.id
            || native.cwd != worktree
            || native.forked_from_id.is_some()
        {
            warn!(
                workspace_id = %workspace.id,
                thread_id = %pending.started.id,
                "refusing an invalid native thread after an ambiguous first turn"
            );
            return;
        }
        if let Err(error) = self.bind_materialized_fresh_thread(workspace, pending) {
            warn!(
                workspace_id = %workspace.id,
                thread_id = %pending.started.id,
                %error,
                "could not preserve a materialized thread after an ambiguous first turn"
            );
        }
    }

    async fn replay_existing_turn_operation(
        &self,
        workspace: Workspace,
        operation_id: &str,
        request_fingerprint: &str,
    ) -> Result<Option<WorkspaceResult>, CoordinatorError> {
        let Some(existing) = self.store.operation_by_client_id(operation_id)? else {
            return Ok(None);
        };
        ensure_operation_replay_matches(&existing, &workspace.id, request_fingerprint)?;
        match existing.state {
            OperationState::Accepted => {
                let workspace = self.hydrate_native_thread_runtime(workspace).await;
                Ok(Some(operation_response(workspace, &existing)))
            }
            OperationState::Dispatching | OperationState::Uncertain => {
                Err(CoordinatorError::OperationUncertain {
                    operation_id: operation_id.to_owned(),
                })
            }
            OperationState::Prepared => Ok(None),
        }
    }

    fn validated_turn_target<'a>(
        &self,
        workspace: &'a Workspace,
    ) -> Result<(&'a str, &'a std::path::Path), CoordinatorError> {
        if workspace.phase != WorkspacePhase::Idle {
            return Err(CoordinatorError::InvalidWorkspaceState {
                expected: "idle",
                actual: workspace.phase,
            });
        }
        self.reject_runtime_turn_conflict(workspace)?;
        let thread_id = workspace
            .codex_thread_id
            .as_deref()
            .ok_or(CoordinatorError::IncompleteWorkspace("Codex thread"))?;
        let worktree = workspace
            .worktree_path
            .as_deref()
            .ok_or(CoordinatorError::IncompleteWorkspace("worktree"))?;
        Ok((thread_id, worktree))
    }

    pub(super) fn runtime_turn_for_workspace(
        &self,
        workspace: &Workspace,
    ) -> Option<RuntimeTurnOperation> {
        let active = self
            .active_turn_operations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        workspace
            .codex_thread_id
            .as_deref()
            .and_then(|thread_id| active.get(thread_id))
            .or_else(|| {
                active
                    .values()
                    .find(|runtime| runtime.workspace_id == workspace.id)
            })
            .cloned()
    }

    pub(super) fn runtime_workspace_id_for_thread(&self, thread_id: &str) -> Option<String> {
        self.active_turn_operations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(thread_id)
            .map(|runtime| runtime.workspace_id.clone())
    }

    pub(super) fn runtime_thread_matches_workspace(
        &self,
        thread_id: &str,
        workspace_id: &str,
    ) -> bool {
        self.runtime_workspace_id_for_thread(thread_id).as_deref() == Some(workspace_id)
    }

    pub(super) fn project_current_runtime_turn(&self, mut workspace: Workspace) -> Workspace {
        let Some(runtime) = self.runtime_turn_for_workspace(&workspace) else {
            workspace.active_turn_id = None;
            return workspace;
        };
        workspace.active_turn_id = Some(runtime.id);
        if runtime.uncertain {
            workspace.phase = WorkspacePhase::Unavailable;
            workspace.wait_reasons.clear();
        } else if workspace.thread_runtime.is_some() {
            (workspace.phase, workspace.wait_reasons) = derive_workspace_runtime(
                workspace.lifecycle,
                workspace.availability,
                workspace.thread_runtime.as_ref(),
                true,
                workspace.codex_thread_id.is_some(),
            );
        } else {
            workspace.phase = WorkspacePhase::Active;
            workspace.wait_reasons.clear();
        }
        workspace
    }

    pub(super) fn clear_runtime_turns(&self) -> usize {
        let mut active = self
            .active_turn_operations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let count = active.len();
        active.clear();
        count
    }

    fn reject_runtime_turn_conflict(&self, workspace: &Workspace) -> Result<(), CoordinatorError> {
        if let Some(runtime) = self.runtime_turn_for_workspace(workspace) {
            if runtime.uncertain {
                return Err(CoordinatorError::OperationUncertain {
                    operation_id: runtime.operation_id,
                });
            }
            return Err(CoordinatorError::InvalidWorkspaceState {
                expected: "idle",
                actual: WorkspacePhase::Active,
            });
        }
        if workspace.active_turn_id.is_some() {
            return Err(CoordinatorError::InvalidWorkspaceState {
                expected: "idle",
                actual: WorkspacePhase::Active,
            });
        }
        Ok(())
    }

    fn track_runtime_turn(&self, thread_id: &str, workspace_id: &str, operation: &Operation) {
        let previous = self
            .active_turn_operations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(
                thread_id.to_owned(),
                RuntimeTurnOperation {
                    id: operation.id.clone(),
                    operation_id: operation.operation_id.clone(),
                    workspace_id: workspace_id.to_owned(),
                    native_result_id: operation.native_result_id.clone(),
                    completed_before_response: None,
                    agent_output: None,
                    uncertain: false,
                },
            );
        debug_assert!(previous.is_none(), "runtime turn guard was replaced");
    }

    fn mark_runtime_turn_uncertain(&self, thread_id: &str, operation_id: &str) {
        let mut active = self
            .active_turn_operations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(runtime) = active.get_mut(thread_id)
            && runtime.id == operation_id
        {
            runtime.uncertain = true;
        }
    }
}

fn require_open_workspace(workspace: &Workspace) -> Result<(), CoordinatorError> {
    if workspace.availability == WorkspaceAvailability::Open {
        Ok(())
    } else {
        Err(CoordinatorError::InvalidWorkspaceState {
            expected: "open",
            actual: workspace.phase,
        })
    }
}

fn ensure_operation_replay_matches(
    operation: &Operation,
    workspace_id: &str,
    request_fingerprint: &str,
) -> Result<(), CoordinatorError> {
    if operation.kind != OperationKind::TurnStart
        || operation.workspace_id != workspace_id
        || operation.request_fingerprint != request_fingerprint
    {
        return Err(CoordinatorError::IdempotencyConflict);
    }
    Ok(())
}

fn workspace_transition_context(workspace: &Workspace) -> Option<serde_json::Value> {
    (workspace.context_mode == crate::domain::ContextMode::Fork).then(|| {
        let value = json!({
            "message": "CoCo forked this conversation into a different Git workspace. Work only in the destination binding below.",
            "workspaceId": workspace.id,
            "workspaceName": workspace.name,
            "worktreePath": workspace.worktree_path,
            "branchName": workspace.branch_name,
            "baseSha": workspace.base_sha,
            "sourceWorkspaceId": workspace
                .context
                .pointer("/resolved/context/source/workspaceId"),
            "sourceWorkspaceName": workspace
                .context
                .pointer("/resolved/context/source/workspaceName"),
            "sourceThreadId": workspace.parent_thread_id,
        })
        .to_string();
        json!({
            "coco.workspace-binding": {
                "kind": "application",
                "value": value,
            }
        })
    })
}

fn message_fingerprint(operation_id: &str, workspace_id: &str, message: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(operation_id.as_bytes());
    digest.update([0]);
    digest.update(workspace_id.as_bytes());
    digest.update([0]);
    digest.update(message.as_bytes());
    format!("coco-{}", hex::encode(digest.finalize()))
}

fn operation_response(workspace: Workspace, operation: &Operation) -> WorkspaceResult {
    WorkspaceResult::with_operation(
        workspace,
        &operation.id,
        operation.native_result_id.as_deref(),
    )
}
