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
