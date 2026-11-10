//! Non-secret metadata needed to exclude inherited control-session tools.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct InheritedMcpServer {
    pub name: String,
    pub transport: McpTransportKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum McpTransportKind {
    Stdio,
    StreamableHttp,
}
