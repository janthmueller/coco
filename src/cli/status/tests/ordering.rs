use super::*;
use WorkspacePhase::*;

#[test]
fn explicit_state_wins_globally_and_repo_grouping_is_opt_in() {
    let mut rows = [
        row("alpha", "ready", Idle),
        row("beta", "waiting", WaitingForInput),
    ];
    sort(&mut rows, &["state"], None);
    assert_eq!(names(&rows), ["waiting", "ready"]);
    sort(&mut rows, &["repo", "state"], None);
    assert_eq!(names(&rows), ["ready", "waiting"]);
}

#[test]
fn requested_secondary_keys_precede_default_ties_and_defaults_stay_repo_name() {
    let mut rows = [row("alpha", "z", Idle), row("beta", "a", Idle)];
    sort(&mut rows, &["state", "name"], None);
    assert_eq!(names(&rows), ["a", "z"]);
    super::super::sorting::sort_workspace_collection(&mut rows);
    assert_eq!(names(&rows), ["z", "a"]);
    sort(&mut rows, &["name"], None);
    assert_eq!(names(&rows), ["a", "z"]);
}

#[test]
fn repo_sort_uses_the_visible_path_and_natural_name_sort_uses_numeric_components() {
    let mut rows = [row("alpha", "w10", Idle), row("beta", "w2", Idle)];
    rows[0].repository.root_path = "/z/path".into();
    rows[1].repository.root_path = "/a/path".into();
    sort(&mut rows, &["repo"], None);
    assert_eq!(names(&rows), ["w2", "w10"]);
    sort(&mut rows, &["name"], None);
    assert_eq!(names(&rows), ["w2", "w10"]);
    sort(&mut rows, &["-name"], None);
    assert_eq!(names(&rows), ["w10", "w2"]);
}

#[test]
fn distinct_states_stay_together_within_attention_classes() {
    let phases = [
        NotLoaded,
        Prepared,
        Completed,
        Idle,
        Deleting,
        Reopening,
        Closing,
        Starting,
        Provisioning,
        Active,
        Failed,
        Unavailable,
        SystemError,
        WaitingForInput,
        WaitingForApproval,
    ];
    let mut rows = phases
        .into_iter()
        .map(|phase| row("repo", phase.as_str(), phase))
        .collect::<Vec<_>>();
    sort(&mut rows, &["state"], None);
    assert_eq!(
        rows.iter()
            .map(|row| row.workspace.phase)
            .collect::<Vec<_>>(),
        phases.into_iter().rev().collect::<Vec<_>>()
    );
    let before = names(&rows)
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    sort(&mut rows, &["-state"], None);
    assert_eq!(
        names(&rows),
        before.iter().rev().map(String::as_str).collect::<Vec<_>>()
    );
}

#[test]
fn numeric_keys_use_raw_values_and_missing_values_stay_last_in_both_directions() {
    use crate::domain::runtime::WorkspaceRuntimeState;
    let mut rows = [
        row("repo", "small", Idle),
        row("repo", "large", Idle),
        row("repo", "unknown", Idle),
        row("repo", "inactive", Idle),
    ];
    resources(&mut rows[0], Some(900), Some(9.0), Some(2));
    resources(&mut rows[1], Some(2048), Some(80.0), Some(10));
    resources(&mut rows[3], Some(u64::MAX), Some(100.0), Some(100));
    rows[3].runtime_resources.as_mut().unwrap().state = WorkspaceRuntimeState::Inactive;
    for field in ["memory", "cpu", "procs"] {
        sort(&mut rows, &[field], None);
        assert_eq!(&names(&rows)[..2], ["small", "large"]);
        sort(&mut rows, &[&format!("-{field}")], None);
        assert_eq!(&names(&rows)[..2], ["large", "small"]);
        assert!(names(&rows)[2..].contains(&"unknown"));
    }
}

#[test]
fn usage_is_joined_by_id_context_uses_ratio_and_live_phase_is_not_replaced() {
    let mut rows = [
        row("repo", "a", Active),
        row("repo", "b", Idle),
        row("repo", "missing", Prepared),
    ];
    let mut data = [
        usage(&rows[1], 2000, 90, Some(100)),
        usage(&rows[0], 900, 100, Some(1000)),
    ];
    data[0].workspace.phase = WaitingForInput; // Stored usage metadata is not live state.
    sort(&mut rows, &["-tokens"], Some(&data));
    assert_eq!(names(&rows), ["b", "a", "missing"]);
    sort(&mut rows, &["-context"], Some(&data));
    assert_eq!(names(&rows), ["b", "a", "missing"]);
    sort(&mut rows, &["state", "-tokens"], Some(&data));
    assert_eq!(names(&rows), ["a", "b", "missing"]);
}

#[test]
fn model_uses_reported_identifiers_and_unknown_models_stay_last() {
    use crate::domain::{CodexThreadStatus, ThreadRuntimeSnapshot};
    let mut rows = [
        row("repo", "unknown", Prepared),
        row("repo", "new", Idle),
        row("repo", "old", Idle),
    ];
    for (index, model) in [
        (1, "future-provider/model10"),
        (2, "future-provider/model2"),
    ] {
        rows[index].workspace.thread_runtime = Some(ThreadRuntimeSnapshot {
            recency_at_ms: None,
            status: CodexThreadStatus::Idle,
            model: Some(model.to_owned()),
            reasoning_effort: Some("custom-effort".to_owned()),
            runtime_generation: "test".to_owned(),
            observed_at_ms: 1,
            is_fresh: true,
        });
    }
    sort(&mut rows, &["model"], None);
    assert_eq!(names(&rows), ["old", "new", "unknown"]);
    sort(&mut rows, &["-model"], None);
    assert_eq!(names(&rows), ["new", "old", "unknown"]);
}

