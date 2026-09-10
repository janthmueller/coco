use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::de::DeserializeOwned;

use super::hooks::insert_hook_dispatch;
use super::{Store, StoreError, json_to_sql_error};
use crate::domain::hooks::HookDispatch;
use crate::domain::signals::{MAX_SIGNAL_TYPES, SIGNAL_RETENTION, Signal, SignalError, SignalType};

mod read;
#[cfg(test)]
mod tests;

impl Store {
    pub(crate) fn register_signal_types(
        &self,
        definitions: Vec<SignalType>,
    ) -> Result<Vec<SignalType>, StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut accepted = Vec::with_capacity(definitions.len());
        for definition in definitions {
            if let Some(existing) = type_by_key(
                &transaction,
                &definition.repository_id,
                &definition.name,
                definition.version,
            )? {
                if existing.description != definition.description
                    || existing.payload_schema != definition.payload_schema
                {
                    return Err(SignalError::VersionConflict {
                        name: definition.name,
                        version: definition.version,
                    }
                    .into());
                }
                accepted.push(existing);
                continue;
            }
            let count: i64 = transaction.query_row(
                "SELECT count(*) FROM signal_types WHERE repository_id = ?1",
                [&definition.repository_id],
                |row| row.get(0),
            )?;
            if count >= MAX_SIGNAL_TYPES as i64 {
                return Err(SignalError::CatalogFull.into());
            }
            transaction.execute("INSERT INTO signal_types(repository_id, name, version, body_json) VALUES (?1, ?2, ?3, ?4)", params![definition.repository_id, definition.name, definition.version, serde_json::to_string(&definition).map_err(json_to_sql_error)?])?;
            accepted.push(definition);
        }
        transaction.commit()?;
        Ok(accepted)
    }

    pub(crate) fn signal_type(
        &self,
        repository: &str,
        name: &str,
        version: u32,
    ) -> Result<Option<SignalType>, StoreError> {
        let connection = self.lock()?;
        type_by_key(&connection, repository, name, version)
    }

    pub(crate) fn list_signal_types(
        &self,
        repository: &str,
    ) -> Result<Vec<SignalType>, StoreError> {
        let connection = self.lock()?;
        let mut statement = connection.prepare("SELECT body_json FROM signal_types WHERE repository_id = ?1 ORDER BY name, version LIMIT 128")?;
        statement
            .query_map([repository], |row| row.get::<_, String>(0))?
            .map(|row| decode(&row?))
            .collect()
    }

    /// Definition validation happens before this call. Its version is immutable.
    #[cfg(test)]
    pub(crate) fn emit_signal(&self, signal: Signal) -> Result<Signal, StoreError> {
        self.emit_signal_with_hook(signal, None)
    }

    /// Definition and hook matching happen before this call. Accepted retries
    /// return their original record without enqueueing another delivery.
    pub(crate) fn emit_signal_with_hook(
        &self,
        mut signal: Signal,
        hook: Option<HookDispatch>,
    ) -> Result<Signal, StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let prior: Option<String> = transaction
            .query_row(
                "SELECT body_json FROM signals WHERE workspace_id = ?1 AND idempotency_key = ?2",
                params![signal.workspace_id, signal.idempotency_key],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(prior) = prior {
            let prior: Signal = decode(&prior)?;
            if prior.name != signal.name
                || prior.version != signal.version
                || prior.payload != signal.payload
                || prior.thread_id != signal.thread_id
            {
                return Err(SignalError::IdempotencyConflict.into());
            }
            return Ok(prior);
        }
        let open: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM workspaces WHERE id = ?1 AND repository_id = ?2 AND codex_thread_id = ?3 AND availability = 'open' AND lifecycle = 'ready')", params![signal.workspace_id, signal.repository_id, signal.thread_id], |row| row.get(0))?;
        if !open {
            return Err(SignalError::InvalidSender.into());
        }
        let now = Utc::now().timestamp_millis();
        let rate: i64 = transaction.query_row(
            "SELECT count(*) FROM signals WHERE workspace_id = ?1 AND occurred_at_ms > ?2",
            params![signal.workspace_id, now - 1000],
            |row| row.get(0),
        )?;
        if rate >= 10 {
            return Err(SignalError::RateLimited.into());
        }
        signal.occurred_at_ms = now;
        signal.sequence = transaction.query_row("UPDATE signal_stream SET high_water = high_water + 1 WHERE singleton = 1 RETURNING high_water", [], |row| row.get(0))?;
        transaction.execute("INSERT INTO signals(sequence, repository_id, workspace_id, name, version, idempotency_key, occurred_at_ms, body_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)", params![signal.sequence, signal.repository_id, signal.workspace_id, signal.name, signal.version, signal.idempotency_key, signal.occurred_at_ms, serde_json::to_string(&signal).map_err(json_to_sql_error)?])?;
        insert_hook_dispatch(&transaction, hook)?;
        let expired = (signal.sequence - SIGNAL_RETENTION).max(0);
        transaction.execute("DELETE FROM signals WHERE sequence <= ?1", [expired])?;
        transaction.execute(
            "UPDATE signal_stream SET expired_through = ?1 WHERE singleton = 1",
            [expired],
        )?;
        transaction.commit()?;
        Ok(signal)
    }
}

fn type_by_key(
    connection: &Connection,
    repository: &str,
    name: &str,
    version: u32,
) -> Result<Option<SignalType>, StoreError> {
    let body: Option<String> = connection.query_row("SELECT body_json FROM signal_types WHERE repository_id = ?1 AND name = ?2 AND version = ?3", params![repository, name, version], |row| row.get(0)).optional()?;
    body.map(|body| decode(&body)).transpose()
}

fn decode<T: DeserializeOwned>(body: &str) -> Result<T, StoreError> {
    serde_json::from_str(body).map_err(json_to_sql_error)
}
