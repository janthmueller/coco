use super::*;
use WorkspacePhase::*;

#[test]
fn excluding_unloaded_keeps_prepared_and_does_not_opt_into_closed_scope() {
    let filter = StateFilter::new(&[], &["unloaded".parse().unwrap()]).unwrap();
    assert!(filter.phases().is_none());
    let mut rows = vec![
        row("repo", "prepared", Prepared),
        row("repo", "unloaded", NotLoaded),
        row("repo", "closed", Closed),
        row("repo", "active", Active),
    ];
    filter.apply(&mut rows);
    assert_eq!(names(&rows), ["prepared", "active"]);
}

#[test]
fn positive_filters_union_and_exclusions_override_them() {
    let filter = StateFilter::new(
        &["working".parse().unwrap(), "needs-input".parse().unwrap()],
        &["working".parse().unwrap()],
    )
    .unwrap();
    assert_eq!(filter.phases().unwrap(), ["active", "waiting_for_input"]);
    let mut rows = vec![
        row("repo", "active", Active),
        row("repo", "waiting", WaitingForInput),
        row("repo", "ready", Idle),
    ];
    filter.apply(&mut rows);
    assert_eq!(names(&rows), ["waiting"]);
}

#[test]
fn selector_aliases_reuse_the_existing_phase_and_reject_unknown_states() {
    for (visible, native) in [
        ("working", "active"),
        ("ready", "idle"),
        ("unloaded", "not_loaded"),
        ("needs-input", "waiting_for_input"),
        ("needs-approval", "waiting_for_approval"),
        ("preparing", "provisioning"),
        ("codex-error", "system_error"),
    ] {
        let left: super::super::StateSelector = visible.parse().unwrap();
        assert_eq!(left, native.parse().unwrap());
    }
    assert!("running".parse::<super::super::StateSelector>().is_err());
    assert!(StateFilter::new(&["closed".parse().unwrap()], &[]).is_err());
}
