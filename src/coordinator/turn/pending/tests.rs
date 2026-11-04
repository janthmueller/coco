use super::*;

#[test]
fn distinct_candidates_keep_completion_and_output_in_both_event_orders() {
    for own_first in [false, true] {
        let mut pending = PendingTurnObservations::default();
        let order = if own_first {
            ["own", "other"]
        } else {
            ["other", "own"]
        };
        for id in order {
            pending.observe_output(id, id);
            pending.observe_completion(id, TurnTerminalStatus::Completed);
        }
        let (own, lost) = pending.resolve("own");
        assert!(!lost);
        assert_eq!(own.completion, Some(TurnTerminalStatus::Completed));
        assert_eq!(own.output.unwrap().response, "own");
    }
}

#[test]
fn overflow_preserves_admitted_candidates_and_marks_unknown_ones_lost() {
    let mut pending = PendingTurnObservations::default();
    for index in 0..MAX_PENDING_TURNS {
        pending.observe_completion(&format!("turn-{index}"), TurnTerminalStatus::Completed);
    }
    pending.observe_output("overflow", "discarded");
    assert!(pending.overflowed);
    assert_eq!(pending.entries.len(), MAX_PENDING_TURNS);
    pending.observe_output("turn-0", "still retained");
    let (retained, lost) = pending.clone().resolve("turn-0");
    assert!(!lost);
    assert_eq!(retained.completion, Some(TurnTerminalStatus::Completed));
    assert_eq!(retained.output.unwrap().response, "still retained");
    let (unknown, lost) = pending.resolve("overflow");
    assert!(lost);
    assert!(unknown.completion.is_none());
    assert!(unknown.output.is_none());
}

#[test]
fn output_budget_is_aggregate_utf8_safe_and_replacement_reclaims_bytes() {
    let mut pending = PendingTurnObservations::default();
    pending.observe_output("first", &"x".repeat(MAX_TURN_RESPONSE_BYTES - 1));
    pending.observe_output("second", "é");
    assert_eq!(pending.response_bytes, MAX_TURN_RESPONSE_BYTES - 1);
    let second = pending.entries["second"].output.as_ref().unwrap();
    assert!(second.truncated);
    assert_eq!(second.response, "");
    pending.observe_output("first", "small");
    pending.observe_output("second", "é");
    assert_eq!(pending.response_bytes, 7);
    assert!(!pending.entries["second"].output.as_ref().unwrap().truncated);
    pending.observe_completion("second", TurnTerminalStatus::Failed);
    assert_eq!(
        pending.completion("second"),
        Some(TurnTerminalStatus::Failed)
    );
}

#[test]
fn conflicting_completions_remain_unknown_but_identical_repeats_are_safe() {
    let mut pending = PendingTurnObservations::default();
    pending.observe_completion("own", TurnTerminalStatus::Completed);
    pending.observe_completion("own", TurnTerminalStatus::Completed);
    assert_eq!(
        pending.completion("own"),
        Some(TurnTerminalStatus::Completed)
    );
    pending.observe_completion("own", TurnTerminalStatus::Failed);
    pending.observe_completion("own", TurnTerminalStatus::Completed);
    let (own, lost) = pending.resolve("own");
    assert!(lost);
    assert!(own.completion.is_none());
}

#[test]
fn oversize_id_never_expands_the_candidate_budget_or_implies_running() {
    let mut pending = PendingTurnObservations::default();
    let id = "x".repeat(MAX_PENDING_TURN_ID_BYTES + 1);
    pending.observe_completion(&id, TurnTerminalStatus::Completed);
    assert!(pending.entries.is_empty());
    assert!(pending.resolve(&id).1);
    assert!(
        !PendingTurnObservations::default()
            .resolve("not-yet-observed")
            .1
    );
}
