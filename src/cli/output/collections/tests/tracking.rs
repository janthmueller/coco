use super::*;
use crate::protocol::{GitTracking, GitTrackingState};

fn item() -> WorkspaceListItem {
    let mut row = workspace_item("repo", "project", "fix/login", WorkspacePhase::Idle);
    row.workspace.branch_name = Some(row.workspace.name.clone());
    row.git_tracking = Some(GitTracking {
        state: GitTrackingState::Diverged,
        upstream: Some("refs/remotes/origin/fix/login".into()),
        remote: Some("origin".into()),
        ahead_by: Some(2),
        behind_by: Some(3),
        observed_at_ms: Some(42),
    });
    row
}

#[test]
fn list_and_tree_use_existing_branch_column_with_semantic_badges() {
    let rows = [item()];
    let list = render_workspace_list(&rows, false, false, true, None, 120, Palette::plain());
    assert!(list.contains("fix/login · ↑2 ↓3"));
    assert_eq!(
        list.lines()
            .next()
            .unwrap()
            .split_whitespace()
            .collect::<Vec<_>>(),
        ["WORKSPACE", "STATE", "MODEL", "BRANCH"]
    );
    let colored = render_workspace_list(&rows, false, false, true, None, 120, Palette::colored());
    assert!(colored.contains(&Palette::colored().paint(Tone::YellowBold, "↑2 ↓3")));
    let tree = render_workspace_tree(&rows, true, false, true, None, 120, Palette::plain());
    assert!(tree.contains("↑2 ↓3"));
    assert!(!list.contains('\x1b') && !tree.contains('\x1b'));
}

#[test]
fn tracking_rows_are_bounded_even_with_wide_branch_names() {
    use unicode_width::UnicodeWidthStr;
    let mut row = item();
    row.workspace.branch_name = Some("特別なブランチ/very-long-branch-name".repeat(3));
    for width in 50..120 {
        let rendered = render_workspace_list(
            std::slice::from_ref(&row),
            false,
            false,
            true,
            None,
            width,
            Palette::plain(),
        );
        assert!(
            rendered.lines().all(|line| line.width() <= width),
            "{rendered}"
        );
    }
}

#[test]
fn review_regression_wide_segmented_cells_pad_the_rendered_width() {
    use unicode_width::UnicodeWidthStr;
    let mut text = StyledText::new("x", Tone::Dim);
    text.push("界".repeat(40), Tone::Primary);
    let cell = Cell::styled(text.clone());
    for width in 0..90 {
        let plain = cell.render(width, false, Palette::plain());
        assert_eq!(plain.width(), width, "cell width {width}: {plain:?}");
        let clipped = text.truncated(width);
        let padding = " ".repeat(width - clipped.width());
        assert_eq!(
            cell.render(width, false, Palette::colored()),
            format!("{}{padding}", clipped.render(width, Palette::colored()))
        );
        assert_eq!(cell.render(width, true, Palette::plain()), clipped.as_str());
    }
}

#[test]
fn structured_tracking_is_present_in_json_without_decorating_machine_values() {
    let row = item();
    let value = crate::cli::output::status_collection_json(&[row], None, None).unwrap();
    let tracking = &value["workspaces"][0]["gitTracking"];
    assert_eq!(tracking["state"], "diverged");
    assert_eq!(tracking["upstream"], "refs/remotes/origin/fix/login");
    assert_eq!(tracking["aheadBy"], 2);
    assert_eq!(tracking["behindBy"], 3);
    assert_eq!(tracking["observedAtMs"], 42);
    assert!(!value.to_string().contains("↑2"));
    assert_eq!(value["workspaces"][0]["branchName"], "fix/login");
}
