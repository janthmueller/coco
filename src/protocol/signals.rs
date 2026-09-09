use super::{DaemonMethod, DaemonRequest, RepositoryScope};
use crate::domain::signals::{Signal, SignalPage, SignalType};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SignalCatalogLoadParams {
    pub repository: PathBuf,
    pub directory: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SignalTypeListParams {
    pub repository: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SignalEmitParams {
    pub repository: PathBuf,
    pub thread_id: String,
    pub name: String,
    pub version: u32,
    pub payload: Value,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SignalListParams {
    pub scope: RepositoryScope,
    pub workspace_id: Option<String>,
    pub name: Option<String>,
    pub after: Option<String>,
    pub limit: u32,
}

impl DaemonRequest for SignalCatalogLoadParams {
    type Response = Vec<SignalType>;
    const METHOD: DaemonMethod = DaemonMethod::SignalCatalogLoad;
}

impl DaemonRequest for SignalTypeListParams {
    type Response = Vec<SignalType>;
    const METHOD: DaemonMethod = DaemonMethod::SignalTypeList;
}

impl DaemonRequest for SignalEmitParams {
    type Response = Signal;
    const METHOD: DaemonMethod = DaemonMethod::SignalEmit;
}

impl DaemonRequest for SignalListParams {
    type Response = SignalPage;
    const METHOD: DaemonMethod = DaemonMethod::SignalList;
}
