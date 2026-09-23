use crate::protocol::{
    AccountQuotaBucket, AccountQuotaResult, AccountQuotaUnavailableReason, AccountQuotaWindow,
};

use super::super::style::{Palette, Tone};

pub(super) fn render_account_quota(quota: &AccountQuotaResult, palette: Palette) -> String {
    let label = palette.paint(Tone::Bold, "Quota");
    match quota {
        AccountQuotaResult::Available {
            ordinary_usage_allowed,
            buckets,
            ..
        } => {
            let windows = preferred_bucket(buckets)
                .map(bucket_windows)
                .unwrap_or_default();
            let mut parts = Vec::with_capacity(windows.len() + 1);
            if *ordinary_usage_allowed == Some(false) {
                parts.push(palette.paint(Tone::RedBold, "blocked"));
            }
            parts.extend(
                windows
                    .into_iter()
                    .map(|(window, secondary)| render_window(window, secondary, palette)),
            );
            if parts.is_empty() {
                parts.push(palette.paint(Tone::Dim, "unavailable"));
            }
            format!(
                "{label}  {}\n",
                parts.join(&palette.paint(Tone::Dim, " · "))
            )
        }
        AccountQuotaResult::Unavailable { reason, .. } => {
            let detail = match reason {
                AccountQuotaUnavailableReason::UnsupportedAuthentication => {
                    "unavailable · ChatGPT login required"
                }
                AccountQuotaUnavailableReason::UnsupportedServer => "unavailable · update Codex",
                AccountQuotaUnavailableReason::NotReported
                | AccountQuotaUnavailableReason::ReadFailed => "unavailable",
            };
            format!("{label}  {}\n", palette.paint(Tone::Dim, detail))
        }
    }
}

fn preferred_bucket(buckets: &[AccountQuotaBucket]) -> Option<&AccountQuotaBucket> {
    buckets
        .iter()
        .find(|bucket| bucket.limit_id == "codex" && bucket_has_windows(bucket))
        .or_else(|| buckets.iter().find(|bucket| bucket_has_windows(bucket)))
}

fn bucket_has_windows(bucket: &AccountQuotaBucket) -> bool {
    bucket.primary.is_some() || bucket.secondary.is_some()
}

fn bucket_windows(bucket: &AccountQuotaBucket) -> Vec<(&AccountQuotaWindow, bool)> {
    bucket
        .primary
        .iter()
        .map(|window| (window, false))
        .chain(bucket.secondary.iter().map(|window| (window, true)))
        .collect()
}

fn render_window(window: &AccountQuotaWindow, secondary: bool, palette: Palette) -> String {
    let remaining = (100 - window.used_percent).clamp(0, 100);
    let tone = if remaining == 0 {
        Tone::RedBold
    } else if remaining <= 20 {
        Tone::YellowBold
    } else {
        Tone::Primary
    };
    palette.paint(
        tone,
        format!(
            "{} {remaining}% left",
            window_label(window.window_duration_mins, secondary)
        ),
    )
}

fn window_label(window_minutes: Option<i64>, secondary: bool) -> &'static str {
    const MINUTES_PER_HOUR: i64 = 60;
    const MINUTES_PER_DAY: i64 = 24 * MINUTES_PER_HOUR;
    let expected = [
        (5 * MINUTES_PER_HOUR, "5h"),
        (MINUTES_PER_DAY, "daily"),
        (7 * MINUTES_PER_DAY, "weekly"),
        (30 * MINUTES_PER_DAY, "monthly"),
        (365 * MINUTES_PER_DAY, "annual"),
    ];
    if let Some(window_minutes) = window_minutes {
        for (expected_minutes, label) in expected {
            if approximately(window_minutes, expected_minutes) {
                return label;
            }
        }
    }
    if secondary {
        "secondary usage"
    } else {
        "usage"
    }
}

fn approximately(actual: i64, expected: i64) -> bool {
    let actual = actual.max(0) as f64;
    let expected = expected as f64;
    actual >= expected * 0.95 && actual <= expected * 1.05
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quota(ordinary_usage_allowed: Option<bool>) -> AccountQuotaResult {
        AccountQuotaResult::Available {
            ordinary_usage_allowed,
            buckets: vec![AccountQuotaBucket {
                limit_id: "codex".to_owned(),
                limit_name: None,
                normal_model_slug: None,
                primary: Some(AccountQuotaWindow {
                    used_percent: 16,
                    window_duration_mins: Some(300),
                    resets_at: Some(1),
                }),
                secondary: Some(AccountQuotaWindow {
                    used_percent: 39,
                    window_duration_mins: Some(10_080),
                    resets_at: Some(2),
                }),
                rate_limit_reached_type: None,
            }],
            observed_at_ms: 3,
        }
    }

    #[test]
    fn renders_native_windows_as_remaining_account_quota() {
        assert_eq!(
            render_account_quota(&quota(Some(true)), Palette::plain()),
            "Quota  5h 84% left · weekly 61% left\n"
        );
    }

    #[test]
    fn preserves_authoritative_blocked_state_and_clamps_percentages() {
        let mut quota = quota(Some(false));
        let AccountQuotaResult::Available { buckets, .. } = &mut quota else {
            unreachable!();
        };
        buckets[0].primary.as_mut().unwrap().used_percent = 140;
        assert_eq!(
            render_account_quota(&quota, Palette::plain()),
            "Quota  blocked · 5h 0% left · weekly 61% left\n"
        );
    }

    #[test]
    fn explains_auth_and_server_unavailability_without_failing_status() {
        assert_eq!(
            render_account_quota(
                &AccountQuotaResult::Unavailable {
                    reason: AccountQuotaUnavailableReason::UnsupportedAuthentication,
                    checked_at_ms: 1,
                },
                Palette::plain(),
            ),
            "Quota  unavailable · ChatGPT login required\n"
        );
        assert_eq!(
            render_account_quota(
                &AccountQuotaResult::Unavailable {
                    reason: AccountQuotaUnavailableReason::UnsupportedServer,
                    checked_at_ms: 1,
                },
                Palette::plain(),
            ),
            "Quota  unavailable · update Codex\n"
        );
    }

    #[test]
    fn collection_json_keeps_account_quota_at_the_top_level() {
        let value =
            super::super::status_collection_json(&[], None, Some(&quota(Some(true)))).unwrap();
        assert_eq!(value["schemaVersion"], 13);
        assert_eq!(value["workspaces"], serde_json::json!([]));
        assert_eq!(value["accountQuota"]["status"], "available");
        assert!(value.pointer("/workspaces/0/accountQuota").is_none());
    }
}
