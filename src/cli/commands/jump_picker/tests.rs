use super::*;
use serde_json::json;

mod selection;

const NOW: i64 = 1_800_000_000_000;

fn row(repo: &str, name: &str, time: Option<i64>) -> WorkspaceListItem {
    serde_json::from_value(json!({
        "id":format!("{repo}-{name}"), "repositoryId":repo, "name":name,
        "contextMode":"fresh", "context":{},
        "profile":{"name":"default","sourceHash":"test","effectiveSettings":{}},
        "lifecycle":"ready", "availability":"open", "phase":"idle",
        "waitReasons":[], "worktreeMode":"new_branch", "branchName":name,
        "createdAtMs":1, "updatedAtMs":NOW, "threadArchived":false,
        "threadRuntime":{"status":{"type":"idle"}, "recencyAtMs":time,
            "runtimeGeneration":"test", "observedAtMs":NOW, "isFresh":true},
        "repository":{"id":repo,"displayName":repo,"rootPath":format!("/repos/{repo}")}
    }))
    .unwrap()
}

fn names(choices: &[Choice]) -> Vec<&str> {
    choices.iter().map(|choice| choice.label.as_str()).collect()
}

#[test]
fn recent_use_is_primary_across_repositories_not_workspace_mutation_or_name() {
    let mut rows = vec![
        row("alpha", "a-old", Some(NOW - 7_200_000)),
        row("zeta", "z-recent", Some(NOW - 300_000)),
        row("alpha", "a-unknown", None),
    ];
    rows[0].workspace.updated_at_ms = i64::MAX;
    let result = choices(&mut rows, JumpSort::Recent, true, NOW);
    assert_eq!(names(&result), ["z-recent", "a-old", "a-unknown"]);
    assert_eq!(result[0].trailing_detail.as_deref(), Some("5m ago"));
    assert_eq!(result[1].trailing_detail.as_deref(), Some("2h ago"));
    assert!(result[2].trailing_detail.is_none());
    assert!(result[0].detail.as_ref().unwrap().contains("/repos/zeta"));
    assert_eq!(rows[0].workspace.name, result[0].label);
}

#[test]
fn ties_and_missing_dates_use_natural_name_then_repository_and_id() {
    let mut rows = vec![
        row("zeta", "work/2", Some(NOW)),
        row("alpha", "work/10", Some(NOW)),
        row("alpha", "work/2", Some(NOW)),
        row("alpha", "unknown/10", None),
        row("zeta", "unknown/2", None),
    ];
    choices(&mut rows, JumpSort::Recent, true, NOW);
    assert_eq!(
        rows.iter()
            .map(|row| row.workspace.id.as_str())
            .collect::<Vec<_>>(),
        [
            "alpha-work/2",
            "zeta-work/2",
            "alpha-work/10",
            "zeta-unknown/2",
            "alpha-unknown/10"
        ]
    );
    let mut duplicate = rows[0].clone();
    duplicate.workspace.id = "a-tie".into();
    rows.push(duplicate);
    choices(&mut rows, JumpSort::Recent, true, NOW);
    assert_eq!(rows[0].workspace.id, "a-tie");
}

#[test]
fn name_order_ignores_recency_without_removing_time_or_scope_details() {
    let mut rows = vec![
        row("alpha", "work/10", Some(NOW)),
        row("zeta", "work/2", None),
    ];
    let result = choices(&mut rows, JumpSort::Name, true, NOW);
    assert_eq!(names(&result), ["work/2", "work/10"]);
    assert!(result[0].detail.as_ref().unwrap().contains("/repos/zeta"));
    assert_eq!(result[1].trailing_detail.as_deref(), Some("just now"));
}

#[test]
fn branches_are_exception_details_only_for_jump_and_detached_stays_explicit() {
    let mut item = row("repo", "feat/login", Some(NOW));
    assert_eq!(choice(&item, false, NOW).detail.as_deref(), Some("Ready"));
    assert!(
        super::super::workspace_choice(&item, false)
            .detail
            .unwrap()
            .contains("feat/login")
    );
    for branch in ["main", "coco/feat/login", "Feat/login"] {
        item.workspace.branch_name = Some(branch.into());
        assert_eq!(
            choice(&item, false, NOW).detail,
            Some(format!("Ready · {branch}"))
        );
    }
    item.workspace.branch_name = None;
    item.workspace.worktree_mode = WorktreeMode::Detached;
    assert_eq!(
        choice(&item, false, NOW).detail.as_deref(),
        Some("Ready · detached")
    );
    item.workspace.worktree_mode = WorktreeMode::NewBranch;
    assert_eq!(choice(&item, false, NOW).detail.as_deref(), Some("Ready"));
}

#[test]
fn invalid_or_absent_dates_stay_unknown_without_using_observation_time() {
    for date in [None, Some(-1), Some(i64::MIN), Some(i64::MAX)] {
        let item = row("repo", "unknown", date);
        assert!(recency(&item).is_none());
        assert!(choice(&item, false, NOW).trailing_detail.is_none());
    }
    let mut item = row("repo", "no-native-data", Some(NOW));
    item.workspace.thread_runtime = None;
    assert!(choice(&item, false, NOW).trailing_detail.is_none());
}

#[test]
fn compact_relative_times_are_fixed_at_creation_and_never_show_seconds() {
    for (minutes, expected) in [
        (0, "just now"),
        (1, "1m ago"),
        (59, "59m ago"),
        (60, "1h ago"),
        (1_439, "23h ago"),
        (1_440, "1d ago"),
        (10_079, "6d ago"),
        (10_080, "1w ago"),
        (40_320, "1mo ago"),
        (525_600, "1y ago"),
    ] {
        assert_eq!(relative_time(NOW - minutes * 60_000, NOW), expected);
    }
    assert_eq!(relative_time(NOW + 60_000, NOW), "just now");
    assert_eq!(relative_time(NOW - 59_999, NOW), "just now");
    let item = row("repo", "frozen", Some(NOW - 300_000));
    let frozen = choice(&item, false, NOW);
    assert_eq!(frozen.trailing_detail.as_deref(), Some("5m ago"));
}
