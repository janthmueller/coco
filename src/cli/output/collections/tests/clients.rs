use super::*;
use crate::domain::clients::{ClientIntegration, ClientMetadata, WorkspaceClient};

fn attached() -> WorkspaceListItem {
    let mut item = workspace_item("repo", "project", "fix/login", WorkspacePhase::Idle);
    item.clients = Some(vec![WorkspaceClient {
        id: "presence".to_owned(),
        metadata: ClientMetadata {
            kind: "native_tui".to_owned(),
            integration: Some(ClientIntegration {
                kind: "tmux".to_owned(),
                scope: "opaque-server".to_owned(),
                locator: "%7".to_owned(),
                label: Some("dev:2.1".to_owned()),
            }),
        },
    }]);
    item
}

#[test]
fn clients_column_is_opt_in_and_preserves_empty_rows_and_branch_alignment() {
    let plain = workspace_item("repo", "project", "fix/search", WorkspacePhase::Active);
    let default = render_workspace_list(
        std::slice::from_ref(&plain),
        false,
        false,
        true,
        None,
        120,
        Palette::plain(),
    );
    assert!(!default.contains("CLIENTS"));
    let output = render_workspace_list(
        &[attached(), plain],
        false,
        false,
        true,
        None,
        140,
        Palette::plain(),
    );
    assert!(output.contains("CLIENTS"));
    assert!(output.contains("tmux dev:2.1"));
    let lines: Vec<_> = output.lines().collect();
    let branch_column =
        |line: &str, branch: &str| line[..line.find(branch).unwrap()].chars().count();
    assert_eq!(
        branch_column(lines[1], "coco/fix/login"),
        branch_column(lines[2], "coco/fix/search")
    );
    assert!(lines[2].contains('—'));
    assert!(!output.contains("opaque-server") && !output.contains("presence"));
}

#[test]
fn clients_compose_with_tree_resources_and_narrow_terminal_output() {
    let mut second = workspace_item("repo", "project", "fix/search", WorkspacePhase::Active);
    second.clients = Some(Vec::new());
    let items = [attached(), second];
    let output = render_workspace_tree(&items, true, true, true, None, 140, Palette::plain());
    assert!(output.contains("/repos/project"));
    assert!(output.contains("├─ login"));
    assert!(output.contains("tmux dev:2.1"));
    assert!(output.contains("MEMORY") && output.contains("CLIENTS"));
    let narrow = render_workspace_list(&items, true, true, true, None, 60, Palette::plain());
    assert!(narrow.lines().all(|line| line.chars().count() <= 60));
}
