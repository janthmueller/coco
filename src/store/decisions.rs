use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use serde_json::{Value, json};

use super::events::insert_event;
use super::rows::{DECISION_SELECT, map_stored_decision, require_turn, require_workspace};
use super::{
    EventDraft, NewDecision, Store, StoreError, StoredDecision, json_to_sql_error, new_id, now_ms,
};
use crate::domain::{DecisionState, EventKind, EventSource, NormalizedEvent};

impl Store {
    pub fn create_decision_with_event(
        &self,
        input: NewDecision,
        mut event: EventDraft,
    ) -> Result<(StoredDecision, NormalizedEvent), StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_decision_binding(&transaction, &input)?;

        let id = new_id();
        let received_at_ms = now_ms();
        let request_id_json =
            serde_json::to_string(&input.native_request_id).map_err(json_to_sql_error)?;
        let prompt_json = serde_json::to_string(&input.prompt).map_err(json_to_sql_error)?;
        let native_options_json =
            serde_json::to_string(&input.native_options).map_err(json_to_sql_error)?;
        transaction.execute(
            "INSERT INTO decisions (
                id, workspace_id, turn_id, codex_thread_id, codex_turn_id,
                runtime_generation, native_request_id_json, method, kind, state,
                prompt_json, native_options_json, received_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'pending', ?10, ?11, ?12)",
            params![
                id,
                input.workspace_id,
                input.turn_id,
                input.codex_thread_id,
                input.codex_turn_id,
                input.runtime_generation,
                request_id_json,
                input.method,
                input.kind.as_str(),
                prompt_json,
                native_options_json,
                received_at_ms,
            ],
        )?;

        event.workspace_id = Some(input.workspace_id);
        event.turn_id = input.turn_id;
        add_event_field(&mut event.payload, "decisionId", Value::String(id.clone()));
        let event = insert_event(&transaction, event)?;
        let decision = require_decision(&transaction, &id)?;
        transaction.commit()?;
        Ok((decision, event))
    }

    pub fn decision_by_id(&self, id: &str) -> Result<Option<StoredDecision>, StoreError> {
        let connection = self.lock()?;
        get_decision_by_id(&connection, id)
    }

    pub fn open_decisions_for_workspace(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<StoredDecision>, StoreError> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(&format!(
            "{} WHERE workspace_id = ?1 AND state IN ('pending', 'submitted')
             ORDER BY received_at_ms, id",
            DECISION_SELECT
        ))?;
        statement
            .query_map([workspace_id], map_stored_decision)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn mark_decision_submitted(
        &self,
        id: &str,
        runtime_generation: &str,
        response_summary: &Value,
    ) -> Result<StoredDecision, StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = require_decision(&transaction, id)?;
        validate_pending_generation(&current, runtime_generation)?;
        let now = now_ms();
        let response_summary =
            serde_json::to_string(response_summary).map_err(json_to_sql_error)?;
        transaction.execute(
            "UPDATE decisions SET state = 'submitted', response_summary_json = ?1,
                submitted_at_ms = ?2 WHERE id = ?3 AND state = 'pending'",
            params![response_summary, now, id],
        )?;
        let decision = require_decision(&transaction, id)?;
        transaction.commit()?;
        Ok(decision)
    }

    pub fn resolve_decision_by_native_request(
        &self,
        runtime_generation: &str,
        codex_thread_id: &str,
        native_request_id: &Value,
    ) -> Result<Option<StoredDecision>, StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let request_id_json =
            serde_json::to_string(native_request_id).map_err(json_to_sql_error)?;
        let Some(current) = transaction
            .query_row(
                &format!(
                    "{} WHERE runtime_generation = ?1 AND codex_thread_id = ?2
                     AND native_request_id_json = ?3",
                    DECISION_SELECT
                ),
                params![runtime_generation, codex_thread_id, request_id_json],
                map_stored_decision,
            )
            .optional()?
        else {
            transaction.commit()?;
            return Ok(None);
        };
        if !current.decision.state.is_open() {
            transaction.commit()?;
            return Ok(Some(current));
        }

        let now = now_ms();
        transaction.execute(
            "UPDATE decisions SET state = 'resolved', resolved_at_ms = ?1
             WHERE id = ?2 AND state IN ('pending', 'submitted')",
            params![now, current.decision.id],
        )?;
        insert_event(
            &transaction,
            EventDraft {
                workspace_id: Some(current.decision.workspace_id.clone()),
                turn_id: current.decision.turn_id.clone(),
                kind: EventKind::DecisionResolved,
                source: EventSource::Codex,
                source_method: Some("serverRequest/resolved".to_owned()),
                occurred_at_ms: None,
                payload: json!({
                    "decisionId": current.decision.id,
                    "outcome": "resolved",
                    "hadSubmittedResponse": current.decision.state == DecisionState::Submitted,
                }),
            },
        )?;
        let decision = require_decision(&transaction, &current.decision.id)?;
        transaction.commit()?;
        Ok(Some(decision))
    }

    pub fn orphan_submitted_decision(
        &self,
        id: &str,
        runtime_generation: &str,
        reason: &str,
    ) -> Result<StoredDecision, StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = require_decision(&transaction, id)?;
        if current.runtime_generation != runtime_generation {
            return Err(StoreError::DecisionGenerationMismatch {
                decision_id: id.to_owned(),
            });
        }
        if current.decision.state == DecisionState::Submitted {
            orphan_decision(&transaction, &current, reason)?;
        }
        let decision = require_decision(&transaction, id)?;
        transaction.commit()?;
        Ok(decision)
    }

    pub fn orphan_open_decisions(
        &self,
        runtime_generation: Option<&str>,
        reason: &str,
    ) -> Result<usize, StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let decisions = open_decisions(&transaction, runtime_generation)?;
        for decision in &decisions {
            orphan_decision(&transaction, decision, reason)?;
        }
        transaction.commit()?;
        Ok(decisions.len())
    }
}

