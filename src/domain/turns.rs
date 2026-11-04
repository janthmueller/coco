use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TurnInterruptMode {
    #[default]
    Strict,
    BestEffort,
}

impl TurnInterruptMode {
    pub const ALL: [Self; 2] = [Self::Strict, Self::BestEffort];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::BestEffort => "bestEffort",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.as_str() == value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TurnInterruptAttemptState {
    Uncertain,
    Requested,
    Rejected,
}

impl TurnInterruptAttemptState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Uncertain => "uncertain",
            Self::Requested => "requested",
            Self::Rejected => "rejected",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "uncertain" => Some(Self::Uncertain),
            "requested" => Some(Self::Requested),
            "rejected" => Some(Self::Rejected),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TurnInterruptAttempt {
    pub mode: TurnInterruptMode,
    pub state: TurnInterruptAttemptState,
}

/// A native terminal outcome, not workspace readiness or a business verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TurnTerminalStatus {
    Completed,
    Interrupted,
    Failed,
}

impl TurnTerminalStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Interrupted => "interrupted",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "completed" => Some(Self::Completed),
            "interrupted" => Some(Self::Interrupted),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}
