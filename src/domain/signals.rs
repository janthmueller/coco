use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub(crate) const MAX_SIGNAL_BYTES: usize = 16 * 1024;
pub(crate) const MAX_SIGNAL_TYPES: usize = 128;
pub(crate) const SIGNAL_RETENTION: i64 = 10_000;
pub(crate) const MAX_SIGNAL_PAGE: u32 = 100;

pub(crate) fn valid_signal_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.starts_with(|character: char| character.is_ascii_lowercase())
        && name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SignalType {
    pub repository_id: String,
    pub name: String,
    pub version: u32,
    pub description: String,
    pub payload_schema: Option<Value>,
    pub registered_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Signal {
    pub id: String,
    pub sequence: i64,
    pub repository_id: String,
    pub repository_name: String,
    pub workspace_id: String,
    pub workspace_name: String,
    pub thread_id: String,
    pub name: String,
    pub version: u32,
    pub payload: Value,
    pub idempotency_key: String,
    pub occurred_at_ms: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub(crate) struct SignalFilter {
    pub repository_id: Option<String>,
    pub workspace_id: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SignalPage {
    pub signals: Vec<Signal>,
    pub next_cursor: String,
    pub has_more: bool,
}

#[derive(Debug, Error)]
pub(crate) enum SignalError {
    #[error("signal type/version is not registered")]
    UnknownType,
    #[error(
        "signal {name}@{version} is immutable; use a new version filename for changed definitions"
    )]
    VersionConflict { name: String, version: u32 },
    #[error("repository already has the maximum of 128 signal type versions")]
    CatalogFull,
    #[error("signal payload does not match its schema at {0}")]
    InvalidPayload(String),
    #[error("signal requires a Codex thread bound to an open CoCo workspace in this repository")]
    InvalidSender,
    #[error("signal idempotency key was already used with different content")]
    IdempotencyConflict,
    #[error("workspace signal rate limit reached (10 new signals per second)")]
    RateLimited,
    #[error("signal cursor is invalid or belongs to another stream/filter")]
    InvalidCursor,
    #[error(
        "signal cursor has expired; explicitly read without a cursor to restart at retained history"
    )]
    ExpiredCursor,
}

impl SignalError {
    pub(crate) const fn code(&self) -> &'static str {
        match self {
            Self::UnknownType => "SIGNAL_TYPE_NOT_FOUND",
            Self::VersionConflict { .. } => "SIGNAL_VERSION_CONFLICT",
            Self::CatalogFull => "SIGNAL_CATALOG_FULL",
            Self::InvalidPayload(_) => "SIGNAL_PAYLOAD_INVALID",
            Self::InvalidSender => "SIGNAL_SENDER_INVALID",
            Self::IdempotencyConflict => "IDEMPOTENCY_CONFLICT",
            Self::RateLimited => "SIGNAL_RATE_LIMITED",
            Self::InvalidCursor => "SIGNAL_CURSOR_INVALID",
            Self::ExpiredCursor => "SIGNAL_CURSOR_EXPIRED",
        }
    }
}
