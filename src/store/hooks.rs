use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};

use super::{Store, StoreError, json_to_sql_error, new_id, now_ms};
use crate::domain::hooks::{
    ClaimedHookDelivery, HOOK_DELIVERY_RETENTION, HookDeliveryState, HookDeliverySummary,
    HookDispatch, HookEventKind,
};

const MAX_HOOK_HISTORY_PAGE: u32 = 100;
const MAX_PUBLIC_HOOK_ERROR_BYTES: usize = 512;

struct PendingHookDeliveryRow {
    id: String,
    event_id: String,
    hook_id: String,
    definition_hash: String,
    event: String,
    attempts: u32,
    created_at_ms: i64,
    body: Vec<u8>,
}

impl Store {
    pub(crate) fn recover_hook_deliveries(&self) -> Result<usize, StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let now = now_ms();
        let recovered = transaction.execute(
            "UPDATE hook_deliveries SET state = 'pending', next_attempt_at_ms = ?1,
                started_at_ms = NULL,
                last_error_message = 'delivery owner stopped before recording an outcome'
             WHERE state = 'running'",
            [now],
        )?;
        transaction.commit()?;
        Ok(recovered)
    }

    pub(crate) fn claim_hook_delivery(
        &self,
        now: i64,
    ) -> Result<Option<ClaimedHookDelivery>, StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let row: Option<PendingHookDeliveryRow> = transaction
            .query_row(
                "SELECT d.id, d.event_id, d.hook_id, d.definition_hash, e.kind,
                        d.attempts, d.created_at_ms, CAST(e.body_json AS BLOB)
                 FROM hook_deliveries d
                 JOIN hook_events e ON e.id = d.event_id
                 WHERE d.state = 'pending' AND d.next_attempt_at_ms <= ?1
                   AND NOT EXISTS (
                       SELECT 1 FROM hook_deliveries earlier
                       JOIN hook_events earlier_event ON earlier_event.id = earlier.event_id
                       WHERE earlier.hook_id = d.hook_id
                         AND earlier.state IN ('pending', 'running')
                         AND earlier_event.sequence < e.sequence
                   )
                 ORDER BY d.next_attempt_at_ms, d.created_at_ms, d.id
                 LIMIT 1",
                [now],
                |row| {
                    Ok(PendingHookDeliveryRow {
                        id: row.get(0)?,
                        event_id: row.get(1)?,
                        hook_id: row.get(2)?,
                        definition_hash: row.get(3)?,
                        event: row.get(4)?,
                        attempts: row.get(5)?,
                        created_at_ms: row.get(6)?,
                        body: row.get(7)?,
                    })
                },
            )
            .optional()?;
        let Some(row) = row else {
            transaction.commit()?;
            return Ok(None);
        };
        let next_attempt = row.attempts.saturating_add(1);
        let changed = transaction.execute(
            "UPDATE hook_deliveries SET state = 'running', attempts = ?1,
                next_attempt_at_ms = NULL, started_at_ms = ?2
             WHERE id = ?3 AND state = 'pending'",
            params![next_attempt, now, &row.id],
        )?;
        if changed != 1 {
            transaction.commit()?;
            return Ok(None);
        }
        let event =
            HookEventKind::parse(&row.event).ok_or_else(|| StoreError::InvalidStoredValue {
                field: "hook event kind",
                value: row.event,
            })?;
        transaction.commit()?;
        Ok(Some(ClaimedHookDelivery {
            summary: HookDeliverySummary {
                id: row.id,
                event_id: row.event_id,
                hook_id: row.hook_id,
                event,
                state: HookDeliveryState::Running,
                attempts: next_attempt,
                created_at_ms: row.created_at_ms,
                next_attempt_at_ms: None,
                started_at_ms: Some(now),
                finished_at_ms: None,
                last_error: None,
            },
            definition_hash: row.definition_hash,
            event_body: row.body,
        }))
    }

    pub(crate) fn complete_hook_delivery(
        &self,
        id: &str,
        max_attempts: u32,
        error: Option<&str>,
    ) -> Result<HookDeliverySummary, StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current =
            hook_delivery_by_id(&transaction, id)?.ok_or_else(|| StoreError::NotFound {
                entity: "hook delivery",
                id: id.to_owned(),
            })?;
        if current.state != HookDeliveryState::Running {
            return Err(StoreError::InvalidStoredValue {
                field: "hook delivery state",
                value: current.state.as_str().to_owned(),
            });
        }
        let now = now_ms();
        let error = error.map(bounded_hook_error);
        let (state, next_attempt_at_ms, finished_at_ms) = match error.as_ref() {
            None => (HookDeliveryState::Succeeded, None, Some(now)),
            Some(_) if current.attempts < max_attempts => (
                HookDeliveryState::Pending,
                Some(now + retry_delay_ms(current.attempts)),
                None,
            ),
            Some(_) => (HookDeliveryState::Failed, None, Some(now)),
        };
        transaction.execute(
            "UPDATE hook_deliveries SET state = ?1, next_attempt_at_ms = ?2,
                started_at_ms = CASE WHEN ?1 = 'pending' THEN NULL ELSE started_at_ms END,
                finished_at_ms = ?3, last_error_message = ?4 WHERE id = ?5",
            params![
                state.as_str(),
                next_attempt_at_ms,
                finished_at_ms,
                error,
                id
            ],
        )?;
        if matches!(
            state,
            HookDeliveryState::Succeeded | HookDeliveryState::Failed | HookDeliveryState::Cancelled
        ) {
            prune_hook_history(&transaction)?;
        }
        let delivery =
            hook_delivery_by_id(&transaction, id)?.ok_or_else(|| StoreError::NotFound {
                entity: "hook delivery",
                id: id.to_owned(),
            })?;
        transaction.commit()?;
        Ok(delivery)
    }

    pub(crate) fn cancel_hook_delivery(
        &self,
        id: &str,
        reason: &str,
    ) -> Result<HookDeliverySummary, StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let now = now_ms();
        let reason = bounded_hook_error(reason);
        let changed = transaction.execute(
            "UPDATE hook_deliveries SET state = 'cancelled', next_attempt_at_ms = NULL,
                finished_at_ms = ?1, last_error_message = ?2
             WHERE id = ?3 AND state = 'running'",
            params![now, reason, id],
        )?;
        if changed != 1 {
            return Err(StoreError::InvalidStoredValue {
                field: "hook delivery state",
                value: "expected running delivery".to_owned(),
            });
        }
        prune_hook_history(&transaction)?;
        let delivery =
            hook_delivery_by_id(&transaction, id)?.ok_or_else(|| StoreError::NotFound {
                entity: "hook delivery",
                id: id.to_owned(),
            })?;
        transaction.commit()?;
        Ok(delivery)
    }

    pub(crate) fn list_hook_deliveries(
        &self,
        limit: u32,
    ) -> Result<Vec<HookDeliverySummary>, StoreError> {
        if !(1..=MAX_HOOK_HISTORY_PAGE).contains(&limit) {
            return Ok(Vec::new());
        }
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT d.id, d.event_id, d.hook_id, e.kind, d.state, d.attempts,
                    d.created_at_ms, d.next_attempt_at_ms, d.started_at_ms,
                    d.finished_at_ms, d.last_error_message
             FROM hook_deliveries d
             JOIN hook_events e ON e.id = d.event_id
             ORDER BY e.sequence DESC, d.id DESC LIMIT ?1",
        )?;
        statement
            .query_map([limit], map_hook_delivery)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }
}

