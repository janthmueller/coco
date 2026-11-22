use serde::{Deserialize, Serialize};

/// An advisory comparison with a configured local upstream ref, not baseSha.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitTracking {
    pub state: GitTrackingState,
    pub upstream: Option<String>,
    pub remote: Option<String>,
    pub ahead_by: Option<u64>,
    pub behind_by: Option<u64>,
    pub observed_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GitTrackingState {
    InSync,
    Ahead,
    Behind,
    Diverged,
    NoUpstream,
    UpstreamMissing,
    Detached,
    Unknown,
}

impl GitTracking {
    pub(crate) fn unknown() -> Self {
        Self::without_counts(GitTrackingState::Unknown)
    }

    pub(crate) fn without_counts(state: GitTrackingState) -> Self {
        Self {
            state,
            upstream: None,
            remote: None,
            ahead_by: None,
            behind_by: None,
            observed_at_ms: None,
        }
    }

    pub(crate) fn counts_state(ahead: u64, behind: u64) -> GitTrackingState {
        match (ahead, behind) {
            (0, 0) => GitTrackingState::InSync,
            (_, 0) => GitTrackingState::Ahead,
            (0, _) => GitTrackingState::Behind,
            _ => GitTrackingState::Diverged,
        }
    }
}
