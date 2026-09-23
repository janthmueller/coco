use std::time::{Duration, Instant};

use chrono::Utc;
use tracing::debug;

use super::{Coordinator, CoordinatorError, NativeAccountQuota, NativeAccountQuotaRead};
use crate::protocol::{
    AccountQuotaBucket, AccountQuotaGetParams, AccountQuotaResult, AccountQuotaUnavailableReason,
    AccountQuotaWindow,
};

const ACCOUNT_QUOTA_CACHE_TTL: Duration = Duration::from_secs(15);

impl Coordinator {
    pub(crate) async fn get_account_quota(
        &self,
        _params: AccountQuotaGetParams,
    ) -> Result<AccountQuotaResult, CoordinatorError> {
        let now = Instant::now();
        if let Some((checked_at, quota)) = self
            .account_quota
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            && now.duration_since(*checked_at) < ACCOUNT_QUOTA_CACHE_TTL
        {
            return Ok(quota.clone());
        }

        let checked_at_ms = Utc::now().timestamp_millis();
        let quota = match self.worker.read_account_quota().await {
            Ok(NativeAccountQuotaRead::Available(quota)) => {
                account_quota_result(quota, checked_at_ms)
            }
            Ok(NativeAccountQuotaRead::UnsupportedAuthentication) => {
                AccountQuotaResult::Unavailable {
                    reason: AccountQuotaUnavailableReason::UnsupportedAuthentication,
                    checked_at_ms,
                }
            }
            Ok(NativeAccountQuotaRead::UnsupportedServer) => AccountQuotaResult::Unavailable {
                reason: AccountQuotaUnavailableReason::UnsupportedServer,
                checked_at_ms,
            },
            Ok(NativeAccountQuotaRead::NotReported) => AccountQuotaResult::Unavailable {
                reason: AccountQuotaUnavailableReason::NotReported,
                checked_at_ms,
            },
            Err(source) => {
                debug!(%source, "Codex account quota is unavailable");
                AccountQuotaResult::Unavailable {
                    reason: AccountQuotaUnavailableReason::ReadFailed,
                    checked_at_ms,
                }
            }
        };
        *self
            .account_quota
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some((now, quota.clone()));
        Ok(quota)
    }

    pub(super) fn invalidate_account_quota(&self) {
        self.account_quota
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
    }
}

fn account_quota_result(quota: NativeAccountQuota, observed_at_ms: i64) -> AccountQuotaResult {
    AccountQuotaResult::Available {
        ordinary_usage_allowed: quota.ordinary_usage_allowed,
        buckets: quota
            .buckets
            .into_iter()
            .map(|bucket| AccountQuotaBucket {
                limit_id: bucket.limit_id,
                limit_name: bucket.limit_name,
                normal_model_slug: bucket.normal_model_slug,
                primary: bucket.primary.map(|window| AccountQuotaWindow {
                    used_percent: window.used_percent,
                    window_duration_mins: window.window_duration_mins,
                    resets_at: window.resets_at,
                }),
                secondary: bucket.secondary.map(|window| AccountQuotaWindow {
                    used_percent: window.used_percent,
                    window_duration_mins: window.window_duration_mins,
                    resets_at: window.resets_at,
                }),
                rate_limit_reached_type: bucket.rate_limit_reached_type,
            })
            .collect(),
        observed_at_ms,
    }
}