pub(super) fn insert_hook_dispatch(
    transaction: &Transaction<'_>,
    dispatch: Option<HookDispatch>,
) -> Result<(), StoreError> {
    let Some(dispatch) = dispatch else {
        return Ok(());
    };
    let body = serde_json::to_string(&dispatch.event).map_err(json_to_sql_error)?;
    transaction.execute(
        "INSERT INTO hook_events (
            id, kind, repository_id, workspace_id, occurred_at_ms, body_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            dispatch.event.id,
            dispatch.event.kind.as_str(),
            dispatch.event.repository.id,
            dispatch.event.workspace.id,
            dispatch.event.occurred_at_ms,
            body
        ],
    )?;
    for target in dispatch.targets {
        transaction.execute(
            "INSERT INTO hook_deliveries (
                id, event_id, hook_id, definition_hash, state, attempts,
                next_attempt_at_ms, created_at_ms
             ) VALUES (?1, ?2, ?3, ?4, 'pending', 0, ?5, ?5)",
            params![
                new_id(),
                dispatch.event.id,
                target.hook_id,
                target.definition_hash,
                dispatch.event.occurred_at_ms
            ],
        )?;
    }
    Ok(())
}

fn hook_delivery_by_id(
    transaction: &Transaction<'_>,
    id: &str,
) -> Result<Option<HookDeliverySummary>, StoreError> {
    transaction
        .query_row(
            "SELECT d.id, d.event_id, d.hook_id, e.kind, d.state, d.attempts,
                    d.created_at_ms, d.next_attempt_at_ms, d.started_at_ms,
                    d.finished_at_ms, d.last_error_message
             FROM hook_deliveries d
             JOIN hook_events e ON e.id = d.event_id WHERE d.id = ?1",
            [id],
            map_hook_delivery,
        )
        .optional()
        .map_err(StoreError::from)
}

fn map_hook_delivery(row: &rusqlite::Row<'_>) -> rusqlite::Result<HookDeliverySummary> {
    let event: String = row.get(3)?;
    let state: String = row.get(4)?;
    Ok(HookDeliverySummary {
        id: row.get(0)?,
        event_id: row.get(1)?,
        hook_id: row.get(2)?,
        event: HookEventKind::parse(&event).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Text,
                format!("invalid hook event kind {event:?}").into(),
            )
        })?,
        state: HookDeliveryState::parse(&state).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                format!("invalid hook delivery state {state:?}").into(),
            )
        })?,
        attempts: row.get(5)?,
        created_at_ms: row.get(6)?,
        next_attempt_at_ms: row.get(7)?,
        started_at_ms: row.get(8)?,
        finished_at_ms: row.get(9)?,
        last_error: row.get(10)?,
    })
}

fn retry_delay_ms(attempt: u32) -> i64 {
    1_000_i64.saturating_mul(1_i64 << attempt.saturating_sub(1).min(5))
}

fn bounded_hook_error(value: &str) -> String {
    let mut sanitized = value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .take(MAX_PUBLIC_HOOK_ERROR_BYTES + 1)
        .collect::<String>();
    if sanitized.chars().count() > MAX_PUBLIC_HOOK_ERROR_BYTES {
        sanitized = sanitized
            .chars()
            .take(MAX_PUBLIC_HOOK_ERROR_BYTES.saturating_sub(1))
            .collect();
        sanitized.push('…');
    }
    sanitized
}

fn prune_hook_history(transaction: &Transaction<'_>) -> Result<(), StoreError> {
    transaction.execute(
        "DELETE FROM hook_events WHERE id IN (
            SELECT e.id FROM hook_events e
            WHERE NOT EXISTS (
                SELECT 1 FROM hook_deliveries d
                WHERE d.event_id = e.id AND d.state IN ('pending', 'running')
            )
            ORDER BY e.sequence DESC
            LIMIT -1 OFFSET ?1
         )",
        [HOOK_DELIVERY_RETENTION],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests;
