use super::*;

fn preset() -> CreateArgs {
    create_args(&[
        "coco",
        "create",
        "demo",
        "-i",
        "--branch",
        "explicit",
        "--base",
        "main",
        "--context",
        "thread:source",
        "-C",
        "--dirty",
        "--profile",
        "dev",
        "--model",
        "model-override",
        "--temporary",
        "--keep-thread",
        "--send",
        "Original message",
        "-j",
    ])
}

#[tokio::test]
async fn fresh_context_clears_compaction_but_preserves_the_code_base() {
    let mut dialog = Dialog::new([
        Pick("Create workspace", "Edit settings"),
        Pick("Edit settings", "Context"),
        Pick("Context", "Fresh"),
        Pick("Create workspace", "Create"),
    ]);
    let args = run_dialog(preset(), &mut dialog).await;
    assert!(args.context.is_none() && !args.compact_context);
    assert_eq!(args.base.as_deref(), Some("main"));
}

#[tokio::test]
async fn editing_an_ambiguous_context_reference_does_not_force_a_thread_interpretation() {
    let mut args = preset();
    args.context = Some("review/source".into());
    let mut dialog = Dialog::new([
        Pick("Create workspace", "Edit settings"),
        Pick("Edit settings", "Context"),
        Pick("Context", "Selected source"),
        Pick("Context size", "Compact context"),
        Pick("Create workspace", "Create"),
    ]);
    let args = run_dialog(args, &mut dialog).await;
    assert_eq!(args.context.as_deref(), Some("review/source"));
    assert!(args.compact_context);
    assert_eq!(
        dialog
            .focus
            .iter()
            .find(|(title, _)| title == "Context")
            .unwrap()
            .1
            .as_deref(),
        Some("selected:review/source")
    );
}

#[tokio::test]
async fn persistent_cleanup_drops_temporary_keep_thread_and_keeps_post_action() {
    let mut dialog = Dialog::new([
        Pick("Create workspace", "Edit settings"),
        Pick("Edit settings", "Cleanup"),
        Pick("Cleanup", "Keep workspace"),
        Pick("Create workspace", "Create"),
    ]);
    let args = run_dialog(preset(), &mut dialog).await;
    assert!(!args.temporary && !args.keep_thread);
    assert!(args.jump && args.send.is_some());
    normalize_create_args("/repo".into(), args, "unchanged-post-action".into()).unwrap();
}

#[tokio::test]
async fn editing_profile_preserves_explicit_model_override() {
    let mut dialog = Dialog::new([
        Pick("Create workspace", "Edit settings"),
        Pick("Edit settings", "Profile"),
        Pick("Profile", "Choose profile"),
        Text("Profile name", "production"),
        Pick("Create workspace", "Create"),
    ]);
    let args = run_dialog(preset(), &mut dialog).await;
    assert_eq!(args.profile.as_deref(), Some("production"));
    assert_eq!(args.model.as_deref(), Some("model-override"));
    assert_eq!(
        dialog.initial_text[0],
        ("Profile name".into(), "dev".into())
    );
}

#[tokio::test]
async fn abandoning_message_edit_preserves_old_action_and_message() {
    let mut dialog = Dialog::new([
        Pick("Create workspace", "Edit settings"),
        Pick("Edit settings", "After create"),
        Pick("After create", "Send message"),
        Back("Message"),
        Back("After create"),
        Back("Edit settings"),
        Pick("Create workspace", "Create"),
    ]);
    let args = run_dialog(preset(), &mut dialog).await;
    assert_eq!(args.send.as_deref(), Some("Original message"));
    assert!(args.jump);
    assert_eq!(dialog.initial_text[0].1, "Original message");
}

#[tokio::test]
async fn accepted_post_action_edit_replaces_message_and_jump_together() {
    let mut dialog = Dialog::new([
        Pick("Create workspace", "Edit settings"),
        Pick("Edit settings", "After create"),
        Pick("After create", "Send message"),
        Back("Message"),
        Pick("After create", "Return to shell"),
        Pick("Create workspace", "Create"),
    ]);
    let args = run_dialog(preset(), &mut dialog).await;
    assert!(args.send.is_none() && !args.jump);
}

#[tokio::test]
async fn context_size_back_does_not_commit_the_pending_source() {
    let mut dialog = Dialog::new([
        Pick("Create workspace", "Edit settings"),
        Pick("Edit settings", "Context"),
        Pick("Context", "Thread ID"),
        Text("Codex thread ID", "other"),
        Back("Context size"),
        Back("Context"),
        Back("Edit settings"),
        Pick("Create workspace", "Create"),
    ]);
    let args = run_dialog(preset(), &mut dialog).await;
    assert_eq!(args.context.as_deref(), Some("thread:source"));
    assert!(args.compact_context);
}

#[tokio::test]
async fn editing_legacy_fork_code_keeps_the_independent_context() {
    let args = create_args(&["coco", "create", "demo", "-i", "--fork-from", "source"]);
    let mut dialog = Dialog::new([
        Tab("Worktree"),
        Pick("Create workspace", "Edit settings"),
        Pick("Edit settings", "Code"),
        Pick("Code", "Current checkout"),
        Pick("Create workspace", "Create"),
    ]);
    let args = run_dialog(args, &mut dialog).await;
    assert!(args.fork_from.is_none() && args.base_workspace.is_none());
    assert_eq!(args.context.as_deref(), Some("workspace:source"));
    let (params, _, _) = normalize_create_args("/repo".into(), args, "split-fork".into()).unwrap();
    assert!(matches!(
        params.context,
        WorkspaceContextRequest::Fork { .. }
    ));
}

#[tokio::test]
async fn switching_from_base_to_branch_adoption_does_not_emit_conflicting_flags() {
    let mut dialog = Dialog::new([
        Pick("Create workspace", "Edit settings"),
        Pick("Edit settings", "Code"),
        Pick("Code", "Current checkout"),
        Pick("Create workspace", "Edit settings"),
        Pick("Edit settings", "Worktree"),
        Pick("Worktree", "Existing branch"),
        Pick("Use branch", "review/existing-branch"),
        Pick("Create workspace", "Create"),
    ]);
    let args = run_dialog(preset(), &mut dialog).await;
    assert_eq!(args.checkout.as_deref(), Some("review/existing-branch"));
    assert!(args.base.is_none() && args.branch.is_none() && !args.detached);
    normalize_create_args("/repo".into(), args, "adopt".into()).unwrap();
}

#[tokio::test]
async fn editing_name_refreshes_git_review_instead_of_reusing_previous_sha() {
    let mut changed = git_state();
    changed.head_sha = Some("b".repeat(40));
    let inspector = FakeGitInspector {
        states: Mutex::new(VecDeque::from([git_state(), git_state(), changed])),
        ..FakeGitInspector::default()
    };
    let mut dialog = Dialog::new([
        Tab("Worktree"),
        Pick("Create workspace", "Edit settings"),
        Pick("Edit settings", "Workspace"),
        Text("Workspace name", "new"),
        Pick("Create workspace", "Create"),
    ]);
    let args = super::super::super::walkthrough_with_inspector(
        &test_paths(),
        "/repo".into(),
        named(),
        &mut dialog,
        &inspector,
    )
    .await
    .unwrap();
    assert_eq!(args.expected_base_sha, Some("b".repeat(40)));
    assert_eq!(dialog.value("Code"), "main · bbbbbbbb");
    assert_eq!(inspector.calls.lock().unwrap().len(), 3);
    assert!(
        dialog
            .notices
            .iter()
            .any(|message| message.contains("checkout changed"))
    );
    dialog.consumed();
}
