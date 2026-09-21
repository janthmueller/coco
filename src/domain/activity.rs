use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum WorkspaceActivitySource {
    ContextCompaction,
    ReasoningSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspaceActivity {
    pub label: String,
    pub source: WorkspaceActivitySource,
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub truncated: bool,
    pub runtime_generation: String,
    pub observed_at_ms: i64,
}
