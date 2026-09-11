use serde::{Deserialize, Serialize};
use thiserror::Error;

pub(crate) const WORKSPACE_TOKEN_USAGE_SCHEMA_VERSION: u32 = 1;

/// One native Codex token breakdown.
///
/// Cached input is a subset of input and reasoning output is a subset of
/// output. Consumers must therefore use `total_tokens` as supplied rather
/// than summing every field.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TokenUsageBreakdown {
    pub(crate) total_tokens: u64,
    pub(crate) input_tokens: u64,
    pub(crate) cached_input_tokens: u64,
    #[serde(default)]
    pub(crate) cache_write_input_tokens: u64,
    pub(crate) output_tokens: u64,
    pub(crate) reasoning_output_tokens: u64,
}

/// Durable checkpoint of a cumulative native App Server usage notification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspaceTokenUsageCheckpoint {
    pub(crate) schema_version: u32,
    pub(crate) thread_id: String,
    pub(crate) turn_id: String,
    pub(crate) total: TokenUsageBreakdown,
    pub(crate) last: TokenUsageBreakdown,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) model_context_window: Option<u64>,
    pub(crate) runtime_generation: String,
    pub(crate) observed_at_ms: i64,
}

impl WorkspaceTokenUsageCheckpoint {
    pub(crate) fn validate(&self) -> Result<(), WorkspaceUsageError> {
        if self.schema_version != WORKSPACE_TOKEN_USAGE_SCHEMA_VERSION {
            return Err(WorkspaceUsageError::UnsupportedSchema(self.schema_version));
        }
        if self.thread_id.trim().is_empty() {
            return Err(WorkspaceUsageError::EmptyIdentity("threadId"));
        }
        if self.turn_id.trim().is_empty() {
            return Err(WorkspaceUsageError::EmptyIdentity("turnId"));
        }
        if self.runtime_generation.trim().is_empty() {
            return Err(WorkspaceUsageError::EmptyIdentity("runtimeGeneration"));
        }
        if self.model_context_window == Some(0) {
            return Err(WorkspaceUsageError::ZeroContextWindow);
        }
        Ok(())
    }
}

/// Optional billing estimate returned by Codex for one native thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NativeThreadCostEstimate {
    pub(crate) thread_id: String,
    pub(crate) estimated_usage_credits_micros: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) estimated_usage_usd_micros: Option<u64>,
    pub(crate) groups: Vec<NativeThreadCostGroup>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NativeThreadCostGroup {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) reasoning_effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) speed: Option<String>,
    pub(crate) estimated_usage_credits_micros: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) net_new_input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) cached_input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) total_tokens: Option<u64>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub(crate) enum WorkspaceUsageError {
    #[error("unsupported workspace token usage schema version {0}")]
    UnsupportedSchema(u32),
    #[error("workspace token usage {0} must not be empty")]
    EmptyIdentity(&'static str),
    #[error("workspace token usage modelContextWindow must be greater than zero")]
    ZeroContextWindow,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_validation_rejects_ambiguous_provenance() {
        let mut checkpoint = WorkspaceTokenUsageCheckpoint {
            schema_version: WORKSPACE_TOKEN_USAGE_SCHEMA_VERSION,
            thread_id: "thread-1".to_owned(),
            turn_id: "turn-1".to_owned(),
            total: TokenUsageBreakdown::default(),
            last: TokenUsageBreakdown::default(),
            model_context_window: Some(200_000),
            runtime_generation: "runtime-1".to_owned(),
            observed_at_ms: 1,
        };
        assert_eq!(checkpoint.validate(), Ok(()));

        checkpoint.model_context_window = Some(0);
        assert_eq!(
            checkpoint.validate(),
            Err(WorkspaceUsageError::ZeroContextWindow)
        );
    }
}
