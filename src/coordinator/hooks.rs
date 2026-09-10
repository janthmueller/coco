use super::{Coordinator, CoordinatorError};
use crate::domain::hooks::{HookDeliverySummary, HookRegistrySummary};
use crate::protocol::{HookDeliveryListParams, HookListParams, HookReloadParams};

impl Coordinator {
    pub(crate) fn list_hooks(&self, _params: HookListParams) -> HookRegistrySummary {
        self.hooks.summary()
    }

    pub(crate) fn reload_hooks(
        &self,
        _params: HookReloadParams,
    ) -> Result<HookRegistrySummary, CoordinatorError> {
        Ok(self.hooks.reload()?)
    }

    pub(crate) fn list_hook_deliveries(
        &self,
        params: HookDeliveryListParams,
    ) -> Result<Vec<HookDeliverySummary>, CoordinatorError> {
        if !(1..=100).contains(&params.limit) {
            return Err(CoordinatorError::InvalidParams(
                "hook history limit must be 1–100".into(),
            ));
        }
        Ok(self.store.list_hook_deliveries(params.limit)?)
    }
}
