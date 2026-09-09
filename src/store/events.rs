use rusqlite::{Transaction, TransactionBehavior, params};

#[cfg(test)]
use super::rows::{AUDIT_SELECT, map_audit};
use super::rows::{EVENT_SELECT, map_event, require_turn};
use super::{AuditDraft, EventDraft, Store, StoreError, json_to_sql_error, new_id, now_ms};
use crate::domain::{Audit, NormalizedEvent};

impl Store {
    pub fn append_audit(&self, input: AuditDraft) -> Result<Audit, StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let audit = insert_audit(&transaction, input)?;
        transaction.commit()?;
        Ok(audit)
    }

    pub fn events_after(
        &self,
        workspace_id: Option<&str>,
        after_sequence: i64,
    ) -> Result<Vec<NormalizedEvent>, StoreError> {
        let connection = self.lock()?;
        let mut statement = if workspace_id.is_some() {
            connection.prepare(&format!(
                "{} WHERE workspace_id = ?1 AND sequence > ?2 ORDER BY sequence",
                EVENT_SELECT
            ))?
        } else {
            connection.prepare(&format!(
                "{} WHERE sequence > ?1 ORDER BY sequence",
                EVENT_SELECT
            ))?
        };
        let rows = match workspace_id {
            Some(workspace_id) => {
                statement.query_map(params![workspace_id, after_sequence], map_event)?
            }
            None => statement.query_map(params![after_sequence], map_event)?,
        };
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    #[cfg(test)]
    pub fn audits_after(
        &self,
        workspace_id: Option<&str>,
        after_sequence: i64,
    ) -> Result<Vec<Audit>, StoreError> {
        let connection = self.lock()?;
        let mut statement = if workspace_id.is_some() {
            connection.prepare(&format!(
                "{} WHERE workspace_id = ?1 AND sequence > ?2 ORDER BY sequence",
                AUDIT_SELECT
            ))?
        } else {
            connection.prepare(&format!(
                "{} WHERE sequence > ?1 ORDER BY sequence",
                AUDIT_SELECT
            ))?
        };
        let rows = match workspace_id {
            Some(workspace_id) => {
                statement.query_map(params![workspace_id, after_sequence], map_audit)?
            }
            None => statement.query_map(params![after_sequence], map_audit)?,
        };
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }
}

pub(super) fn insert_event(
    transaction: &Transaction<'_>,
    mut event: EventDraft,
) -> Result<NormalizedEvent, StoreError> {
    if let Some(turn_id) = event.turn_id.as_deref() {
        let turn = require_turn(transaction, turn_id)?;
        if let Some(workspace_id) = event.workspace_id.as_deref() {
            if workspace_id != turn.workspace_id {
                return Err(StoreError::EventCorrelation {
                    turn_id: turn_id.to_owned(),
                    turn_workspace_id: turn.workspace_id,
                    event_workspace_id: workspace_id.to_owned(),
                });
            }
        } else {
            event.workspace_id = Some(turn.workspace_id);
        }
    }
    let id = new_id();
    let recorded_at_ms = now_ms();
    let payload_json = serde_json::to_string(&event.payload).map_err(json_to_sql_error)?;
    transaction.execute(
        "INSERT INTO events (
            id, workspace_id, turn_id, kind, source, source_method, occurred_at_ms,
            recorded_at_ms, payload_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            id,
            event.workspace_id,
            event.turn_id,
            event.kind.as_str(),
            event.source.as_str(),
            event.source_method,
            event.occurred_at_ms,
            recorded_at_ms,
            payload_json,
        ],
    )?;
    Ok(NormalizedEvent {
        sequence: transaction.last_insert_rowid(),
        id,
        workspace_id: event.workspace_id,
        turn_id: event.turn_id,
        kind: event.kind,
        source: event.source,
        source_method: event.source_method,
        occurred_at_ms: event.occurred_at_ms,
        recorded_at_ms,
        payload: event.payload,
    })
}

fn insert_audit(transaction: &Transaction<'_>, input: AuditDraft) -> Result<Audit, StoreError> {
    let id = new_id();
    let occurred_at_ms = input.occurred_at_ms.unwrap_or_else(now_ms);
    let details_json = serde_json::to_string(&input.details).map_err(json_to_sql_error)?;
    transaction.execute(
        "INSERT INTO audit_events (
            id, source, action, workspace_id, operation_id, outcome, details_json, occurred_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            id,
            input.source,
            input.action,
            input.workspace_id,
            input.operation_id,
            input.outcome.as_str(),
            details_json,
            occurred_at_ms,
        ],
    )?;
    Ok(Audit {
        sequence: transaction.last_insert_rowid(),
        id,
        source: input.source,
        action: input.action,
        workspace_id: input.workspace_id,
        operation_id: input.operation_id,
        outcome: input.outcome,
        details: input.details,
        occurred_at_ms,
    })
}
