use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde_json::json;

use super::events::insert_event;
use super::operations::reconcile_unconfirmed_operations;
#[cfg(test)]
use super::rows::{TURN_SELECT, map_turn, require_turn};
use super::rows::{WORKSPACE_SELECT, get_workspace_by_id, map_workspace, require_workspace};
use super::{
    EventDraft, NewThreadBinding, NewWorkspace, ReconciliationSummary, Store, StoreError,
    WorkspaceDeletionIntent, json_to_sql_error, new_id, now_ms, path_text,
};
#[cfg(test)]
use super::{NewTurn, TurnCompletion, sanitized_error_columns};
#[cfg(test)]
use crate::domain::{CodexThreadStatus, Turn, TurnPhase};
use crate::domain::{
    EventKind, EventSource, NormalizedEvent, ProfileSnapshot, Workspace, WorkspaceAvailability,
    WorkspaceLifecycle,
};

impl Store {
    pub fn create_workspace_with_event(
        &self,
        input: NewWorkspace,
        mut event: EventDraft,
    ) -> Result<(Workspace, NormalizedEvent), StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let now = now_ms();
        let workspace_id = new_id();
        let profile_json = serde_json::to_string(&input.profile).map_err(json_to_sql_error)?;
        let context_json = serde_json::to_string(&input.context).map_err(json_to_sql_error)?;
        let worktree_path = input
            .worktree_path
            .as_deref()
            .map(path_text)
            .transpose()?
            .map(str::to_owned);
        transaction.execute(
            "INSERT INTO workspaces (
                id, create_operation_id, repository_id, name, context_mode,
                context_json, profile_json, lifecycle, thread_status_json,
                thread_status_generation, thread_status_observed_at_ms,
                thread_status_is_fresh, worktree_mode, branch_name, base_sha, worktree_path,
                codex_thread_id, parent_thread_id, active_turn_id, last_error_code,
                last_error_message, created_at_ms, updated_at_ms, completed_at_ms
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, 'provisioning', NULL, NULL, NULL, 0,
                ?8, ?9, ?10, ?11, NULL, NULL, NULL, NULL, NULL, ?12, ?12, NULL
             )",
            params![
                workspace_id,
                input.create_operation_id,
                input.repository_id,
                input.name,
                input.context_mode.as_str(),
                context_json,
                profile_json,
                input.worktree_mode.as_str(),
                input.branch_name,
                input.base_sha,
                worktree_path,
                now,
            ],
        )?;
        event.workspace_id = Some(workspace_id.clone());
        let event = insert_event(&transaction, event)?;
        let workspace = get_workspace_by_id(&transaction, &workspace_id)?.ok_or_else(|| {
            StoreError::NotFound {
                entity: "workspace",
                id: workspace_id.clone(),
            }
        })?;
        transaction.commit()?;
        Ok((workspace, event))
    }

    pub fn transition_workspace_lifecycle_with_event(
        &self,
        workspace_id: &str,
        expected: WorkspaceLifecycle,
        next: WorkspaceLifecycle,
        last_error: Option<(&str, &str)>,
        event: EventDraft,
    ) -> Result<(Workspace, NormalizedEvent), StoreError> {
        self.transition_workspace_lifecycle_from_with_event(
            workspace_id,
            &[expected],
            next,
            last_error,
            event,
        )
    }

    pub fn transition_workspace_lifecycle_from_with_event(
        &self,
        workspace_id: &str,
        expected: &[WorkspaceLifecycle],
        next: WorkspaceLifecycle,
        last_error: Option<(&str, &str)>,
        mut event: EventDraft,
    ) -> Result<(Workspace, NormalizedEvent), StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        assert_workspace_lifecycle(&transaction, workspace_id, expected)?;
        let now = now_ms();
        let (error_code, error_message) = last_error
            .map(|(code, message)| (Some(code), Some(message)))
            .unwrap_or((None, None));
        let completed_at = (next == WorkspaceLifecycle::Completed).then_some(now);
        transaction.execute(
            "UPDATE workspaces SET lifecycle = ?1, last_error_code = ?2, last_error_message = ?3,
                completed_at_ms = ?4, updated_at_ms = ?5 WHERE id = ?6",
            params![
                next.as_str(),
                error_code,
                error_message,
                completed_at,
                now,
                workspace_id
            ],
        )?;
        event.workspace_id = Some(workspace_id.to_owned());
        let event = insert_event(&transaction, event)?;
        let workspace = require_workspace(&transaction, workspace_id)?;
        transaction.commit()?;
        Ok((workspace, event))
    }

    pub fn bind_thread_with_event(
        &self,
        workspace_id: &str,
        expected: WorkspaceLifecycle,
        next: WorkspaceLifecycle,
        binding: NewThreadBinding,
        mut event: EventDraft,
    ) -> Result<(Workspace, NormalizedEvent), StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        assert_workspace_lifecycle(&transaction, workspace_id, &[expected])?;
        let current = require_workspace(&transaction, workspace_id)?;
        if current.codex_thread_id.is_some() {
            return Err(StoreError::InvalidWorkspaceTransition {
                workspace_id: workspace_id.to_owned(),
                expected: "no bound Codex thread".to_owned(),
                actual: "bound Codex thread".to_owned(),
            });
        }
        let now = now_ms();
        transaction.execute(
            "UPDATE workspaces SET codex_thread_id = ?1, parent_thread_id = ?2,
                lifecycle = ?3, thread_status_json = NULL,
                thread_status_generation = NULL, thread_status_observed_at_ms = NULL,
                thread_status_is_fresh = 0, updated_at_ms = ?4 WHERE id = ?5",
            params![
                binding.thread_id,
                binding.parent_thread_id,
                next.as_str(),
                now,
                workspace_id
            ],
        )?;
        event.workspace_id = Some(workspace_id.to_owned());
        let event = insert_event(&transaction, event)?;
        let workspace = require_workspace(&transaction, workspace_id)?;
        transaction.commit()?;
        Ok((workspace, event))
    }

    #[cfg(test)]
    pub fn start_turn_with_event(
        &self,
        workspace_id: &str,
        input: NewTurn,
        mut event: EventDraft,
    ) -> Result<(Workspace, Turn, NormalizedEvent), StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        assert_workspace_lifecycle(&transaction, workspace_id, &[WorkspaceLifecycle::Ready])?;
        let current = require_workspace(&transaction, workspace_id)?;
        if current.active_turn_id.is_some() {
            return Err(StoreError::InvalidWorkspaceTransition {
                workspace_id: workspace_id.to_owned(),
                expected: "no active turn".to_owned(),
                actual: "active turn".to_owned(),
            });
        }
        let now = now_ms();
        let turn_id = new_id();
        let turn_phase = if input.codex_turn_id.is_some() {
            TurnPhase::InProgress
        } else {
            TurnPhase::Starting
        };
        transaction.execute(
            "INSERT INTO turns (
                id, workspace_id, operation_id, client_message_id, codex_turn_id, phase,
                requested_at_ms, started_at_ms, completed_at_ms, error_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, NULL)",
            params![
                turn_id,
                workspace_id,
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
            "UPDATE workspaces SET active_turn_id = ?1, updated_at_ms = ?2 WHERE id = ?3",
            params![turn_id, now, workspace_id],
        )?;
        event.workspace_id = Some(workspace_id.to_owned());
        event.turn_id = Some(turn_id.clone());
        let event = insert_event(&transaction, event)?;
        let workspace = require_workspace(&transaction, workspace_id)?;
        let turn = require_turn(&transaction, &turn_id)?;
        transaction.commit()?;
        Ok((workspace, turn, event))
    }

    #[cfg(test)]
    pub fn complete_turn_with_event(
        &self,
        workspace_id: &str,
        turn_id: &str,
        completion: TurnCompletion,
        mut event: EventDraft,
    ) -> Result<(Workspace, Turn, NormalizedEvent), StoreError> {
        match completion.phase {
            TurnPhase::Completed | TurnPhase::Failed | TurnPhase::Interrupted => {}
            phase => {
                return Err(StoreError::InvalidTurnCompletion {
                    turn_id: turn_id.to_owned(),
                    phase: phase.as_str().to_owned(),
                });
            }
        };
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        assert_workspace_lifecycle(&transaction, workspace_id, &[WorkspaceLifecycle::Ready])?;
        let current = require_workspace(&transaction, workspace_id)?;
        if current.active_turn_id.as_deref() != Some(turn_id) {
            return Err(StoreError::InvalidWorkspaceTransition {
                workspace_id: workspace_id.to_owned(),
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
             WHERE id = ?4 AND workspace_id = ?5 AND phase IN ('starting', 'in_progress')",
            params![
                completion.phase.as_str(),
                now,
                error_json,
                turn_id,
                workspace_id
            ],
        )?;
        if updated != 1 {
            return Err(StoreError::InvalidWorkspaceTransition {
                workspace_id: workspace_id.to_owned(),
                expected: format!("unfinished turn {turn_id}"),
                actual: "turn missing or already terminal".to_owned(),
            });
        }
        let (error_code, error_message) = sanitized_error_columns(completion.error.as_ref());
        transaction.execute(
            "UPDATE workspaces SET active_turn_id = NULL, last_error_code = ?1,
                last_error_message = ?2, updated_at_ms = ?3 WHERE id = ?4",
            params![error_code, error_message, now, workspace_id],
        )?;
        event.workspace_id = Some(workspace_id.to_owned());
        event.turn_id = Some(turn_id.to_owned());
        let event = insert_event(&transaction, event)?;
        let workspace = require_workspace(&transaction, workspace_id)?;
        let turn = require_turn(&transaction, turn_id)?;
        transaction.commit()?;
        Ok((workspace, turn, event))
    }

    #[cfg(test)]
    pub fn observe_thread_status_with_event(
        &self,
        workspace_id: &str,
        status: CodexThreadStatus,
        runtime_generation: &str,
        mut event: EventDraft,
    ) -> Result<(Workspace, NormalizedEvent), StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = require_workspace(&transaction, workspace_id)?;
        if current.codex_thread_id.is_none() {
            return Err(StoreError::InvalidWorkspaceTransition {
                workspace_id: workspace_id.to_owned(),
                expected: "a bound Codex thread".to_owned(),
                actual: "no Codex thread".to_owned(),
            });
        }
        let status_json =
            serde_json::to_string(&status.canonicalized()).map_err(json_to_sql_error)?;
        let now = now_ms();
        transaction.execute(
            "UPDATE workspaces SET thread_status_json = ?1, thread_status_generation = ?2,
                thread_status_observed_at_ms = ?3, thread_status_is_fresh = 1,
                last_error_code = CASE
                    WHEN last_error_code = 'THREAD_RECOVERY_FAILED' THEN NULL
                    ELSE last_error_code END,
                last_error_message = CASE
                    WHEN last_error_code = 'THREAD_RECOVERY_FAILED' THEN NULL
                    ELSE last_error_message END,
                updated_at_ms = ?3 WHERE id = ?4",
            params![status_json, runtime_generation, now, workspace_id],
        )?;
        event.workspace_id = Some(workspace_id.to_owned());
        let event = insert_event(&transaction, event)?;
        let workspace = require_workspace(&transaction, workspace_id)?;
        transaction.commit()?;
        Ok((workspace, event))
    }

    pub fn mark_thread_statuses_stale(&self) -> Result<usize, StoreError> {
        let connection = self.lock()?;
        connection
            .execute(
                "UPDATE workspaces SET thread_status_is_fresh = 0
                 WHERE thread_status_is_fresh = 1",
                [],
            )
            .map_err(StoreError::from)
    }

    pub fn workspace_by_id(&self, id: &str) -> Result<Option<Workspace>, StoreError> {
        let connection = self.lock()?;
        get_workspace_by_id(&connection, id)
    }

    pub fn workspace_by_create_operation_id(
        &self,
        operation_id: &str,
    ) -> Result<Option<Workspace>, StoreError> {
        let connection = self.lock()?;
        connection
            .query_row(
                &format!("{} WHERE create_operation_id = ?1", WORKSPACE_SELECT),
                [operation_id],
                map_workspace,
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn workspace_by_thread_id(&self, thread_id: &str) -> Result<Option<Workspace>, StoreError> {
        let connection = self.lock()?;
        connection
            .query_row(
                &format!("{} WHERE codex_thread_id = ?1", WORKSPACE_SELECT),
                [thread_id],
                map_workspace,
            )
            .optional()
            .map_err(StoreError::from)
    }

    /// Replaces a provisional profile with effective, non-secret App Server settings.
    /// A ready workspace may change only while its native thread is still unbound.
    pub fn update_workspace_profile(
        &self,
        workspace_id: &str,
        profile: &ProfileSnapshot,
    ) -> Result<Workspace, StoreError> {
        let profile_json = serde_json::to_string(profile).map_err(json_to_sql_error)?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = require_workspace(&transaction, workspace_id)?;
        let mutable = matches!(
            current.lifecycle,
            WorkspaceLifecycle::Provisioning | WorkspaceLifecycle::Starting
        ) || (current.lifecycle == WorkspaceLifecycle::Ready
            && current.codex_thread_id.is_none()
            && current.active_turn_id.is_none());
        if !mutable {
            return Err(StoreError::InvalidWorkspaceTransition {
                workspace_id: workspace_id.to_owned(),
                expected: "a workspace awaiting native thread binding".to_owned(),
                actual: current.lifecycle.as_str().to_owned(),
            });
        }
        transaction.execute(
            "UPDATE workspaces SET profile_json = ?1, updated_at_ms = ?2 WHERE id = ?3",
            params![profile_json, now_ms(), workspace_id],
        )?;
        let workspace = require_workspace(&transaction, workspace_id)?;
        transaction.commit()?;
        Ok(workspace)
    }

    pub fn workspace_by_name(
        &self,
        repository_id: &str,
        name: &str,
    ) -> Result<Option<Workspace>, StoreError> {
        let connection = self.lock()?;
        connection
            .query_row(
                &format!(
                    "{} WHERE repository_id = ?1 AND name = ?2",
                    WORKSPACE_SELECT
                ),
                params![repository_id, name],
                map_workspace,
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn workspaces_by_name(&self, name: &str) -> Result<Vec<Workspace>, StoreError> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(&format!(
            "{} WHERE name = ?1 ORDER BY repository_id, id",
            WORKSPACE_SELECT
        ))?;
        statement
            .query_map([name], map_workspace)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn list_workspaces(
        &self,
        repository_id: Option<&str>,
    ) -> Result<Vec<Workspace>, StoreError> {
        let connection = self.lock()?;
        if let Some(repository_id) = repository_id {
            let mut statement = connection.prepare(&format!(
                "{} WHERE repository_id = ?1 ORDER BY updated_at_ms DESC, id",
                WORKSPACE_SELECT
            ))?;
            statement
                .query_map([repository_id], map_workspace)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(StoreError::from)
        } else {
            let mut statement = connection.prepare(&format!(
                "{} ORDER BY updated_at_ms DESC, id",
                WORKSPACE_SELECT
            ))?;
            statement
                .query_map([], map_workspace)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(StoreError::from)
        }
    }

    pub fn transition_workspace_availability(
        &self,
        workspace_id: &str,
        expected: WorkspaceAvailability,
        next: WorkspaceAvailability,
        closed_head_sha: Option<&str>,
    ) -> Result<Workspace, StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = require_workspace(&transaction, workspace_id)?;
        if current.availability != expected {
            return Err(StoreError::InvalidWorkspaceTransition {
                workspace_id: workspace_id.to_owned(),
                expected: expected.as_str().to_owned(),
                actual: current.availability.as_str().to_owned(),
            });
        }
        let now = now_ms();
        let closed_at_ms = (next == WorkspaceAvailability::Closed).then_some(now);
        transaction.execute(
            "UPDATE workspaces SET availability = ?1,
                closed_head_sha = CASE
                    WHEN ?1 = 'open' THEN NULL
                    WHEN ?2 IS NOT NULL THEN ?2
                    ELSE closed_head_sha END,
                thread_archived = CASE
                    WHEN ?1 = 'open' THEN 0
                    ELSE thread_archived END,
                closed_at_ms = CASE
                    WHEN ?1 = 'closed' THEN ?3
                    WHEN ?1 = 'open' THEN NULL
                    ELSE closed_at_ms END,
                updated_at_ms = ?3 WHERE id = ?4",
            params![
                next.as_str(),
                closed_head_sha,
                closed_at_ms.unwrap_or(now),
                workspace_id
            ],
        )?;
        let workspace = require_workspace(&transaction, workspace_id)?;
        transaction.commit()?;
        Ok(workspace)
    }

    pub fn begin_workspace_close(
        &self,
        workspace_id: &str,
        closed_head_sha: &str,
        archive_thread: bool,
    ) -> Result<Workspace, StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = require_workspace(&transaction, workspace_id)?;
        if current.availability != WorkspaceAvailability::Open {
            return Err(StoreError::InvalidWorkspaceTransition {
                workspace_id: workspace_id.to_owned(),
                expected: WorkspaceAvailability::Open.as_str().to_owned(),
                actual: current.availability.as_str().to_owned(),
            });
        }
        let now = now_ms();
        transaction.execute(
            "UPDATE workspaces SET availability = 'closing', closed_head_sha = ?1,
                thread_archived = ?2, closed_at_ms = NULL, updated_at_ms = ?3
             WHERE id = ?4",
            params![closed_head_sha, archive_thread, now, workspace_id],
        )?;
        let workspace = require_workspace(&transaction, workspace_id)?;
        transaction.commit()?;
        Ok(workspace)
    }

    pub fn begin_workspace_deletion(
        &self,
        workspace_id: &str,
        intent: WorkspaceDeletionIntent,
    ) -> Result<Workspace, StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = require_workspace(&transaction, workspace_id)?;
        if current.availability != WorkspaceAvailability::Closed {
            return Err(StoreError::InvalidWorkspaceTransition {
                workspace_id: workspace_id.to_owned(),
                expected: WorkspaceAvailability::Closed.as_str().to_owned(),
                actual: current.availability.as_str().to_owned(),
            });
        }
        transaction.execute(
            "UPDATE workspaces SET availability = 'deleting',
                delete_thread_requested = ?1, delete_branch_requested = ?2,
                updated_at_ms = ?3 WHERE id = ?4",
            params![
                intent.delete_thread,
                intent.delete_branch,
                now_ms(),
                workspace_id
            ],
        )?;
        let workspace = require_workspace(&transaction, workspace_id)?;
        transaction.commit()?;
        Ok(workspace)
    }

    pub fn workspace_deletion_intent(
        &self,
        workspace_id: &str,
    ) -> Result<WorkspaceDeletionIntent, StoreError> {
        let connection = self.lock()?;
        require_workspace(&connection, workspace_id)?;
        connection
            .query_row(
                "SELECT delete_thread_requested, delete_branch_requested
                 FROM workspaces WHERE id = ?1",
                [workspace_id],
                |row| {
                    Ok(WorkspaceDeletionIntent {
                        delete_thread: row.get(0)?,
                        delete_branch: row.get(1)?,
                    })
                },
            )
            .map_err(StoreError::from)
    }

    pub fn clear_workspace_thread_binding(
        &self,
        workspace_id: &str,
    ) -> Result<Workspace, StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = require_workspace(&transaction, workspace_id)?;
        if current.availability != WorkspaceAvailability::Deleting {
            return Err(StoreError::InvalidWorkspaceTransition {
                workspace_id: workspace_id.to_owned(),
                expected: WorkspaceAvailability::Deleting.as_str().to_owned(),
                actual: current.availability.as_str().to_owned(),
            });
        }
        transaction.execute(
            "UPDATE workspaces SET codex_thread_id = NULL, thread_archived = 0,
                thread_status_json = NULL, thread_status_generation = NULL,
                thread_status_observed_at_ms = NULL, thread_status_is_fresh = 0,
                updated_at_ms = ?1 WHERE id = ?2",
            params![now_ms(), workspace_id],
        )?;
        let workspace = require_workspace(&transaction, workspace_id)?;
        transaction.commit()?;
        Ok(workspace)
    }

    pub fn delete_workspace_record(&self, workspace_id: &str) -> Result<(), StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let workspace = require_workspace(&transaction, workspace_id)?;
        if workspace.availability != WorkspaceAvailability::Deleting {
            return Err(StoreError::InvalidWorkspaceTransition {
                workspace_id: workspace_id.to_owned(),
                expected: WorkspaceAvailability::Deleting.as_str().to_owned(),
                actual: workspace.availability.as_str().to_owned(),
            });
        }
        transaction.execute(
            "DELETE FROM decisions WHERE workspace_id = ?1",
            [workspace_id],
        )?;
        transaction.execute("DELETE FROM events WHERE workspace_id = ?1", [workspace_id])?;
        transaction.execute(
            "DELETE FROM audit_events WHERE workspace_id = ?1",
            [workspace_id],
        )?;
        transaction.execute(
            "DELETE FROM operations WHERE workspace_id = ?1",
            [workspace_id],
        )?;
        transaction.execute("DELETE FROM turns WHERE workspace_id = ?1", [workspace_id])?;
        transaction.execute("DELETE FROM workspaces WHERE id = ?1", [workspace_id])?;
        transaction.commit()?;
        Ok(())
    }

    pub fn cancel_workspace_deletion(&self, workspace_id: &str) -> Result<Workspace, StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = require_workspace(&transaction, workspace_id)?;
        if current.availability != WorkspaceAvailability::Deleting {
            return Err(StoreError::InvalidWorkspaceTransition {
                workspace_id: workspace_id.to_owned(),
                expected: WorkspaceAvailability::Deleting.as_str().to_owned(),
                actual: current.availability.as_str().to_owned(),
            });
        }
        transaction.execute(
            "UPDATE workspaces SET availability = 'closed', delete_thread_requested = 0,
                delete_branch_requested = 0, updated_at_ms = ?1 WHERE id = ?2",
            params![now_ms(), workspace_id],
        )?;
        let workspace = require_workspace(&transaction, workspace_id)?;
        transaction.commit()?;
        Ok(workspace)
    }

    pub fn transitional_workspaces(&self) -> Result<Vec<Workspace>, StoreError> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(&format!(
            "{} WHERE availability IN ('closing', 'reopening', 'deleting') ORDER BY id",
            WORKSPACE_SELECT
        ))?;
        statement
            .query_map([], map_workspace)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    #[cfg(test)]
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

    /// Marks state that cannot safely be assumed successful after daemon loss.
    pub fn reconcile_unfinished(&self) -> Result<ReconciliationSummary, StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let stale_thread_snapshots = transaction.execute(
            "UPDATE workspaces SET thread_status_is_fresh = 0
             WHERE thread_status_is_fresh = 1",
            [],
        )?;
        let now = now_ms();
        let uncertain_operations = reconcile_unconfirmed_operations(&transaction, now)?;
        let mut statement = transaction.prepare(&format!(
            "{} WHERE lifecycle IN ('provisioning', 'starting') ORDER BY id",
            WORKSPACE_SELECT
        ))?;
        let workspaces = statement
            .query_map([], map_workspace)?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);

        let failed_workspace_preparations = workspaces.len();
        for workspace in workspaces {
            let message = "Workspace preparation was unfinished when cocod restarted";
            transaction.execute(
                "UPDATE workspaces SET lifecycle = 'failed',
                    last_error_code = 'DAEMON_RESTART', last_error_message = ?1,
                    updated_at_ms = ?2 WHERE id = ?3",
                params![message, now, workspace.id],
            )?;
            insert_event(
                &transaction,
                EventDraft {
                    workspace_id: Some(workspace.id),
                    turn_id: None,
                    kind: EventKind::AgentFailed,
                    source: EventSource::Coco,
                    source_method: Some("startup.reconcile".to_owned()),
                    occurred_at_ms: None,
                    payload: json!({
                        "lifecycle": "failed",
                        "reason": "daemon_restart"
                    }),
                },
            )?;
        }
        transaction.commit()?;
        Ok(ReconciliationSummary {
            failed_workspace_preparations,
            uncertain_operations,
            stale_thread_snapshots,
        })
    }
}

pub(super) fn assert_workspace_lifecycle(
    connection: &Connection,
    workspace_id: &str,
    expected: &[WorkspaceLifecycle],
) -> Result<(), StoreError> {
    let workspace = require_workspace(connection, workspace_id)?;
    if expected.contains(&workspace.lifecycle) {
        return Ok(());
    }
    Err(StoreError::InvalidWorkspaceTransition {
        workspace_id: workspace_id.to_owned(),
        expected: expected
            .iter()
            .map(|lifecycle| lifecycle.as_str())
            .collect::<Vec<_>>()
            .join(" or "),
        actual: workspace.lifecycle.as_str().to_owned(),
    })
}
