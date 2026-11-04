use std::collections::BTreeMap;

use super::result::{MAX_TURN_RESPONSE_BYTES, bounded_response};
use super::{ObservedAgentOutput, TurnTerminalStatus};

const MAX_PENDING_TURNS: usize = 32;
const MAX_PENDING_TURN_ID_BYTES: usize = 256;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct PendingTurnObservation {
    pub(super) completion: Option<TurnTerminalStatus>,
    pub(super) output: Option<ObservedAgentOutput>,
    conflicted: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct PendingTurnObservations {
    entries: BTreeMap<String, PendingTurnObservation>,
    response_bytes: usize,
    overflowed: bool,
}

impl PendingTurnObservations {
    fn entry(&mut self, turn: &str) -> Option<&mut PendingTurnObservation> {
        if turn.len() > MAX_PENDING_TURN_ID_BYTES
            || (!self.entries.contains_key(turn) && self.entries.len() == MAX_PENDING_TURNS)
        {
            self.overflowed = true;
            return None;
        }
        Some(self.entries.entry(turn.to_owned()).or_default())
    }

    pub(super) fn observe_completion(&mut self, turn: &str, status: TurnTerminalStatus) {
        let Some(entry) = self.entry(turn) else {
            return;
        };
        if entry.completion.is_some_and(|previous| previous != status) {
            entry.conflicted = true;
        }
        if entry.conflicted {
            entry.completion = None;
        } else {
            entry.completion = Some(status);
        }
    }

    pub(super) fn observe_output(&mut self, turn: &str, response: &str) {
        let previous_bytes = self
            .entries
            .get(turn)
            .and_then(|entry| entry.output.as_ref())
            .map_or(0, |output| output.response.len());
        let budget = MAX_TURN_RESPONSE_BYTES - self.response_bytes + previous_bytes;
        let Some(entry) = self.entry(turn) else {
            return;
        };
        let (response, truncated) = bounded_response(response, budget);
        let retained = response.len();
        entry.output = Some(ObservedAgentOutput {
            native_turn_id: turn.to_owned(),
            response,
            truncated,
        });
        self.response_bytes = self.response_bytes - previous_bytes + retained;
    }

    pub(super) fn completion(&self, turn: &str) -> Option<TurnTerminalStatus> {
        self.entries.get(turn).and_then(|entry| entry.completion)
    }

    pub(super) fn resolve(mut self, turn: &str) -> (PendingTurnObservation, bool) {
        let entry = self.entries.remove(turn);
        let lost = entry
            .as_ref()
            .map_or(self.overflowed, |entry| entry.conflicted);
        (entry.unwrap_or_default(), lost)
    }
}

#[cfg(test)]
mod tests;
