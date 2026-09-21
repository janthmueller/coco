use super::*;
use serde_json::json;

#[test]
fn extracts_the_latest_safe_reasoning_heading() {
    assert_eq!(
        latest_summary_line("# First\n<!-- hidden -->\n**Checking\t tests**:\u{1b} now"),
        Some("Checking tests: now".to_owned())
    );
    assert_eq!(latest_summary_line("**Incomplete"), None);
}

#[test]
fn bounds_unicode_labels_without_splitting_characters() {
    let (label, truncated) = truncate(&"🚀".repeat(121), MAX_ACTIVITY_LABEL_CHARS);
    assert!(truncated);
    assert_eq!(label.chars().count(), MAX_ACTIVITY_LABEL_CHARS);
    assert!(label.ends_with('…'));
}

#[test]
fn bounded_reasoning_buffer_keeps_the_newest_heading() {
    let mut buffer = "x".repeat(MAX_REASONING_BUFFER_CHARS);
    push_bounded(&mut buffer, "\n**Newest heading**");
    assert!(buffer.chars().count() <= MAX_REASONING_BUFFER_CHARS);
    assert_eq!(
        latest_summary_line(&buffer),
        Some("Newest heading".to_owned())
    );
}

#[test]
fn structured_compaction_outranks_and_clears_reasoning() {
    let mut registry = ActivityRegistry::default();
    registry.observe(
        "turn/started",
        &json!({"threadId": "thread", "turn": {"id": "turn"}}),
        "generation",
    );
    registry.observe(
        "item/started",
        &json!({
            "threadId": "thread",
            "turnId": "turn",
            "item": {"id": "reasoning", "type": "reasoning"}
        }),
        "generation",
    );
    registry.observe(
        "item/reasoning/summaryTextDelta",
        &json!({
            "threadId": "thread",
            "turnId": "turn",
            "itemId": "reasoning",
            "summaryIndex": 0,
            "delta": "**Checking tests**"
        }),
        "generation",
    );
    registry.observe(
        "item/started",
        &json!({
            "threadId": "thread",
            "turnId": "turn",
            "item": {"id": "compact", "type": "contextCompaction"}
        }),
        "generation",
    );
    assert_eq!(
        registry.snapshot("thread").map(|value| value.source),
        Some(WorkspaceActivitySource::ContextCompaction)
    );
    registry.observe(
        "item/completed",
        &json!({
            "threadId": "thread",
            "turnId": "turn",
            "item": {"id": "compact", "type": "contextCompaction"}
        }),
        "generation",
    );
    assert_eq!(registry.snapshot("thread"), None);
}

#[test]
fn reasoning_heading_survives_its_item_but_not_the_turn() {
    let mut registry = ActivityRegistry::default();
    registry.observe(
        "turn/started",
        &json!({"threadId": "thread", "turn": {"id": "turn"}}),
        "generation",
    );
    registry.observe(
        "item/started",
        &json!({
            "threadId": "thread",
            "turnId": "turn",
            "item": {"id": "reasoning", "type": "reasoning"}
        }),
        "generation",
    );
    registry.observe(
        "item/reasoning/summaryTextDelta",
        &json!({
            "threadId": "thread",
            "turnId": "turn",
            "itemId": "reasoning",
            "summaryIndex": 0,
            "delta": "**Checking tests**"
        }),
        "generation",
    );
    registry.observe(
        "item/completed",
        &json!({
            "threadId": "thread",
            "turnId": "turn",
            "item": {"id": "reasoning", "type": "reasoning"}
        }),
        "generation",
    );
    registry.observe(
        "item/reasoning/summaryTextDelta",
        &json!({
            "threadId": "thread",
            "turnId": "turn",
            "itemId": "reasoning",
            "summaryIndex": 0,
            "delta": "**Late replacement**"
        }),
        "generation",
    );
    assert_eq!(
        registry.snapshot("thread").map(|value| value.label),
        Some("Checking tests".to_owned())
    );

    registry.observe(
        "turn/completed",
        &json!({"threadId": "thread", "turn": {"id": "turn"}}),
        "generation",
    );
    assert_eq!(registry.snapshot("thread"), None);
}
