use super::*;
use crate::protocol::{AccountQuotaGetParams, AccountQuotaResult, AccountQuotaUnavailableReason};

fn native_quota(used_percent: i32) -> NativeAccountQuotaRead {
    NativeAccountQuotaRead::Available(NativeAccountQuota {
        ordinary_usage_allowed: Some(true),
        buckets: vec![NativeAccountQuotaBucket {
            limit_id: "codex".to_owned(),
            limit_name: None,
            normal_model_slug: None,
            primary: Some(NativeAccountQuotaWindow {
                used_percent,
                window_duration_mins: Some(300),
                resets_at: Some(10),
            }),
            secondary: Some(NativeAccountQuotaWindow {
                used_percent: 39,
                window_duration_mins: Some(10_080),
                resets_at: Some(11),
            }),
            rate_limit_reached_type: None,
        }],
    })
}

#[tokio::test]
async fn account_quota_is_global_cached_and_invalidated_by_native_updates() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.worker.set_account_quota(native_quota(16));

    let first = fixture
        .coordinator
        .get_account_quota(AccountQuotaGetParams {})
        .await
        .unwrap();
    let AccountQuotaResult::Available { buckets, .. } = first else {
        panic!("account quota was not available");
    };
    assert_eq!(buckets[0].primary.as_ref().unwrap().used_percent, 16);

    fixture.worker.set_account_quota(native_quota(25));
    let cached = fixture
        .coordinator
        .get_account_quota(AccountQuotaGetParams {})
        .await
        .unwrap();
    let AccountQuotaResult::Available { buckets, .. } = cached else {
        panic!("cached account quota was not available");
    };
    assert_eq!(buckets[0].primary.as_ref().unwrap().used_percent, 16);
    assert_eq!(
        fixture
            .worker
            .calls()
            .iter()
            .filter(|call| matches!(call, WorkerCall::AccountQuota))
            .count(),
        1
    );

    fixture
        .coordinator
        .record_codex_event(CodexEvent::Notification {
            method: "account/rateLimits/updated".to_owned(),
            params: json!({"rateLimits": {"primary": {"usedPercent": 25}}}),
        })
        .unwrap();
    let refreshed = fixture
        .coordinator
        .get_account_quota(AccountQuotaGetParams {})
        .await
        .unwrap();
    let AccountQuotaResult::Available { buckets, .. } = refreshed else {
        panic!("refreshed account quota was not available");
    };
    assert_eq!(buckets[0].primary.as_ref().unwrap().used_percent, 25);
    assert_eq!(
        fixture
            .worker
            .calls()
            .iter()
            .filter(|call| matches!(call, WorkerCall::AccountQuota))
            .count(),
        2
    );
}

#[tokio::test]
async fn thread_usage_does_not_invalidate_the_independent_account_quota() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let thread_id = workspace.codex_thread_id.as_deref().unwrap();
    fixture.worker.set_account_quota(native_quota(16));
    fixture
        .coordinator
        .get_account_quota(AccountQuotaGetParams {})
        .await
        .unwrap();

    fixture.worker.set_account_quota(native_quota(25));
    fixture
        .coordinator
        .record_codex_event(CodexEvent::Notification {
            method: "thread/tokenUsage/updated".to_owned(),
            params: json!({
                "threadId": thread_id,
                "turnId": "turn-usage",
                "tokenUsage": {
                    "total": {
                        "totalTokens": 100,
                        "inputTokens": 90,
                        "cachedInputTokens": 20,
                        "outputTokens": 10,
                        "reasoningOutputTokens": 4
                    },
                    "last": {
                        "totalTokens": 40,
                        "inputTokens": 30,
                        "cachedInputTokens": 10,
                        "outputTokens": 10,
                        "reasoningOutputTokens": 4
                    },
                    "modelContextWindow": 200_000
                }
            }),
        })
        .unwrap();

    let cached = fixture
        .coordinator
        .get_account_quota(AccountQuotaGetParams {})
        .await
        .unwrap();
    let AccountQuotaResult::Available { buckets, .. } = cached else {
        panic!("cached account quota was not available");
    };
    assert_eq!(buckets[0].primary.as_ref().unwrap().used_percent, 16);
    assert_eq!(
        fixture
            .worker
            .calls()
            .iter()
            .filter(|call| matches!(call, WorkerCall::AccountQuota))
            .count(),
        1
    );
}

#[tokio::test]
async fn unsupported_account_auth_is_data_instead_of_a_status_failure() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture
        .worker
        .set_account_quota(NativeAccountQuotaRead::UnsupportedAuthentication);

    assert!(matches!(
        fixture
            .coordinator
            .get_account_quota(AccountQuotaGetParams {})
            .await
            .unwrap(),
        AccountQuotaResult::Unavailable {
            reason: AccountQuotaUnavailableReason::UnsupportedAuthentication,
            ..
        }
    ));
}
