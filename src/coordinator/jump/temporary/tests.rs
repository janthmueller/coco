use super::*;

#[test]
fn cleanup_failure_reports_partial_recovery_without_claiming_resource_retention() {
    for state in [
        WorkspaceAvailability::Closing,
        WorkspaceAvailability::Closed,
        WorkspaceAvailability::Reopening,
        WorkspaceAvailability::Deleting,
    ] {
        assert_eq!(
            cleanup_failure(
                CoordinatorError::InvalidParams("fixture failure".into()),
                Some(state)
            ),
            WorkspaceCleanupResult::Incomplete
        );
    }
    assert_eq!(
        cleanup_failure(
            CoordinatorError::InvalidParams("fixture failure".into()),
            None
        ),
        WorkspaceCleanupResult::Deleted
    );
    assert!(
        matches!(cleanup_failure(CoordinatorError::WorkspaceRetirementBlocked(vec!["local changes".into()]), Some(WorkspaceAvailability::Open)), WorkspaceCleanupResult::Retained { reason } if reason == "local changes")
    );
}

#[test]
fn only_a_live_matching_session_can_complete_and_it_cannot_be_replayed() {
    let mut registry = JumpLeaseRegistry::default();
    let lease = registry.acquire("workspace", false, None).unwrap();
    assert!(registry.finish_session("foreign", &lease).is_err());
    assert!(registry.finish_session("workspace", "unknown").is_err());
    assert_eq!(registry.clients("workspace").len(), 1);
    assert!(registry.finish_session("workspace", &lease).unwrap());
    assert!(registry.finish_session("workspace", &lease).is_err());
}

#[test]
fn another_session_and_in_flight_adoption_prevent_cleanup_permission() {
    let mut registry = JumpLeaseRegistry::default();
    let first = registry.acquire("workspace", false, None).unwrap();
    let last = registry.acquire("workspace", false, None).unwrap();
    assert!(!registry.finish_session("workspace", &first).unwrap());
    assert!(registry.finish_session("workspace", &last).unwrap());
    let adopting = registry.acquire("pending", true, None).unwrap();
    registry
        .begin_adoption("pending", &adopting, "candidate")
        .unwrap();
    assert!(!registry.finish_session("pending", &adopting).unwrap());
    registry
        .finish_adoption("pending", &adopting, true)
        .unwrap();
    assert!(registry.finish_session("pending", &adopting).is_err());
}

#[test]
fn expiration_and_generation_clear_cannot_complete_sessions() {
    for expired in [false, true] {
        let mut registry = JumpLeaseRegistry::default();
        let lease = registry.acquire("workspace", false, None).unwrap();
        if expired {
            registry.leases.get_mut(&lease).unwrap().expires_at =
                Instant::now() - Duration::from_secs(1);
        } else {
            registry.clear();
        }
        assert!(registry.finish_session("workspace", &lease).is_err());
    }
}

#[test]
fn disarm_authority_excludes_expired_generations_and_released_pinned_adoptions() {
    for expired in [false, true] {
        let mut registry = JumpLeaseRegistry::default();
        let lease = registry.acquire("workspace", false, None).unwrap();
        if expired {
            registry.leases.get_mut(&lease).unwrap().expires_at =
                Instant::now() - Duration::from_secs(1);
        } else {
            registry.clear();
        }
        assert!(registry.live_session("workspace", &lease).is_err());
    }
    let mut registry = JumpLeaseRegistry::default();
    let lease = registry.acquire("workspace", true, None).unwrap();
    registry
        .begin_adoption("workspace", &lease, "candidate")
        .unwrap();
    registry.release("workspace", &lease).unwrap();
    assert!(registry.live_session("workspace", &lease).is_err());
}