fn validate_decision_binding(
    transaction: &Transaction<'_>,
    input: &NewDecision,
) -> Result<(), StoreError> {
    let workspace = require_workspace(transaction, &input.workspace_id)?;
    if workspace.codex_thread_id.as_deref() != Some(input.codex_thread_id.as_str()) {
        return Err(StoreError::InvalidStoredValue {
            field: "decision codex_thread_id",
            value: input.codex_thread_id.clone(),
        });
    }
    if let Some(turn_id) = input.turn_id.as_deref() {
        let turn = require_turn(transaction, turn_id)?;
        if turn.workspace_id != input.workspace_id {
            return Err(StoreError::EventCorrelation {
                turn_id: turn_id.to_owned(),
                turn_workspace_id: turn.workspace_id,
                event_workspace_id: input.workspace_id.clone(),
            });
        }
        if let Some(codex_turn_id) = input.codex_turn_id.as_deref()
            && turn.codex_turn_id.as_deref() != Some(codex_turn_id)
        {
            return Err(StoreError::InvalidStoredValue {
                field: "decision codex_turn_id",
                value: codex_turn_id.to_owned(),
            });
        }
    }
    let valid_request_id = match &input.native_request_id {
        Value::String(_) => true,
        Value::Number(number) => number.is_i64() || number.is_u64(),
        _ => false,
    };
    if !valid_request_id {
        return Err(StoreError::InvalidStoredValue {
            field: "decision native_request_id",
            value: input.native_request_id.to_string(),
        });
    }
    Ok(())
}

fn validate_pending_generation(
    decision: &StoredDecision,
    runtime_generation: &str,
) -> Result<(), StoreError> {
    if decision.runtime_generation != runtime_generation {
        return Err(StoreError::DecisionGenerationMismatch {
            decision_id: decision.decision.id.clone(),
        });
    }
    if decision.decision.state != DecisionState::Pending {
        return Err(StoreError::InvalidDecisionState {
            decision_id: decision.decision.id.clone(),
            actual: decision.decision.state.as_str().to_owned(),
        });
    }
    Ok(())
}

fn get_decision_by_id(
    connection: &rusqlite::Connection,
    id: &str,
) -> Result<Option<StoredDecision>, StoreError> {
    connection
        .query_row(
            &format!("{} WHERE id = ?1", DECISION_SELECT),
            [id],
            map_stored_decision,
        )
        .optional()
        .map_err(StoreError::from)
}

fn require_decision(
    connection: &rusqlite::Connection,
    id: &str,
) -> Result<StoredDecision, StoreError> {
    get_decision_by_id(connection, id)?.ok_or_else(|| StoreError::NotFound {
        entity: "decision",
        id: id.to_owned(),
    })
}

fn open_decisions(
    transaction: &Transaction<'_>,
    runtime_generation: Option<&str>,
) -> Result<Vec<StoredDecision>, StoreError> {
    let query = match runtime_generation {
        Some(_) => format!(
            "{} WHERE runtime_generation = ?1 AND state IN ('pending', 'submitted')
             ORDER BY received_at_ms, id",
            DECISION_SELECT
        ),
        None => format!(
            "{} WHERE state IN ('pending', 'submitted') ORDER BY received_at_ms, id",
            DECISION_SELECT
        ),
    };
    let mut statement = transaction.prepare(&query)?;
    let rows = match runtime_generation {
        Some(runtime_generation) => {
            statement.query_map([runtime_generation], map_stored_decision)?
        }
        None => statement.query_map([], map_stored_decision)?,
    };
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(StoreError::from)
}

fn orphan_decision(
    transaction: &Transaction<'_>,
    current: &StoredDecision,
    reason: &str,
) -> Result<(), StoreError> {
    let now = now_ms();
    transaction.execute(
        "UPDATE decisions SET state = 'orphaned', resolved_at_ms = ?1
         WHERE id = ?2 AND state IN ('pending', 'submitted')",
        params![now, current.decision.id],
    )?;
    insert_event(
        transaction,
        EventDraft {
            workspace_id: Some(current.decision.workspace_id.clone()),
            turn_id: current.decision.turn_id.clone(),
            kind: EventKind::DecisionResolved,
            source: EventSource::Coco,
            source_method: Some("decision.orphan".to_owned()),
            occurred_at_ms: None,
            payload: json!({
                "decisionId": current.decision.id,
                "outcome": "orphaned",
                "reason": reason,
            }),
        },
    )?;
    Ok(())
}

fn add_event_field(payload: &mut Value, key: &str, value: Value) {
    if let Value::Object(object) = payload {
        object.insert(key.to_owned(), value);
    } else {
        let mut object = serde_json::Map::new();
        object.insert(key.to_owned(), value);
        *payload = Value::Object(object);
    }
}
