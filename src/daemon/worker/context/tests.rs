use super::*;

mod pagination;

#[test]
fn selects_latest_terminal_turn_and_excludes_active_suffix() {
    let page = decode_turn_page(json!({"data": [
        {"id": "working", "status": "inProgress"},
        {"id": "latest", "status": "completed"},
        {"id": "older", "status": "completed"},
    ], "nextCursor": null}))
    .unwrap();
    assert_eq!(completed_boundary(&page).unwrap(), Some("latest"));
}

#[test]
fn all_native_terminal_statuses_are_valid_fork_boundaries() {
    for status in ["completed", "interrupted", "failed"] {
        let page = decode_turn_page(json!({"data": [
            {"id": "boundary", "status": status},
        ], "nextCursor": null}))
        .unwrap();
        assert_eq!(completed_boundary(&page).unwrap(), Some("boundary"));
    }
}

#[test]
fn empty_or_active_only_page_does_not_invent_a_boundary() {
    for data in [
        json!([]),
        json!([{"id": "working", "status": "inProgress"}]),
    ] {
        let page = decode_turn_page(json!({"data": data, "nextCursor": null})).unwrap();
        assert_eq!(completed_boundary(&page).unwrap(), None);
    }
}

#[test]
fn unsupported_status_does_not_silently_fall_back_to_older_history() {
    let page = decode_turn_page(json!({"data": [
        {"id": "new", "status": "futureStatus"},
        {"id": "old", "status": "completed"},
    ], "nextCursor": null}))
    .unwrap();
    assert!(
        completed_boundary(&page)
            .unwrap_err()
            .to_string()
            .contains("unsupported")
    );
}

#[test]
fn invalid_ids_and_malformed_turns_fail_closed() {
    let page = decode_turn_page(json!({"data": [
        {"id": "  ", "status": "completed"},
    ], "nextCursor": null}))
    .unwrap();
    assert!(completed_boundary(&page).is_err());
    assert!(decode_turn_page(json!({"data": [{}], "nextCursor": null})).is_err());
    assert!(decode_turn_page(json!({"data": null})).is_err());
}

#[test]
fn refuses_unbounded_page_data() {
    let data = vec![json!({"id": "turn", "status": "inProgress"}); TURN_PAGE_LIMIT + 1];
    assert!(decode_turn_page(json!({"data": data, "nextCursor": null})).is_err());
}

#[test]
fn pagination_rejects_empty_repeated_and_cycling_cursors() {
    let mut seen = HashSet::new();
    for cursor in ["", " ", "\n"] {
        assert!(validate_next_cursor(cursor, &mut seen).is_err());
    }
    validate_next_cursor("page-2", &mut seen).unwrap();
    assert!(validate_next_cursor("page-2", &mut seen).is_err());
    validate_next_cursor("page-3", &mut seen).unwrap();
    assert!(validate_next_cursor("page-2", &mut seen).is_err());
}

#[test]
fn synthetic_legacy_boundaries_fail_without_selecting_an_older_turn() {
    for status in ["completed", "interrupted", "failed"] {
        let page = decode_turn_page(json!({"data": [
            {"id": "rollout-2", "status": status},
            {"id": "older-canonical", "status": "completed"},
        ], "nextCursor": null}))
        .unwrap();
        assert!(matches!(
            completed_boundary(&page),
            Err(WorkerError::ContextBoundaryUnsupported)
        ));
    }
    for id in [
        "rollout-",
        "rollout-abc",
        "turn-1",
        "0192a058-0000-7000-8000-000000000123",
    ] {
        let page = decode_turn_page(json!({"data": [
            {"id": id, "status": "completed"},
        ], "nextCursor": null}))
        .unwrap();
        assert_eq!(completed_boundary(&page).unwrap(), Some(id));
    }
}

#[test]
fn native_canonical_boundary_rejection_gets_actionable_safe_handling() {
    let rejection = CodexError::Rpc {
        code: -32600,
        message:
            "lastTurnId 'private-turn-id' is not a persisted canonical turn in the source thread"
                .to_owned(),
        data: Some(json!({"secret": "must-not-escape"})),
    };
    let error = fork_error(rejection.clone(), true);
    assert!(matches!(error, WorkerError::ContextBoundaryUnsupported));
    assert!(error.to_string().contains("Finish a new turn"));
    assert!(!error.to_string().contains("private-turn-id"));
    assert!(!error.to_string().contains("must-not-escape"));
    assert!(matches!(
        fork_error(rejection, false),
        WorkerError::Runtime(_)
    ));
    for error in [
        CodexError::Rpc {
            code: -32600,
            message: "source is busy".to_owned(),
            data: None,
        },
        CodexError::Rpc {
            code: -32000,
            message: "lastTurnId 't' is not a persisted canonical turn in the source thread"
                .to_owned(),
            data: None,
        },
        CodexError::Closed {
            reason: "lost reply".to_owned(),
            stderr: String::new(),
        },
    ] {
        assert!(matches!(fork_error(error, true), WorkerError::Runtime(_)));
    }
}
