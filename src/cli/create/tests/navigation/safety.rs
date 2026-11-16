use std::sync::atomic::{AtomicBool, Ordering};

use super::*;

struct FailOnce {
    review: bool,
    failed: AtomicBool,
    inner: FakeGitInspector,
}

#[async_trait]
impl super::super::super::git_source::GitInspector for FailOnce {
    async fn inspect(
        &self,
        params: crate::protocol::RepositoryGitInspectParams,
    ) -> Result<crate::protocol::RepositoryGitState> {
        let target = if self.review {
            params.base.is_some()
        } else {
            params.include_branches
        };
        if target && !self.failed.swap(true, Ordering::SeqCst) {
            anyhow::bail!("read-only discovery temporarily unavailable");
        }
        super::super::super::git_source::GitInspector::inspect(&self.inner, params).await
    }
}

#[tokio::test]
async fn retry_reopens_failed_discovery_without_losing_prior_answers() {
    let inspector = FailOnce {
        review: false,
        failed: AtomicBool::new(false),
        inner: FakeGitInspector::default(),
    };
    let mut dialog = Dialog::new([
        Pick("Worktree", "Detached HEAD"),
        Pick("Code", "Branch"),
        Retry("Code"),
        Pick("Code", "Branch"),
        Pick("Code from branch", "review/existing-branch"),
        Tab("Context"),
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
    assert!(args.detached);
    assert_eq!(
        dialog
            .focus
            .iter()
            .filter(|(title, _)| title == "Code")
            .nth(1)
            .unwrap()
            .1
            .as_deref(),
        Some("Branch")
    );
    assert_eq!(
        args.base.as_deref(),
        Some("refs/heads/review/existing-branch")
    );
    assert!(
        dialog
            .notices
            .iter()
            .any(|message| message.contains("discovery temporarily unavailable"))
    );
    dialog.consumed();
}

#[tokio::test]
async fn back_after_discovery_failure_rolls_back_and_returns_to_previous_step() {
    let inspector = FailOnce {
        review: false,
        failed: AtomicBool::new(false),
        inner: FakeGitInspector::default(),
    };
    let mut dialog = Dialog::new([
        Pick("Worktree", "New branch"),
        Pick("Code", "Branch"),
        Back("Code"),
        Pick("Worktree", "Detached HEAD"),
        Pick("Code", "Current checkout"),
        Tab("Git changes"),
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
    assert!(args.detached && args.base.is_none());
    dialog.consumed();
}

#[tokio::test]
async fn a_failed_review_must_be_refreshed_before_confirmation_is_available() {
    let inspector = FailOnce {
        review: true,
        failed: AtomicBool::new(false),
        inner: FakeGitInspector::default(),
    };
    let mut dialog = Dialog::new([
        Tab("Worktree"),
        Retry("Create workspace"),
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
    assert_eq!(dialog.reviews.len(), 1);
    assert_eq!(args.expected_base_sha, Some("a".repeat(40)));
    dialog.consumed();
}

#[tokio::test]
async fn incompatible_code_and_carry_require_explicit_correction_not_silent_data_loss() {
    let mut other = git_state();
    other.base_sha = Some("b".repeat(40));
    let inspector = FakeGitInspector {
        states: Mutex::new(VecDeque::from([
            git_state(),
            other.clone(),
            git_state(),
            other,
        ])),
        ..FakeGitInspector::default()
    };
    let args = create_args(&["coco", "create", "demo", "-i", "--base", "other", "--dirty"]);
    let mut dialog = Dialog::new([
        Tab("Worktree"),
        Pick("Edit settings", "Git changes"),
        Pick("Git changes", "Don't copy"),
        Pick("Create workspace", "Create"),
    ]);
    let args = super::super::super::walkthrough_with_inspector(
        &test_paths(),
        "/repo".into(),
        args,
        &mut dialog,
        &inspector,
    )
    .await
    .unwrap();
    assert!(!args.dirty && !args.carry_changes && !args.carry_untracked);
    assert_eq!(args.base.as_deref(), Some("other"));
    assert_eq!(args.expected_base_sha, Some("b".repeat(40)));
    assert!(
        dialog
            .notices
            .iter()
            .any(|message| message.contains("Change Code or Git changes"))
    );
    dialog.consumed();
}

#[tokio::test]
async fn editing_cannot_replace_the_original_repository_identity_anchor() {
    let mut replaced = git_state();
    replaced.repository_id = "foreign-repository".into();
    let inspector = FakeGitInspector {
        states: Mutex::new(VecDeque::from([git_state(), git_state(), replaced])),
        ..FakeGitInspector::default()
    };
    let mut dialog = Dialog::new([
        Tab("Worktree"),
        Pick("Create workspace", "Edit settings"),
        Pick("Edit settings", "Git changes"),
    ]);
    let result = super::super::super::walkthrough_with_inspector(
        &test_paths(),
        "/repo".into(),
        named(),
        &mut dialog,
        &inspector,
    )
    .await;
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("selected repository changed")
    );
    assert_eq!(dialog.reviews.len(), 1);
    dialog.consumed();
}

#[test]
fn only_typed_back_is_consumed_by_a_parent() {
    assert!(
        parent_choice::<()>(Err(NavigationExit::Back.into()))
            .unwrap()
            .is_none()
    );
    assert!(parent_choice::<()>(Err(NavigationExit::Cancel.into())).is_err());
    assert_eq!(
        parent_choice::<()>(Err(anyhow::anyhow!("back")))
            .unwrap_err()
            .to_string(),
        "back"
    );
}

#[tokio::test]
async fn editing_an_explicit_empty_carry_policy_keeps_it_until_a_real_choice() {
    let mut clean = git_state();
    clean.changes.tracked_files = 0;
    let inspector = FakeGitInspector {
        states: Mutex::new(VecDeque::from([
            clean.clone(),
            clean.clone(),
            clean.clone(),
            clean,
        ])),
        ..FakeGitInspector::default()
    };
    let args = create_args(&["coco", "create", "demo", "-i", "--dirty"]);
    let mut dialog = Dialog::new([
        Tab("Worktree"),
        Pick("Create workspace", "Edit settings"),
        Pick("Edit settings", "Git changes"),
        Pick("Git changes", "Copy tracked + untracked"),
        Pick("Create workspace", "Create"),
    ]);
    let args = super::super::super::walkthrough_with_inspector(
        &test_paths(),
        "/repo".into(),
        args,
        &mut dialog,
        &inspector,
    )
    .await
    .unwrap();
    assert!(args.carry_changes && args.carry_untracked);
    assert_eq!(dialog.value("Git changes"), "copy tracked + untracked");
    dialog.consumed();
}

#[tokio::test]
async fn late_untracked_files_cannot_reuse_a_tracked_only_answer_by_row_number() {
    let mut late = git_state();
    late.changes.untracked_files = 1;
    let inspector = FakeGitInspector {
        states: Mutex::new(VecDeque::from([
            git_state(),
            git_state(),
            late.clone(),
            late,
        ])),
        ..FakeGitInspector::default()
    };
    let args = create_args(&["coco", "create", "demo", "-i", "--carry-changes"]);
    let mut dialog = Dialog::new([
        Tab("Worktree"),
        Pick("Create workspace", "Edit settings"),
        Pick("Edit settings", "Git changes"),
        Pick("Git changes", "Don't copy"),
        Pick("Create workspace", "Create"),
    ]);
    let args = super::super::super::walkthrough_with_inspector(
        &test_paths(),
        "/repo".into(),
        args,
        &mut dialog,
        &inspector,
    )
    .await
    .unwrap();
    assert!(!args.carry_changes && !args.carry_untracked);
    assert!(
        dialog
            .notices
            .iter()
            .any(|message| message.contains("no longer available"))
    );
    dialog.consumed();
}

#[tokio::test]
#[ignore = "manual recovery probe; requires an interactive PTY, fake discovery only"]
async fn interactive_recovery_walkthrough_probe() {
    let inspector = FailOnce {
        review: true,
        failed: AtomicBool::new(false),
        inner: FakeGitInspector::default(),
    };
    let mut interaction = crate::cli::prompt::TerminalInteraction::detect(false);
    let result = super::super::super::walkthrough_with_inspector(
        &test_paths(),
        "/unreachable/coco-recovery-probe".into(),
        named(),
        &mut interaction,
        &inspector,
    )
    .await;
    assert!(!crossterm::terminal::is_raw_mode_enabled().unwrap());
    assert!(
        result.is_err(),
        "terminal script must cancel, never accept creation"
    );
    eprintln!("recovery-result={result:?}");
}
