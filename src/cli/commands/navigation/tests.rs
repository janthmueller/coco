use super::*;

mod picker;

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
    }
}

#[test]
fn navigation_order_is_natural_and_global_duplicates_are_deterministic() {
    let mut rows = vec![
        entry("feat/10", "a"),
        entry("feat/2", "b"),
        entry("feat/2", "a"),
    ];
    sort_entries(&mut rows);
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
    assert_eq!(choice(&row, false), Choice::new("fix/login", None));
    assert_eq!(choice(&row, true).detail.as_deref(), Some("/repos/repo"));
    row.branch_name = Some("another/branch".to_owned());
    assert_eq!(
        choice(&row, false).detail.as_deref(),
        Some("another/branch")
    );
    row.branch_name = None;
    row.worktree_mode = WorktreeMode::Detached;
    assert_eq!(choice(&row, false).detail.as_deref(), Some("detached"));
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
