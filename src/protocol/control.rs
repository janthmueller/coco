use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{DaemonMethod, DaemonRequest};
pub(crate) use crate::domain::control::{InheritedMcpServer, McpTransportKind};

pub(crate) const CONTROL_CODEX_VERSION: &str = "0.166.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ControlSessionPrepareParams {
    pub repository_path: PathBuf,
}

/// Non-secret launch information only, never native configuration recipes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ControlSessionPreparation {
    pub working_directory: PathBuf,
    pub default_repository_id: Option<String>,
    pub repositories: Vec<ControlRepository>,
    pub unavailable_repositories: Vec<PathBuf>,
    pub codex_home: PathBuf,
    pub coco_version: String,
    pub mcp_servers: Vec<InheritedMcpServer>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ControlRepository {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
}

impl DaemonRequest for ControlSessionPrepareParams {
    type Response = ControlSessionPreparation;
    const METHOD: DaemonMethod = DaemonMethod::ControlSessionPrepare;
}
