use super::*;

fn clean() -> crate::protocol::RepositoryGitState {
    let mut state = git_state();
    state.changes = crate::protocol::GitChangeSummary::default();
    state
}

fn select_tracked() -> [Action; 4] {
    [
        Pick("Worktree", "New branch"),
        Pick("Code", "Current checkout"),
        Pick("Git changes", "Copy tracked"),
        Tab("Context"),
    ]
}

async fn run_states(
    args: CreateArgs,
    states: Vec<crate::protocol::RepositoryGitState>,
    dialog: &mut Dialog,
) -> CreateArgs {
    let expected_calls = states.len();
    let inspector = FakeGitInspector {
        states: Mutex::new(states.into()),
        ..FakeGitInspector::default()
    };
    let args = super::super::super::walkthrough_with_inspector(
        &test_paths(),
        "/repo".into(),
        args,
        dialog,
        &inspector,
    )
    .await
    .unwrap();
    dialog.consumed();
    assert_eq!(inspector.calls.lock().unwrap().len(), expected_calls);
    args
}

fn review_policy(dialog: &Dialog, review: usize) -> &str {
    &dialog.reviews[review]
        .iter()
        .find(|field| field.label == "Git changes")
        .unwrap()
        .value
}

#[tokio::test]
async fn clean_checkout_reset_is_the_editor_default_and_enter_keeps_it() {
    let mut dialog = Dialog::new(select_tracked().into_iter().chain([
        Pick("Create workspace", "Edit settings"),
        Pick("Edit settings", "Git changes"),
        Enter("Git changes"),
        Pick("Create workspace", "Create"),
    ]));
    let args = run_states(
        named(),
        vec![git_state(), clean(), clean(), clean()],
        &mut dialog,
    )
    .await;
    assert_eq!(review_policy(&dialog, 0), "don't copy");
    assert!(!args.carry_changes && !args.carry_untracked && !args.dirty);
    assert_eq!(dialog.value("Git changes"), "don't copy");
    assert_eq!(
        dialog
            .focus
            .iter()
            .rfind(|(title, _)| title == "Git changes")
            .unwrap()
            .1
            .as_deref(),
        Some("Don't copy")
    );
}

#[tokio::test]
async fn clean_reset_stays_consistent_after_read_only_review_retry() {
    let mut dialog = Dialog::new(select_tracked().into_iter().chain([
        Fail("Create workspace"),
        Retry("Create workspace"),
        Pick("Create workspace", "Edit settings"),
        Pick("Edit settings", "Git changes"),
        Enter("Git changes"),
        Pick("Create workspace", "Create"),
    ]));
    let args = run_states(
        named(),
        vec![git_state(), clean(), clean(), clean(), clean()],
        &mut dialog,
    )
    .await;
    assert!(!args.carry_changes && !args.carry_untracked && !args.dirty);
    assert!(dialog.reviews.iter().all(|fields| {
        fields
            .iter()
            .find(|field| field.label == "Git changes")
            .unwrap()
            .value
            == "don't copy"
    }));
}

#[tokio::test]
async fn backing_out_of_clean_review_restores_the_original_explicit_choice() {
    let mut dialog = Dialog::new(select_tracked().into_iter().chain([
        Back("Create workspace"),
        Back("Context"),
        Enter("Git changes"),
        Pick("Create workspace", "Create"),
    ]));
    let args = run_states(
        named(),
        vec![git_state(), clean(), clean(), clean()],
        &mut dialog,
    )
    .await;
    assert_eq!(review_policy(&dialog, 0), "don't copy");
    assert!(args.carry_changes && !args.carry_untracked);
    assert_eq!(dialog.value("Git changes"), "copy tracked");
    assert_eq!(
        dialog
            .focus
            .iter()
            .rfind(|(title, _)| title == "Git changes")
            .unwrap()
            .1
            .as_deref(),
        Some("tracked")
    );
}

#[tokio::test]
async fn enter_keeps_empty_explicit_carry_presets_when_editing() {
    for (flag, both) in [("--carry-changes", false), ("--dirty", true)] {
        let args = create_args(&["coco", "create", "demo", "-i", flag]);
        let mut dialog = Dialog::new([
            Tab("Worktree"),
            Pick("Create workspace", "Edit settings"),
            Pick("Edit settings", "Git changes"),
            Enter("Git changes"),
            Pick("Create workspace", "Create"),
        ]);
        let args = run_states(args, vec![clean(), clean(), clean(), clean()], &mut dialog).await;
        assert!(args.carry_changes);
        assert_eq!(args.carry_untracked, both);
        assert_eq!(
            dialog.value("Git changes"),
            if both {
                "copy tracked + untracked"
            } else {
                "copy tracked"
            }
        );
    }
}

#[tokio::test]
#[ignore = "manual carry-policy probe; requires an interactive PTY, fake Git inspection only"]
async fn interactive_clean_checkout_walkthrough_probe() {
    let mut interaction = crate::cli::prompt::TerminalInteraction::detect(false);
    let inspector = FakeGitInspector {
        states: Mutex::new(vec![git_state(), clean(), clean(), clean()].into()),
        ..FakeGitInspector::default()
    };
    let args = super::super::super::walkthrough_with_inspector(
        &test_paths(),
        "/unreachable/coco-clean-probe".into(),
        named(),
        &mut interaction,
        &inspector,
    )
    .await
    .unwrap();
    assert!(!args.carry_changes && !args.carry_untracked && !args.dirty);
    assert_eq!(inspector.calls.lock().unwrap().len(), 4);
    assert!(!crossterm::terminal::is_raw_mode_enabled().unwrap());
    eprintln!("clean-policy-result=ok");
}
