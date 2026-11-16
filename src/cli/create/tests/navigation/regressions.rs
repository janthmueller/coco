use super::*;

fn with_context() -> CreateArgs {
    let mut args = named();
    args.context = Some("thread:source".into());
    args.compact_context = true;
    args
}

#[tokio::test]
async fn abandoning_a_context_edit_restores_accepted_focus_and_prefilled_text() {
    let mut dialog = Dialog::new([
        Tab("Worktree"),
        Pick("Create workspace", "Edit settings"),
        Pick("Edit settings", "Context"),
        Pick("Context", "Thread ID"),
        Text("Codex thread ID", "abandoned"),
        Back("Context size"),
        Back("Context"),
        Back("Edit settings"),
        Pick("Create workspace", "Edit settings"),
        Pick("Edit settings", "Context"),
        Pick("Context", "Thread ID"),
        Text("Codex thread ID", "source"),
        Pick("Context size", "Compact context"),
        Pick("Create workspace", "Create"),
    ]);
    let args = run_dialog(with_context(), &mut dialog).await;
    assert_eq!(args.context.as_deref(), Some("thread:source"));
    assert_eq!(dialog.initial_text[1].1, "source");
    let contexts = dialog
        .focus
        .iter()
        .filter(|(title, _)| title == "Context")
        .collect::<Vec<_>>();
    assert_eq!(
        contexts.last().unwrap().1.as_deref(),
        Some("selected:thread:source")
    );
}

#[tokio::test]
async fn abandoning_an_edit_after_retry_still_restores_the_original_accepted_answers() {
    let mut dialog = Dialog::new([
        Tab("Worktree"),
        Pick("Create workspace", "Edit settings"),
        Pick("Edit settings", "Context"),
        Pick("Context", "Thread ID"),
        Text("Codex thread ID", "abandoned"),
        Fail("Context size"),
        Retry("Context"),
        Back("Context"),
        Back("Edit settings"),
        Pick("Create workspace", "Edit settings"),
        Pick("Edit settings", "Context"),
        Pick("Context", "Thread ID"),
        Text("Codex thread ID", "source"),
        Pick("Context size", "Compact context"),
        Pick("Create workspace", "Create"),
    ]);
    let args = run_dialog(with_context(), &mut dialog).await;
    assert_eq!(args.context.as_deref(), Some("thread:source"));
    assert_eq!(dialog.initial_text.last().unwrap().1, "source");
    let contexts = dialog
        .focus
        .iter()
        .filter(|(title, _)| title == "Context")
        .collect::<Vec<_>>();
    assert_eq!(
        contexts[1].1.as_deref(),
        Some("Thread ID"),
        "retry must retain pending input"
    );
    assert_eq!(
        contexts[2].1.as_deref(),
        Some("selected:thread:source"),
        "Back must restore accepted input"
    );
}

#[tokio::test]
async fn leaving_an_abandoned_edit_restores_normal_back_navigation_from_review() {
    let mut dialog = Dialog::new([
        Tab("Worktree"),
        Pick("Create workspace", "Edit settings"),
        Pick("Edit settings", "Profile"),
        Pick("Profile", "Choose profile"),
        Back("Profile name"),
        Back("Profile"),
        Back("Edit settings"),
        Back("Create workspace"),
        Tab("Worktree"),
        Pick("Create workspace", "Create"),
    ]);
    let args = run_dialog(named(), &mut dialog).await;
    assert!(args.profile.is_none());
}

#[tokio::test]
async fn back_skips_code_history_after_branch_adoption_instead_of_moving_forward() {
    let mut dialog = Dialog::new([
        Pick("Worktree", "New branch"),
        Pick("Code", "Current checkout"),
        Tab("Git changes"),
        Pick("Create workspace", "Edit settings"),
        Pick("Edit settings", "Worktree"),
        Pick("Worktree", "Existing branch"),
        Pick("Use branch", "review/existing-branch"),
        Back("Create workspace"),
        Back("Worktree"),
        Back("Git changes"),
        Tab("Worktree"),
        Pick("Create workspace", "Create"),
    ]);
    let args = run_dialog(named(), &mut dialog).await;
    assert_eq!(args.checkout.as_deref(), Some("review/existing-branch"));
    assert!(args.base.is_none() && args.branch.is_none());
}

