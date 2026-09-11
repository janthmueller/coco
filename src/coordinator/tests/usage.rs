use super::*;
use crate::protocol::{
    WorkspaceCostEstimate, WorkspaceCostUnavailableReason, WorkspaceUsageGetParams,
    WorkspaceUsageListParams,
};

fn notification(thread_id: &str, turn_id: &str, total_tokens: u64) -> CodexEvent {
    CodexEvent::Notification {
        method: "thread/tokenUsage/updated".to_owned(),
        params: json!({
            "threadId": thread_id,
            "turnId": turn_id,
            "tokenUsage": {
                "total": {
                    "totalTokens": total_tokens,
                    "inputTokens": total_tokens.saturating_sub(10),
                    "cachedInputTokens": 20,
                    "cacheWriteInputTokens": 3,
                    "outputTokens": 10,
                    "reasoningOutputTokens": 4
                },
                "last": {
                    "totalTokens": 40_000,
                    "inputTokens": 39_000,
                    "cachedInputTokens": 10_000,
                    "outputTokens": 1_000,
                    "reasoningOutputTokens": 400
                },
                "modelContextWindow": 200_000
            }
        }),
    }
}

#[tokio::test]
async fn native_usage_is_persisted_without_loading_or_sampling_the_workspace() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let thread_id = workspace.codex_thread_id.as_deref().unwrap();
    fixture
        .coordinator
        .record_codex_event(notification(thread_id, "turn-usage", 123_456))
        .unwrap();
    let call_count = fixture.worker.calls().len();

    let usage = fixture
        .coordinator
        .get_workspace_usage(WorkspaceUsageGetParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.name.clone(),
        })
        .await
        .unwrap();
    let tokens = usage.tokens.unwrap();
    assert_eq!(tokens.checkpoint.thread_id, thread_id);
    assert_eq!(tokens.checkpoint.turn_id, "turn-usage");
    assert_eq!(tokens.checkpoint.total.total_tokens, 123_456);
    assert_eq!(tokens.checkpoint.last.total_tokens, 40_000);
    assert_eq!(tokens.checkpoint.model_context_window, Some(200_000));
    assert!(tokens.is_fresh);
    assert!(matches!(
        usage.cost,
        WorkspaceCostEstimate::Unavailable {
            reason: WorkspaceCostUnavailableReason::NotReported,
            ..
        }
    ));
    assert_eq!(
        &fixture.worker.calls()[call_count..],
        &[WorkerCall::Cost {
            thread_id: thread_id.to_owned()
        }]
    );
}

#[tokio::test]
async fn cumulative_usage_never_regresses_and_is_stale_after_restart() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let thread_id = workspace.codex_thread_id.as_deref().unwrap();
    fixture
        .coordinator
        .record_codex_event(notification(thread_id, "turn-new", 200))
        .unwrap();
    fixture
        .coordinator
        .record_codex_event(notification(thread_id, "turn-old", 100))
        .unwrap();

    let restarted = fixture.recovery_coordinator(Arc::new(FakeWorker::default()), "runtime-next");
    let usage = restarted
        .get_workspace_usage(WorkspaceUsageGetParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id,
        })
        .await
        .unwrap();
    let tokens = usage.tokens.unwrap();
    assert_eq!(tokens.checkpoint.total.total_tokens, 200);
    assert_eq!(tokens.checkpoint.turn_id, "turn-new");
    assert!(!tokens.is_fresh);
}

#[tokio::test]
async fn native_cost_is_optional_cached_and_invalidated_by_new_usage() {
    let fixture = Fixture::new(FakeWorker::default());
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    let thread_id = workspace.codex_thread_id.clone().unwrap();
    fixture.worker.set_thread_cost(cost(&thread_id, 1_000_000));

    let params = WorkspaceUsageGetParams {
        scope: RepositoryScope::repository(fixture.source.clone()),
        workspace: workspace.id.clone(),
    };
    let first = fixture
        .coordinator
        .get_workspace_usage(params.clone())
        .await
        .unwrap();
    assert_cost(&first.cost, 1_000_000);
    fixture.worker.set_thread_cost(cost(&thread_id, 2_000_000));
    let cached = fixture
        .coordinator
        .get_workspace_usage(params.clone())
        .await
        .unwrap();
    assert_cost(&cached.cost, 1_000_000);

    fixture
        .coordinator
        .record_codex_event(notification(&thread_id, "turn-next", 300))
        .unwrap();
    let refreshed = fixture
        .coordinator
        .get_workspace_usage(params)
        .await
        .unwrap();
    assert_cost(&refreshed.cost, 2_000_000);
    assert_eq!(
        fixture
            .worker
            .calls()
            .iter()
            .filter(|call| matches!(call, WorkerCall::Cost { .. }))
            .count(),
        2
    );

    let listed = fixture
        .coordinator
        .list_workspace_usage(WorkspaceUsageListParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
        })
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert_cost(&listed[0].cost, 2_000_000);
}

fn cost(thread_id: &str, credits: u64) -> NativeThreadCostEstimate {
    NativeThreadCostEstimate {
        thread_id: thread_id.to_owned(),
        estimated_usage_credits_micros: credits,
        estimated_usage_usd_micros: Some(credits / 2),
        groups: vec![NativeThreadCostGroup {
            model: Some("gpt-test".to_owned()),
            reasoning_effort: Some("medium".to_owned()),
            speed: None,
            estimated_usage_credits_micros: credits,
            net_new_input_tokens: Some(100),
            cached_input_tokens: Some(50),
            input_tokens: Some(150),
            output_tokens: Some(25),
            total_tokens: Some(175),
        }],
    }
}

fn assert_cost(cost: &WorkspaceCostEstimate, expected: u64) {
    assert!(matches!(
        cost,
        WorkspaceCostEstimate::Available {
            estimated_usage_credits_micros,
            estimated_usage_usd_micros: Some(_),
            ..
        } if *estimated_usage_credits_micros == expected
    ));
}
