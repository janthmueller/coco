use super::*;
use WorkspacePhase::*;

#[test]
fn memory_uses_cgroup_charge_before_rss_and_keeps_zero_distinct_from_missing() {
    let mut rows = [
        row("repo", "cgroup", Idle),
        row("repo", "rss", Idle),
        row("repo", "zero", Idle),
        row("repo", "missing", Idle),
    ];
    resources(&mut rows[0], Some(1024), None, None);
    resources(&mut rows[1], None, None, None);
    rows[1]
        .runtime_resources
        .as_mut()
        .unwrap()
        .resident_memory_bytes = Some(128);
    resources(&mut rows[2], Some(0), None, None);
    sort(&mut rows, &["memory"], None);
    assert_eq!(names(&rows), ["zero", "rss", "cgroup", "missing"]);
    sort(&mut rows, &["-memory"], None);
    assert_eq!(names(&rows), ["cgroup", "rss", "zero", "missing"]);
}

#[test]
fn token_totals_keep_integer_precision_and_include_last_seen_and_zero_counts() {
    let mut rows = [
        row("repo", "large", Idle),
        row("repo", "larger", Idle),
        row("repo", "zero", Idle),
        row("repo", "unknown", Idle),
    ];
    let data = [
        usage(&rows[0], u64::MAX - 1, 0, None),
        usage(&rows[1], u64::MAX, 0, None),
        usage(&rows[2], 0, 0, None),
    ];
    assert!(
        data.iter()
            .all(|item| !item.tokens.as_ref().unwrap().is_fresh)
    );
    sort(&mut rows, &["tokens"], Some(&data));
    assert_eq!(names(&rows), ["zero", "large", "larger", "unknown"]);
    sort(&mut rows, &["-tokens"], Some(&data));
    assert_eq!(names(&rows), ["larger", "large", "zero", "unknown"]);
}

#[test]
fn context_requires_a_nonzero_window_and_keeps_a_known_empty_context() {
    let mut rows = [
        row("repo", "filled", Idle),
        row("repo", "empty", Idle),
        row("repo", "no-window", Idle),
        row("repo", "zero-window", Idle),
    ];
    let data = [
        usage(&rows[0], 1000, 100, Some(1000)),
        usage(&rows[1], 1000, 0, Some(1000)),
        usage(&rows[2], 1000, 50, None),
        usage(&rows[3], 1000, 50, Some(0)),
    ];
    sort(&mut rows, &["context"], Some(&data));
    assert_eq!(
        names(&rows),
        ["empty", "filled", "no-window", "zero-window"]
    );
    sort(&mut rows, &["-context"], Some(&data));
    assert_eq!(
        names(&rows),
        ["filled", "empty", "no-window", "zero-window"]
    );
}
