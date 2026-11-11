use super::*;

mod picker;

const NOW: i64 = 1_800_000_000_000;

fn entry(name: &str, repo: &str) -> WorkspacePathEntry {
    WorkspacePathEntry {
        workspace_id: format!("{repo}-{name}"),
        name: name.to_owned(),
        repository: crate::protocol::RepositorySummary {
            id: repo.to_owned(),
            display_name: repo.to_owned(),
            root_path: PathBuf::from(format!("/repos/{repo}")),
        },
        path: PathBuf::from(format!("/worktrees/{repo}/{name}")),
        branch_name: Some(name.to_owned()),
        worktree_mode: WorktreeMode::NewBranch,
        recency_at_ms: None,
    }
}

#[test]
fn navigation_order_is_natural_and_global_duplicates_are_deterministic() {
    let mut rows = vec![
        entry("feat/10", "a"),
        entry("feat/2", "b"),
        entry("feat/2", "a"),
    ];
    sort_entries(&mut rows, WorkspaceSort::Name);
    assert_eq!(
        rows.iter()
            .map(|row| row.workspace_id.as_str())
            .collect::<Vec<_>>(),
        ["a-feat/2", "b-feat/2", "a-feat/10"]
    );
}

#[test]
fn navigation_picker_omits_redundant_branch_and_native_state() {
    let mut row = entry("fix/login", "repo");
    assert_eq!(choice(&row, false, NOW), Choice::new("fix/login", None));
    assert_eq!(
        choice(&row, true, NOW).detail.as_deref(),
        Some("/repos/repo")
    );
    row.branch_name = Some("another/branch".to_owned());
    assert_eq!(
        choice(&row, false, NOW).detail.as_deref(),
        Some("another/branch")
    );
    row.branch_name = None;
    row.worktree_mode = WorktreeMode::Detached;
    assert_eq!(choice(&row, false, NOW).detail.as_deref(), Some("detached"));
}

#[test]
fn recent_navigation_matches_shared_picker_and_freezes_compact_ages() {
    let mut old = entry("feat/2", "a");
    old.recency_at_ms = Some(NOW - 7_200_000);
    let mut recent = entry("feat/10", "z");
    recent.recency_at_ms = Some(NOW - 300_000);
    let unknown = entry("feat/1", "a");
    let mut rows = vec![old, unknown, recent];
    sort_entries(&mut rows, WorkspaceSort::Recent);
    assert_eq!(
        rows.iter().map(|row| row.name.as_str()).collect::<Vec<_>>(),
        ["feat/10", "feat/2", "feat/1"]
    );
    let choices = rows
        .iter()
        .map(|row| choice(row, true, NOW))
        .collect::<Vec<_>>();
    assert_eq!(choices[0].trailing_detail.as_deref(), Some("5m ago"));
    assert_eq!(choices[1].trailing_detail.as_deref(), Some("2h ago"));
    assert!(choices[2].trailing_detail.is_none());
    assert_eq!(choices[0].detail.as_deref(), Some("/repos/z"));
    sort_entries(&mut rows, WorkspaceSort::Name);
    assert_eq!(rows[0].name, "feat/1");
    assert_eq!(rows[1].name, "feat/2");
}

#[test]
fn unknown_invalid_and_tied_dates_use_natural_name_repository_and_exact_id() {
    let mut rows = vec![
        entry("feat/10", "a"),
        entry("feat/2", "z"),
        entry("feat/2", "a"),
    ];
    let mut duplicate = rows[2].clone();
    duplicate.workspace_id = "a-tie".to_owned();
    rows.push(duplicate);
    for time in [None, Some(NOW), Some(-1), Some(i64::MAX)] {
        for row in &mut rows {
            row.recency_at_ms = time;
        }
        sort_entries(&mut rows, WorkspaceSort::Recent);
        assert_eq!(
            rows.iter()
                .map(|row| row.workspace_id.as_str())
                .collect::<Vec<_>>(),
            ["a-feat/2", "a-tie", "z-feat/2", "a-feat/10"]
        );
        if time != Some(NOW) {
            assert!(
                rows.iter()
                    .all(|row| choice(row, false, NOW).trailing_detail.is_none())
            );
        }
    }
}

#[test]
fn plain_output_preserves_unicode_and_shell_characters_but_rejects_ambiguous_lines() {
    let path = Path::new("/tmp/über 'quotes' $vars ; [glob] $(command)");
    assert_eq!(printable_path(path).unwrap(), path.to_str().unwrap());
    for path in ["relative", "/tmp/trailing\n", "/tmp/a\rb", "/tmp/\x1b[31m"] {
        assert!(printable_path(Path::new(path)).is_err());
    }
}

#[cfg(unix)]
#[test]
fn non_utf8_paths_fail_instead_of_silently_changing_the_destination() {
    use std::os::unix::ffi::OsStringExt;
    let path = PathBuf::from(std::ffi::OsString::from_vec(b"/tmp/invalid-\xff".to_vec()));
    assert!(printable_path(&path).is_err());
}