#[test]
fn invalid_cpu_is_missing_and_zero_is_known() {
    let mut rows = [
        row("repo", "nan", Idle),
        row("repo", "inf", Idle),
        row("repo", "zero", Idle),
    ];
    for (index, cpu) in [(0, f64::NAN), (1, f64::INFINITY), (2, 0.0)] {
        resources(&mut rows[index], None, Some(cpu), None);
    }
    for field in ["cpu", "-cpu"] {
        sort(&mut rows, &[field], None);
        assert_eq!(names(&rows)[0], "zero");
    }
}

#[test]
fn branch_values_and_exact_ties_have_deterministic_order() {
    let mut rows = [row("repo", "same", Idle), row("repo", "same", Idle)];
    rows[0].workspace.id = "z-id".to_owned();
    rows[1].workspace.id = "a-id".to_owned();
    sort(&mut rows, &["state"], None);
    assert_eq!(rows[0].workspace.id, "a-id");
    rows[0].workspace.branch_name = None;
    sort(&mut rows, &["-branch"], None);
    assert!(rows[0].workspace.branch_name.is_none());
}

fn recency(row: &mut WorkspaceListItem, time: Option<i64>) {
    row.workspace.thread_runtime = Some(
        serde_json::from_value(json!({
            "status":{"type":"idle"}, "recencyAtMs":time,
            "runtimeGeneration":"test", "observedAtMs":i64::MAX, "isFresh":true,
        }))
        .unwrap(),
    );
}

#[test]
fn recent_is_native_newest_first_globally_and_reverse_keeps_unknown_last() {
    let mut rows = [
        row("alpha", "old", Idle),
        row("zeta", "new", Idle),
        row("alpha", "unknown", Prepared),
    ];
    recency(&mut rows[0], Some(1_000));
    recency(&mut rows[1], Some(1_800_000_000_000));
    rows[0].workspace.updated_at_ms = i64::MAX;
    rows[2].workspace.created_at_ms = i64::MAX;
    sort(&mut rows, &["recent"], None);
    assert_eq!(names(&rows), ["new", "old", "unknown"]);
    sort(&mut rows, &["-recent"], None);
    assert_eq!(names(&rows), ["old", "new", "unknown"]);
    super::super::sorting::sort_workspace_collection(&mut rows);
    assert_eq!(names(&rows), ["old", "unknown", "new"]);
}

#[test]
fn recent_respects_primary_state_and_explicit_secondary_keys() {
    let mut rows = [
        row("alpha", "old-ready", Idle),
        row("zeta", "new-ready", Idle),
        row("beta", "old-waiting", WaitingForInput),
        row("alpha", "unknown-waiting", WaitingForInput),
    ];
    for (index, time) in [(0, 1_000), (1, 3_000), (2, 2_000)] {
        recency(&mut rows[index], Some(time));
    }
    sort(&mut rows, &["state", "recent"], None);
    assert_eq!(
        names(&rows),
        ["old-waiting", "unknown-waiting", "new-ready", "old-ready"]
    );
    sort(&mut rows, &["recent", "state"], None);
    assert_eq!(
        names(&rows),
        ["new-ready", "old-waiting", "old-ready", "unknown-waiting"]
    );
}

#[test]
fn invalid_and_missing_recency_never_borrow_observation_or_management_times() {
    let mut rows = [
        row("repo", "negative", Idle),
        row("repo", "overflow", Idle),
        row("repo", "missing", Idle),
        row("repo", "epoch", Idle),
        row("repo", "no-runtime", Prepared),
    ];
    for (index, time) in [(0, Some(-1)), (1, Some(i64::MAX)), (2, None), (3, Some(0))] {
        recency(&mut rows[index], time);
    }
    for key in ["recent", "-recent"] {
        sort(&mut rows, &[key], None);
        assert_eq!(
            names(&rows),
            ["epoch", "missing", "negative", "no-runtime", "overflow"]
        );
    }
}

#[test]
fn equal_dates_use_existing_repository_natural_name_and_id_ties() {
    let mut rows = [
        row("zeta", "work/1", Idle),
        row("alpha", "work/10", Idle),
        row("alpha", "work/2", Idle),
        row("alpha", "work/2", Idle),
    ];
    rows[2].workspace.id = "z-id".to_owned();
    rows[3].workspace.id = "a-id".to_owned();
    for row in &mut rows {
        recency(row, Some(1_800_000_000_000));
    }
    for key in ["recent", "-recent"] {
        sort(&mut rows, &[key], None);
        assert_eq!(
            rows.iter()
                .map(|row| row.workspace.id.as_str())
                .collect::<Vec<_>>(),
            ["a-id", "z-id", "alpha-work/10", "zeta-work/1"]
        );
    }
    sort(&mut rows, &["recent", "-name"], None);
    assert_eq!(names(&rows), ["work/10", "work/2", "work/2", "work/1"]);
}