#[tokio::test]
async fn ordinary_back_keeps_pending_first_visit_choices_when_revisiting_an_earlier_question() {
    let mut dialog = Dialog::new([
        Text("Workspace name", "first"),
        Pick("Worktree", "Custom branch"),
        Back("Branch name"),
        Back("Worktree"),
        Text("Workspace name", "second"),
        Pick("Worktree", "Custom branch"),
        Text("Branch name", "topic"),
        Tab("Code"),
        Pick("Create workspace", "Create"),
    ]);
    let args = run_dialog(create_args(&["coco", "create"]), &mut dialog).await;
    assert_eq!(args.name.as_deref(), Some("second"));
    assert_eq!(args.branch.as_deref(), Some("topic"));
    assert_eq!(
        dialog
            .focus
            .iter()
            .rfind(|(title, _)| title == "Worktree")
            .unwrap()
            .1
            .as_deref(),
        Some("Custom branch")
    );
}

#[tokio::test]
async fn child_back_keeps_pending_context_input_until_the_entire_edit_is_abandoned() {
    let mut dialog = Dialog::new([
        Tab("Worktree"),
        Pick("Create workspace", "Edit settings"),
        Pick("Edit settings", "Context"),
        Pick("Context", "Thread ID"),
        Text("Codex thread ID", "replacement"),
        Back("Context size"),
        Pick("Context", "Thread ID"),
        Text("Codex thread ID", "replacement"),
        Pick("Context size", "Full context"),
        Pick("Create workspace", "Create"),
    ]);
    let args = run_dialog(with_context(), &mut dialog).await;
    assert_eq!(args.context.as_deref(), Some("thread:replacement"));
    assert_eq!(dialog.initial_text[1].1, "replacement");
    assert!(!args.compact_context);
}

#[tokio::test]
async fn backing_out_of_a_reopened_main_step_restores_its_accepted_text() {
    let mut dialog = Dialog::new([
        Pick("Worktree", "New branch"),
        Pick("Code", "Current checkout"),
        Pick("Git changes", "Don't copy"),
        Pick("Context", "Thread ID"),
        Text("Codex thread ID", "source"),
        Pick("Context size", "Full context"),
        Tab("Profile"),
        Back("Create workspace"),
        Back("Profile"),
        Pick("Context", "Thread ID"),
        Text("Codex thread ID", "abandoned"),
        Back("Context size"),
        Back("Context"),
        Tab("Git changes"),
        Pick("Create workspace", "Edit settings"),
        Pick("Edit settings", "Context"),
        Pick("Context", "Thread ID"),
        Text("Codex thread ID", "source"),
        Pick("Context size", "Full context"),
        Pick("Create workspace", "Create"),
    ]);
    let args = run_dialog(named(), &mut dialog).await;
    assert_eq!(args.context.as_deref(), Some("thread:source"));
    assert_eq!(dialog.initial_text.last().unwrap().1, "source");
}

#[tokio::test]
async fn back_is_unavailable_when_every_earlier_question_is_now_inapplicable() {
    let args = create_args(&[
        "coco",
        "create",
        "demo",
        "-i",
        "--branch",
        "topic",
        "--context",
        "thread:source",
        "--profile",
        "dev",
        "--model",
        "model",
        "--temporary",
        "--keep-thread",
        "--carry-changes",
        "-j",
    ]);
    let mut dialog = Dialog::new([
        Pick("Code", "Current checkout"),
        Pick("Create workspace", "Edit settings"),
        Pick("Edit settings", "Worktree"),
        Pick("Worktree", "Existing branch"),
        Pick("Use branch", "review/existing-branch"),
        Back("Create workspace"),
        Cancel("Worktree"),
    ]);
    let error = walkthrough(&test_paths(), "/repo".into(), args, &mut dialog)
        .await
        .unwrap_err();
    assert!(matches!(error.downcast_ref(), Some(NavigationExit::Cancel)));
    assert_eq!(
        dialog.back_available.last().unwrap(),
        &("Worktree".into(), false)
    );
    dialog.consumed();
}
