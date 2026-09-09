use super::{Store, StoreError, decode};
use crate::domain::signals::{MAX_SIGNAL_PAGE, Signal, SignalError, SignalFilter, SignalPage};
use rusqlite::params;
use sha2::{Digest, Sha256};

impl Store {
    pub(crate) fn list_signals(
        &self,
        filter: &SignalFilter,
        after: Option<&str>,
        limit: u32,
    ) -> Result<SignalPage, StoreError> {
        if !(1..=MAX_SIGNAL_PAGE).contains(&limit) {
            return Err(SignalError::InvalidCursor.into());
        }
        let connection = self.lock()?;
        let (stream, high_water, expired): (String, i64, i64) = connection.query_row(
            "SELECT stream_id, high_water, expired_through FROM signal_stream WHERE singleton = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        let digest = hex::encode(Sha256::digest(
            serde_json::to_vec(filter).map_err(super::json_to_sql_error)?,
        ));
        let prefix = format!("v1.{stream}.{digest}.");
        let sequence = match after {
            None => expired,
            Some(cursor) => cursor
                .strip_prefix(&prefix)
                .and_then(|value| value.parse::<i64>().ok())
                .filter(|value| *value >= 0 && *value <= high_water)
                .ok_or(SignalError::InvalidCursor)?,
        };
        if sequence < expired {
            return Err(SignalError::ExpiredCursor.into());
        }
        let mut statement = connection.prepare("SELECT body_json FROM signals WHERE sequence > ?1 AND sequence <= ?2 AND (?3 IS NULL OR repository_id = ?3) AND (?4 IS NULL OR workspace_id = ?4) AND (?5 IS NULL OR name = ?5) ORDER BY sequence LIMIT ?6")?;
        let mut signals: Vec<Signal> = statement
            .query_map(
                params![
                    sequence,
                    high_water,
                    filter.repository_id,
                    filter.workspace_id,
                    filter.name,
                    limit + 1
                ],
                |row| row.get::<_, String>(0),
            )?
            .map(|body| decode(&body?))
            .collect::<Result<_, _>>()?;
        let has_more = signals.len() > limit as usize;
        signals.truncate(limit as usize);
        let next = if has_more {
            signals.last().expect("nonempty limited page").sequence
        } else {
            high_water
        };
        Ok(SignalPage {
            signals,
            next_cursor: format!("{prefix}{next}"),
            has_more,
        })
    }
}
