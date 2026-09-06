use std::collections::HashSet;
use std::sync::Mutex as StdMutex;

use serde_json::json;
use sha2::{Digest, Sha256};

use super::{Coordinator, CoordinatorError, validate_non_empty, validate_operation_id};
use crate::domain::{EventKind, EventSource, Turn, Workspace, WorkspacePhase};
use crate::protocol::{TurnStartParams, WorkspaceResult};
use crate::store::{EventDraft, NewTurn};

pub(super) struct PendingTurnGuard<'a> {
    pending: &'a StdMutex<HashSet<String>>,
    thread_id: String,
}

impl<'a> PendingTurnGuard<'a> {
    pub(super) fn new(pending: &'a StdMutex<HashSet<String>>, thread_id: &str) -> Self {
        pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(thread_id.to_owned());
        Self {
            pending,
            thread_id: thread_id.to_owned(),
        }
    }
}

impl Drop for PendingTurnGuard<'_> {
    fn drop(&mut self) {
        self.pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&self.thread_id);
    }
}

impl Coordinator {
    pub(crate) async fn start_turn(
        &self,
        params: TurnStartParams,
    ) -> Result<WorkspaceResult, CoordinatorError> {
        validate_non_empty("message", &params.message)?;
        validate_operation_id(&params.operation_id)?;
        let (repository, _) = self.registered_repository_for_path(&params.repository_path)?;
        let repository_lock = self.repository_lock(&repository.id).await;
        let _guard = repository_lock.lock().await;
        let workspace = self.resolve_workspace(&repository, &params.workspace)?;
        let client_message_id =
            message_fingerprint(&params.operation_id, &workspace.id, &params.message);
        if let Some(existing) = self.store.turn_by_operation_id(&params.operation_id)? {
            if existing.workspace_id != workspace.id
                || existing.client_message_id != client_message_id
            {
                return Err(CoordinatorError::IdempotencyConflict);
            }
            return Ok(workspace_and_turn_response(workspace, &existing));
        }
        if workspace.phase != WorkspacePhase::Idle {
            return Err(CoordinatorError::InvalidWorkspaceState {
                expected: "idle",
                actual: workspace.phase,
            });
        }
        let thread_id = workspace
            .codex_thread_id
            .as_deref()
            .ok_or(CoordinatorError::IncompleteWorkspace("Codex thread"))?;
        let worktree = workspace
            .worktree_path
            .as_deref()
            .ok_or(CoordinatorError::IncompleteWorkspace("worktree"))?;

        self.store.append_event(EventDraft {
            workspace_id: Some(workspace.id.clone()),
            turn_id: None,
            kind: EventKind::MessageReceived,
            source: EventSource::Coco,
            source_method: Some("turn.start".to_owned()),
            occurred_at_ms: None,
            payload: json!({
                "clientMessageId": client_message_id,
                "text": params.message,
            }),
        })?;
        let _pending_turn = PendingTurnGuard::new(&self.pending_turn_threads, thread_id);
        let started = self
            .worker
            .start_turn(thread_id, worktree, &client_message_id, &params.message)
            .await?;
        let (workspace, turn, _) = self.store.start_turn_with_event(
            &workspace.id,
            NewTurn {
                operation_id: Some(params.operation_id),
                client_message_id,
                codex_turn_id: Some(started.id.clone()),
                started_at_ms: None,
            },
            EventDraft::workspace(
                EventKind::TurnStarted,
                EventSource::Codex,
                json!({"codexTurnId": started.id}),
            ),
        )?;
        Ok(workspace_and_turn_response(workspace, &turn))
    }
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

fn workspace_and_turn_response(workspace: Workspace, turn: &Turn) -> WorkspaceResult {
    WorkspaceResult::with_turn(workspace, turn)
}
