use super::*;
use crate::protocol::{GitChangeSummary, RepositoryGitInspectParams, RepositoryGitState};

fn inspector(states: Vec<RepositoryGitState>) -> FakeGitInspector {
    FakeGitInspector {
        states: Mutex::new(states.into()),
        ..Default::default()
    }
}

fn choices() -> ScriptedInteraction {
    let mut interaction = ScriptedInteraction::new([0], [] as [&str; 0]);
    interaction.defaults_at = Some("Worktree".into());
    interaction
}

async fn run(states: Vec<RepositoryGitState>, interaction: &mut ScriptedInteraction) -> CreateArgs {
    super::super::walkthrough_with_inspector(
        &test_paths(),
        "/repo".into(),
        create_args(&["coco", "create", "test/new", "-i"]),
        interaction,
        &inspector(states),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn clean_checkout_skips_carry_and_final_review_shows_actual_branch_and_commit() {
    let mut clean = git_state();
    clean.changes = GitChangeSummary::default();
    let mut interaction = choices();
    let args = run(vec![clean.clone(), clean], &mut interaction).await;
    assert!(
        !interaction
            .titles
            .iter()
            .any(|title| title == "Git changes")
    );
    assert_eq!(final_review_value(&interaction, "Code"), "main · aaaaaaaa");
    assert_eq!(args.expected_base_sha, Some("a".repeat(40)));
    assert!(!args.carry_changes && !args.carry_untracked);
    interaction.assert_consumed();
}

#[tokio::test]
async fn carry_choices_match_the_observed_categories_without_useless_or_blocked_options() {
    for (tracked, untracked, label) in [
        (1, 0, "Copy tracked"),
        (0, 2, "Copy tracked + untracked"),
        (2, 3, "Copy tracked + untracked"),
    ] {
        let mut state = git_state();
        state.changes = GitChangeSummary {
            tracked_files: tracked,
            untracked_files: untracked,
        };
        let mut interaction = ScriptedInteraction::new([0, 0, 1, 0, 0, 0, 0, 0], [] as [&str; 0]);
        let args = run(vec![state.clone(), state], &mut interaction).await;
        let offered = choices_for_title(&interaction, "Git changes");
        assert_eq!(offered.len(), 2);
        assert_eq!(offered[1].label, label);
        assert!(args.carry_changes);
        assert_eq!(args.carry_untracked, untracked > 0);
        interaction.assert_consumed();
    }
}

#[tokio::test]
async fn late_untracked_files_reopen_selection_and_do_not_upgrade_tracked_carry() {
    let before = git_state();
    let mut after = before.clone();
    after.changes.untracked_files = 1;
    let mut interaction = ScriptedInteraction::new([0, 0, 1, 0, 0, 0, 0, 0, 0], [] as [&str; 0]);
    let args = run(vec![before, after], &mut interaction).await;
    assert_eq!(
        interaction
            .titles
            .iter()
            .filter(|title| *title == "Git changes")
            .count(),
        2
    );
    assert!(!args.carry_changes && !args.carry_untracked && !args.dirty);
    assert_eq!(
        final_review_value(&interaction, "Git changes"),
        "don't copy"
    );
    interaction.assert_consumed();
}

#[tokio::test]
async fn branch_base_picker_includes_current_and_remote_refs_but_checkout_only_free_locals() {
    let mut interaction = ScriptedInteraction::new([0, 1, 2, 0, 0, 0, 0, 0], [] as [&str; 0]);
    let args = run(vec![], &mut interaction).await;
    assert_eq!(args.base.as_deref(), Some("refs/remotes/origin/main"));
    let offered = choices_for_title(&interaction, "Code from branch");
    assert_eq!(
        offered
            .iter()
            .map(|choice| choice.label.as_str())
            .collect::<Vec<_>>(),
        ["main", "review/existing-branch", "origin/main"]
    );
    interaction.assert_consumed();
    let mut interaction = ScriptedInteraction::new([2, 0, 0, 0, 0, 0, 0], [] as [&str; 0]);
    let args = run(vec![], &mut interaction).await;
    assert_eq!(args.checkout.as_deref(), Some("review/existing-branch"));
    assert_eq!(choices_for_title(&interaction, "Use branch").len(), 1);
    interaction.assert_consumed();
}

#[tokio::test]
async fn detached_checkout_has_an_explicit_commit_label() {
    let mut state = git_state();
    state.branch = None;
    let mut interaction = choices();
    run(vec![state.clone(), state], &mut interaction).await;
    assert_eq!(
        final_review_value(&interaction, "Code"),
        "Detached HEAD · aaaaaaaa"
    );
}

#[tokio::test]
async fn remote_base_pins_its_own_commit_and_unborn_head_is_never_a_default() {
    let before = git_state();
    let mut branches = before.clone();
    branches.branches[2].commit = "b".repeat(40);
    let mut review = before.clone();
    review.base_sha = Some("b".repeat(40));
    let mut interaction = ScriptedInteraction::new([0, 1, 2, 0, 0, 0, 0, 0], [] as [&str; 0]);
    let args = run(vec![before.clone(), branches, review], &mut interaction).await;
    assert_eq!(args.expected_base_sha, Some("b".repeat(40)));
    assert_eq!(
        final_review_value(&interaction, "Code"),
        "origin/main · bbbbbbbb"
    );
    interaction.assert_consumed();

    let mut unborn = before;
    unborn.head_sha = None;
    unborn.branches.retain(|branch| branch.remote);
    let mut review = unborn.clone();
    review.base_sha = Some("b".repeat(40));
    let mut interaction = ScriptedInteraction::new([0, 0, 0], [] as [&str; 0]);
    interaction.defaults_at = Some("Worktree".into());
    let args = run(vec![unborn.clone(), unborn, review], &mut interaction).await;
    assert!(
        choices_for_title(&interaction, "Code")
            .iter()
            .all(|choice| choice.label != "Current checkout" && !choice.is_default)
    );
    assert_eq!(args.base.as_deref(), Some("refs/remotes/origin/main"));
    interaction.assert_consumed();
}

#[tokio::test]
async fn newly_dirty_checkout_is_reconsidered_and_explicit_carry_presets_stay_explicit() {
    let mut clean = git_state();
    clean.changes = GitChangeSummary::default();
    let dirty = git_state();
    let mut interaction = ScriptedInteraction::new([0, 0, 0, 0, 0, 0, 0, 0], [] as [&str; 0]);
    let args = run(vec![clean.clone(), dirty], &mut interaction).await;
    assert!(!args.carry_changes && !args.carry_untracked);
    assert_eq!(
        interaction
            .titles
            .iter()
            .filter(|title| *title == "Git changes")
            .count(),
        1
    );
    interaction.assert_consumed();
    let mut interaction = choices();
    let args = super::super::walkthrough_with_inspector(
        &test_paths(),
        "/repo".into(),
        create_args(&["coco", "create", "test/new", "-i", "-d"]),
        &mut interaction,
        &inspector(vec![clean.clone(), clean]),
    )
    .await
    .unwrap();
    assert!(args.dirty);
    assert!(
        !interaction
            .titles
            .iter()
            .any(|title| title == "Git changes")
    );
    interaction.assert_consumed();
}

#[derive(Default)]
struct RpcGit {
    seen: Mutex<Vec<RepositoryGitInspectParams>>,
    failure: Option<RpcErrorPayload>,
    fail_during_preview: bool,
}

#[async_trait]
impl RpcHandler for RpcGit {
    async fn handle(&self, method: &str, params: Value) -> Result<Value, RpcErrorPayload> {
        assert_eq!(method, "repository.git.inspect");
        let params: RepositoryGitInspectParams = serde_json::from_value(params).unwrap();
        let preview = params.base.is_some();
        self.seen.lock().unwrap().push(params);
        if let Some(error) = &self.failure
            && (!self.fail_during_preview || preview)
        {
            return Err(error.clone());
        }
        let mut state = git_state();
        state.changes = GitChangeSummary::default();
        if preview {
            state.base_sha = state.head_sha.clone();
        }
        Ok(serde_json::to_value(state).unwrap())
    }
}

async fn walkthrough_via_rpc(
    handler: Arc<RpcGit>,
    interaction: &mut ScriptedInteraction,
) -> Result<CreateArgs> {
    let root = tempdir().unwrap();
    let paths = test_paths_at(root.path().to_path_buf());
    let server = RpcServer::bind(&paths.socket_path, handler).await.unwrap();
    let (stop, receiver) = watch::channel(false);
    let task = tokio::spawn(server.run(receiver));
    let args = super::super::walkthrough(
        &paths,
        "/repo/nested".into(),
        create_args(&["coco", "create", "test/new", "-i"]),
        interaction,
    )
    .await;
    stop.send(true).unwrap();
    task.await.unwrap().unwrap();
    args
}

#[tokio::test]
async fn real_walkthrough_uses_read_only_typed_git_rpc_and_preserves_the_selected_path() {
    let handler = Arc::new(RpcGit::default());
    let mut interaction = choices();
    let args = walkthrough_via_rpc(handler.clone(), &mut interaction).await;
    assert_eq!(args.unwrap().expected_base_sha, Some("a".repeat(40)));
    let seen = handler.seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert!(
        seen.iter()
            .all(|params| params.path == Path::new("/repo/nested"))
    );
    assert!(seen[0].base.is_none() && seen[1].base.is_some());
    interaction.assert_consumed();
}

#[tokio::test]
async fn empty_branch_picker_returns_to_code_selection_without_losing_current_checkout() {
    let source = git_state();
    let mut empty = source.clone();
    empty.branches.clear();
    let mut interaction = ScriptedInteraction::new([0, 1, 0, 0, 0, 0, 0, 0, 0], [] as [&str; 0]);
    let args = run(vec![source.clone(), empty, source], &mut interaction).await;
    assert!(args.base.is_none() && args.checkout.is_none());
    assert_eq!(args.expected_base_sha, Some("a".repeat(40)));
    assert_eq!(
        interaction.notices,
        ["No branch found. Choose another code source."]
    );
    assert_eq!(
        interaction
            .titles
            .iter()
            .filter(|title| *title == "Code")
            .count(),
        2
    );
    assert!(
        !interaction
            .titles
            .iter()
            .any(|title| title == "Code from branch")
    );
    interaction.assert_consumed();
}

#[tokio::test]
async fn no_free_local_branch_returns_to_worktree_selection_without_adopting_a_remote() {
    let source = git_state();
    let mut occupied = source.clone();
    occupied
        .branches
        .retain(|branch| branch.remote || branch.checkout_path.is_some());
    let mut interaction = ScriptedInteraction::new([2, 0, 0, 0, 0, 0, 0, 0, 0], [] as [&str; 0]);
    let args = run(vec![source.clone(), occupied, source], &mut interaction).await;
    assert!(args.checkout.is_none() && args.branch.is_none() && !args.detached);
    assert_eq!(
        interaction.notices,
        ["No free local branch found. Create a new branch instead."]
    );
    assert_eq!(
        interaction
            .titles
            .iter()
            .filter(|title| *title == "Worktree")
            .count(),
        2
    );
    assert!(!interaction.titles.iter().any(|title| title == "Use branch"));
    assert_eq!(
        final_review_value(&interaction, "Worktree"),
        "branch test/new"
    );
    interaction.assert_consumed();
}

#[tokio::test]
async fn old_daemon_gets_a_restart_hint_but_other_git_errors_are_preserved() {
    for code in ["METHOD_NOT_FOUND", "GIT_ERROR"] {
        let handler = Arc::new(RpcGit {
            failure: Some(RpcErrorPayload::new(
                code,
                "synthetic Git inspection failure",
            )),
            ..Default::default()
        });
        let mut interaction = ScriptedInteraction::new([], [] as [&str; 0]);
        let error = walkthrough_via_rpc(handler.clone(), &mut interaction)
            .await
            .unwrap_err()
            .to_string();
        if code == "METHOD_NOT_FOUND" {
            assert!(error.contains("matching cocod version") && error.contains("restart cocod"));
            assert!(!error.contains("synthetic Git inspection failure"));
        } else {
            assert!(error.contains("GIT_ERROR: synthetic Git inspection failure"));
            assert!(!error.contains("restart cocod"));
        }
        assert!(interaction.titles.is_empty());
        let calls = handler.seen.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert!(calls[0].base.is_none() && !calls[0].include_branches);
        interaction.assert_consumed();
    }
}

#[tokio::test]
async fn deleted_branch_preview_error_stops_before_confirmation_or_creation() {
    let handler = Arc::new(RpcGit {
        failure: Some(RpcErrorPayload::new(
            "GIT_ERROR",
            "selected branch no longer exists",
        )),
        fail_during_preview: true,
        ..Default::default()
    });
    let mut interaction = ScriptedInteraction::new([0, 1, 1, 0, 0, 0, 0], [] as [&str; 0]);
    let error = walkthrough_via_rpc(handler.clone(), &mut interaction)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("GIT_ERROR: selected branch no longer exists"));
    assert!(
        !interaction
            .titles
            .iter()
            .any(|title| title == "Create workspace")
    );
    let calls = handler.seen.lock().unwrap();
    assert_eq!(calls.len(), 3);
    assert!(calls[0].base.is_none() && !calls[0].include_branches);
    assert!(calls[1].base.is_none() && calls[1].include_branches);
    assert_eq!(
        calls[2].base,
        Some(WorkspaceBaseRequest::Revision {
            revision: "refs/heads/review/existing-branch".into(),
        })
    );
    assert!(!calls[2].include_branches);
    interaction.assert_consumed();
}
