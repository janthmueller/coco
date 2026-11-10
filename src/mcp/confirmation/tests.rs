use super::*;

#[test]
fn only_a_positive_exact_host_reply_authorizes_the_action() {
    assert!(accepted(
        &ElicitResult::new(ElicitationAction::Accept).with_content(json!({"confirm":true}))
    ));
    for action in [
        ElicitationAction::Accept,
        ElicitationAction::Decline,
        ElicitationAction::Cancel,
    ] {
        for content in [
            json!(null),
            json!({}),
            json!({"confirm":false}),
            json!({"confirm":"true"}),
            json!({"confirm":true,"extra":true}),
        ] {
            assert!(!accepted(
                &ElicitResult::new(action.clone()).with_content(content)
            ));
        }
    }
}

#[test]
fn review_is_derived_from_actual_inputs_without_terminal_injection_or_recipe_contents() {
    let input = json!({"operationId":"creation-key", "name":"fix/login", "worktree":{"kind":"newBranch", "base":{"kind":"revision","revision":"main"}},
        "changes":"carryTrackedAndUntracked", "profile":"default", "context":{"kind":"fork","workspace":"feat/auth","compact":false}});
    let request = serde_json::from_value(input).unwrap();
    let review = create_review(Path::new("/projects/frontend"), &request);
    for value in [
        "Create workspace",
        "/projects/frontend",
        "fix/login",
        "main",
        "feat/auth",
        "copy tracked + untracked changes",
        "prepare only",
    ] {
        assert!(review.contains(value), "{value}: {review}");
    }
    let review = send_review(
        Path::new("/projects/backend"),
        &WorkspaceSendInput {
            workspace: "fix/login".into(),
            message: "first\nRepository   fake\u{1b}[2J".into(),
            operation_id: Some("send-key".into()),
        },
    );
    assert!(review.contains("first\\nRepository"));
    assert!(!review.contains('\u{1b}'));
    assert!(
        interrupt_review(
            Path::new("/repo"),
            "fix/login",
            "old-key",
            crate::protocol::TurnInterruptMode::BestEffort
        )
        .contains("newer turn could be interrupted")
    );
}
