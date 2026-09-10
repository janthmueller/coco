use serde::{Deserialize, Serialize};

use super::{DaemonMethod, DaemonRequest};
use crate::domain::hooks::{HookDeliverySummary, HookRegistrySummary};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HookListParams {}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HookReloadParams {}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct HookDeliveryListParams {
    pub limit: u32,
}

impl DaemonRequest for HookListParams {
    type Response = HookRegistrySummary;
    const METHOD: DaemonMethod = DaemonMethod::HookList;
}

impl DaemonRequest for HookReloadParams {
    type Response = HookRegistrySummary;
    const METHOD: DaemonMethod = DaemonMethod::HookReload;
}

impl DaemonRequest for HookDeliveryListParams {
    type Response = Vec<HookDeliverySummary>;
    const METHOD: DaemonMethod = DaemonMethod::HookDeliveryList;
}
