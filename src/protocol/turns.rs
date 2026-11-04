use serde::{Deserialize, Serialize};

use super::{RepositoryScope, TurnInterruptAttempt, TurnInterruptMode, TurnTerminalStatus};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TurnGetParams {
    pub scope: RepositoryScope,
    pub workspace: String,
    pub operation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TurnInterruptParams {
    pub scope: RepositoryScope,
    pub workspace: String,
    pub operation_id: String,
    #[serde(default)]
    pub mode: TurnInterruptMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TurnObservation {
    pub workspace_id: String,
    pub operation_id: String,
    pub codex_thread_id: Option<String>,
    pub codex_turn_id: Option<String>,
    pub dispatch: TurnDispatchState,
    pub state: TurnObservationState,
    pub outcome: Option<TurnTerminalStatus>,
    pub outcome_observed_at_ms: Option<i64>,
    pub interrupt_requested_at_ms: Option<i64>,
    pub interrupt_attempt: Option<TurnInterruptAttempt>,
    pub unavailable_reason: Option<TurnUnavailableReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TurnDispatchState {
    Prepared,
    Dispatching,
    Accepted,
    Uncertain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TurnObservationState {
    NotStarted,
    Starting,
    Running,
    Finished,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TurnUnavailableReason {
    AcceptanceUnconfirmed,
    LegacyCorrelationMissing,
    CompletionNotObserved,
    NativeLookupFailed,
    NativeHistoryUnavailable,
    NativeLookupTimedOut,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TurnInterruptResult {
    pub turn: TurnObservation,
    pub interruption: TurnInterruptionState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TurnInterruptionState {
    Requested,
    Uncertain,
    Rejected,
    Confirmed,
    AlreadyFinished,
    NotRequested,
}

impl super::DaemonRequest for TurnGetParams {
    type Response = TurnObservation;
    const METHOD: super::DaemonMethod = super::DaemonMethod::TurnGet;
}

impl super::DaemonRequest for TurnInterruptParams {
    type Response = TurnInterruptResult;
    const METHOD: super::DaemonMethod = super::DaemonMethod::TurnInterrupt;
}
