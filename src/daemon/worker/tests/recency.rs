use super::*;

#[test]
fn native_recency_is_optional_validated_and_normalized_without_reading_turns() {
    for (value, expected) in [
        (json!(1_800_000_000), Some(1_800_000_000_000)),
        (json!(0), Some(0)),
        (json!(null), None),
        (json!(-1), None),
        (json!(i64::MAX), None),
        (json!(1.25), None),
        (json!("yesterday"), None),
        (json!({"unexpected":true}), None),
    ] {
        let native = decode_thread_read_response(json!({"thread": {
            "id":"thread-1", "cwd":"/worktree", "status":{"type":"notLoaded"},
            "updatedAt":1_900_000_000, "recencyAt":value
        }}))
        .unwrap();
        assert_eq!(native.recency_at_ms, expected);
        assert_eq!(native.status, CodexThreadStatus::NotLoaded);
    }
    let native = decode_thread_read_response(json!({"thread": {
        "id":"thread-1", "cwd":"/worktree", "status":{"type":"idle"}, "updatedAt":1_900_000_000
    }}))
    .unwrap();
    assert!(native.recency_at_ms.is_none());
}
