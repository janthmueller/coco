use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde_json::json;

use super::events::insert_event;
use super::rows::{
    TASK_SELECT, TURN_SELECT, get_task_by_id, get_turn_by_id, map_task, map_turn, require_task,
    require_turn,
};
use super::{
    EventDraft, NewTask, NewTurn, Store, StoreError, TurnCompletion, json_to_sql_error, new_id,
    now_ms, path_text, sanitized_error_columns,
};
use crate::domain::{
    EventKind, EventSource, NormalizedEvent, ProfileSnapshot, Task, TaskPhase, Turn, TurnPhase,
};

impl Store {
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
