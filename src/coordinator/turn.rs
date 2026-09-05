use std::collections::HashSet;
use std::sync::Mutex as StdMutex;

use serde_json::json;
use sha2::{Digest, Sha256};

use super::{Coordinator, CoordinatorError, validate_non_empty, validate_operation_id};
use crate::domain::{EventKind, EventSource, Task, TaskPhase, Turn};
use crate::protocol::{TaskResult, TurnStartParams};
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
    ) -> Result<TaskResult, CoordinatorError> {
        validate_non_empty("message", &params.message)?;
        validate_operation_id(&params.operation_id)?;
        let (repository, _) = self.registered_repository_for_path(&params.repository_path)?;
        let repository_lock = self.repository_lock(&repository.id).await;
        let _guard = repository_lock.lock().await;
        let task = self.resolve_task(&repository, &params.task)?;
        let client_message_id =
            message_fingerprint(&params.operation_id, &task.id, &params.message);
        if let Some(existing) = self.store.turn_by_operation_id(&params.operation_id)? {
            if existing.task_id != task.id || existing.client_message_id != client_message_id {
                return Err(CoordinatorError::IdempotencyConflict);
            }
            return Ok(task_and_turn_response(task, &existing));
        }
        if task.phase != TaskPhase::Idle {
            return Err(CoordinatorError::InvalidTaskState {
                expected: "idle",
                actual: task.phase,
            });
        }
        let thread_id = task
            .codex_thread_id
            .as_deref()
            .ok_or(CoordinatorError::IncompleteTask("Codex thread"))?;
        let worktree = task
            .worktree_path
            .as_deref()
            .ok_or(CoordinatorError::IncompleteTask("worktree"))?;

        self.store.append_event(EventDraft {
            task_id: Some(task.id.clone()),
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
        let (task, turn, _) = self.store.start_turn_with_event(
            &task.id,
            &[TaskPhase::Idle],
            NewTurn {
                operation_id: Some(params.operation_id),
                client_message_id,
                codex_turn_id: Some(started.id.clone()),
                started_at_ms: None,
            },
            EventDraft::task(
                EventKind::TurnStarted,
                EventSource::Codex,
                json!({"codexTurnId": started.id}),
            ),
        )?;
        Ok(task_and_turn_response(task, &turn))
    }
}

fn message_fingerprint(operation_id: &str, task_id: &str, message: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(operation_id.as_bytes());
    digest.update([0]);
    digest.update(task_id.as_bytes());
    digest.update([0]);
    digest.update(message.as_bytes());
    format!("coco-{}", hex::encode(digest.finalize()))
}

fn task_and_turn_response(task: Task, turn: &Turn) -> TaskResult {
    TaskResult::with_turn(task, turn)
}
