use crate::domain::WorkspacePhase;
use crate::protocol::{WorkspaceListItem, WorkspaceUsageItem};
use serde_json::json;

use super::filter::StateFilter;
use super::sorting::SortPlan;

#[path = "tests/filtering.rs"]
mod filtering;
#[path = "tests/metrics.rs"]
mod metrics;
#[path = "tests/ordering.rs"]
mod ordering;

fn row(repository: &str, name: &str, phase: WorkspacePhase) -> WorkspaceListItem {
    serde_json::from_value(json!({
        "id":format!("{repository}-{name}"), "repositoryId":repository, "name":name,
        "contextMode":"fresh", "context":{},
        "profile":{"name":"default","sourceHash":"test","effectiveSettings":{}},
        "lifecycle":"ready", "availability":"open", "phase":phase,
        "waitReasons":[], "worktreeMode":"new_branch", "branchName":format!("coco/{name}"),
        "createdAtMs":1, "updatedAtMs":1, "threadArchived":false,
        "repository":{"id":repository,"displayName":repository,"rootPath":format!("/repos/{repository}")}
    })).unwrap()
}

fn resources(
    row: &mut WorkspaceListItem,
    memory: Option<u64>,
    cpu: Option<f64>,
    procs: Option<u32>,
) {
    use crate::domain::runtime::*;
    row.runtime_resources = Some(WorkspaceRuntimeResources {
        backend: WorkspaceRuntimeBackend::ExecServer,
        state: WorkspaceRuntimeState::Running,
        scope: WorkspaceResourceScope::CgroupV2,
        process_id: None,
        process_count: procs,
        task_count: None,
        resident_memory_bytes: Some(7),
        memory_current_bytes: memory,
        cpu_percent: cpu,
        cpu_usage_usec: None,
        cgroup_unit: None,
        events: None,
        sampled_at_ms: None,
    });
}

fn usage(
    row: &WorkspaceListItem,
    total: u64,
    current: u64,
    window: Option<u64>,
) -> WorkspaceUsageItem {
    serde_json::from_value(json!({
        "workspace":row.workspace, "repository":row.repository,
        "tokens":{
            "schemaVersion":1,"threadId":"test-thread","turnId":"test-turn",
            "runtimeGeneration":"test-generation","observedAtMs":1,
            "source":"threadTokenUsageUpdated","isFresh":false,"modelContextWindow":window,
            "total":{"totalTokens":total,"inputTokens":0,"cachedInputTokens":0,"outputTokens":0,"reasoningOutputTokens":0},
            "last":{"totalTokens":current,"inputTokens":0,"cachedInputTokens":0,"outputTokens":0,"reasoningOutputTokens":0}
        },
        "cost":{"status":"unavailable","reason":"notReported"}
    })).unwrap()
}

fn sort(rows: &mut [WorkspaceListItem], keys: &[&str], usage: Option<&[WorkspaceUsageItem]>) {
    SortPlan::new(
        &keys
            .iter()
            .map(|key| key.parse().unwrap())
            .collect::<Vec<_>>(),
        true,
        true,
    )
    .unwrap()
    .apply(rows, usage);
}

fn names(rows: &[WorkspaceListItem]) -> Vec<&str> {
    rows.iter().map(|row| row.workspace.name.as_str()).collect()
}
