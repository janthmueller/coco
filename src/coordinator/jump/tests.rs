use super::*;
use crate::domain::clients::ClientIntegration;

fn metadata() -> ClientMetadata {
    ClientMetadata {
        kind: "native_tui".to_owned(),
        integration: Some(ClientIntegration {
            kind: "tmux".to_owned(),
            scope: "opaque-server".to_owned(),
            locator: "%7".to_owned(),
            label: Some("dev:2.1".to_owned()),
        }),
    }
}

#[test]
fn clients_are_independent_and_never_expose_lease_capabilities() {
    let mut registry = JumpLeaseRegistry::default();
    let first = registry
        .acquire("workspace", false, Some(metadata()))
        .unwrap();
    let second = registry.acquire("workspace", false, None).unwrap();
    registry.acquire("other", false, None).unwrap();
    let clients = registry.clients("workspace");
    assert_eq!(clients.len(), 2);
    assert_ne!(clients[0].id, clients[1].id);
    let encoded = serde_json::to_string(&clients).unwrap();
    assert!(!encoded.contains(&first));
    assert!(!encoded.contains(&second));
    for client in &clients {
        assert!(registry.renew("workspace", &client.id).is_err());
    }
    registry.renew("workspace", &first).unwrap();
    assert_eq!(clients, registry.clients("workspace"));
    registry.release("workspace", &first).unwrap();
    assert_eq!(registry.clients("workspace").len(), 1);
    assert_eq!(registry.clients("other").len(), 1);
    registry.release("workspace", &second).unwrap();
    assert!(registry.clients("workspace").is_empty());
}

#[test]
fn expiry_removes_only_stale_clients_and_status_does_not_renew_them() {
    let mut registry = JumpLeaseRegistry::default();
    let stale = registry.acquire("workspace", false, None).unwrap();
    let live = registry
        .acquire("workspace", false, Some(metadata()))
        .unwrap();
    let expiry = registry.leases[&live].expires_at;
    let client_expiry = registry.leases[&live].client_expires_at;
    let live_id = registry.leases[&live].client.id.clone();
    expire_lease(&mut registry, &stale);
    let clients = registry.clients("workspace");
    assert_eq!(clients.len(), 1);
    assert_eq!(clients[0].id, live_id);
    assert_eq!(registry.leases[&live].expires_at, expiry);
    assert_eq!(registry.leases[&live].client_expires_at, client_expiry);
    assert!(registry.renew("workspace", &stale).is_err());
}

#[test]
fn pending_adoption_pins_authority_but_not_expired_or_released_presence() {
    let mut registry = JumpLeaseRegistry::default();
    let lease = registry
        .acquire("workspace", true, Some(metadata()))
        .unwrap();
    registry
        .begin_adoption("workspace", &lease, "thread")
        .unwrap();
    let client = registry.clients("workspace")[0].clone();
    expire_lease(&mut registry, &lease);
    assert!(registry.clients("workspace").is_empty());
    assert!(registry.leases.contains_key(&lease));
    registry.renew("workspace", &lease).unwrap();
    assert_eq!(registry.clients("workspace"), [client]);
    registry.release("workspace", &lease).unwrap();
    assert!(registry.clients("workspace").is_empty());
    assert!(registry.leases.contains_key(&lease));
    registry.finish_adoption("workspace", &lease, true).unwrap();
    assert!(!registry.leases.contains_key(&lease));
}

#[test]
fn adoption_reconciliation_preserves_identity_and_generation_clear_removes_presence() {
    let mut registry = JumpLeaseRegistry::default();
    let lease = registry
        .acquire("workspace", true, Some(metadata()))
        .unwrap();
    let before = registry.clients("workspace");
    registry
        .begin_adoption("workspace", &lease, "thread")
        .unwrap();
    registry.finish_adoption("workspace", &lease, true).unwrap();
    registry.reconcile_bound("workspace", &lease).unwrap();
    assert_eq!(registry.clients("workspace"), before);
    registry.clear();
    assert!(registry.clients("workspace").is_empty());
    assert!(registry.renew("workspace", &lease).is_err());
}

fn expire_lease(registry: &mut JumpLeaseRegistry, lease_id: &str) {
    let lease = registry.leases.get_mut(lease_id).unwrap();
    let expired = Instant::now() - Duration::from_secs(1);
    lease.expires_at = expired;
    lease.client_expires_at = expired;
}

#[test]
fn adoption_completion_cannot_revive_expired_client_presence() {
    for bound in [true, false] {
        let mut registry = JumpLeaseRegistry::default();
        let lease = registry
            .acquire("workspace", true, Some(metadata()))
            .unwrap();
        let client = registry.clients("workspace")[0].clone();
        registry
            .begin_adoption("workspace", &lease, "thread")
            .unwrap();
        expire_lease(&mut registry, &lease);
        assert!(registry.clients("workspace").is_empty());
        assert!(registry.reject_active("workspace").is_err());

        registry
            .finish_adoption("workspace", &lease, bound)
            .unwrap();
        assert!(
            registry.clients("workspace").is_empty(),
            "adoption completion must not revive presence (bound={bound})"
        );
        assert_eq!(registry.leases[&lease].pending_adoption, !bound);
        assert!(!registry.leases[&lease].adoption_in_flight);
        assert!(registry.leases[&lease].expires_at > Instant::now());

        registry.renew("workspace", &lease).unwrap();
        assert_eq!(registry.clients("workspace"), [client]);
        registry.release("workspace", &lease).unwrap();
        assert!(registry.clients("workspace").is_empty());
    }
}

#[test]
fn reconciliation_cannot_revive_expired_client_or_hide_another_client() {
    let mut registry = JumpLeaseRegistry::default();
    let stale = registry
        .acquire("workspace", true, Some(metadata()))
        .unwrap();
    let stale_client = registry.clients("workspace")[0].clone();
    registry
        .begin_adoption("workspace", &stale, "thread")
        .unwrap();
    expire_lease(&mut registry, &stale);
    assert!(registry.clients("workspace").is_empty());

    registry.reconcile_bound("workspace", &stale).unwrap();
    assert!(registry.clients("workspace").is_empty());
    let live = registry.acquire("workspace", false, None).unwrap();
    let live_client = registry.leases[&live].client.clone();
    assert_eq!(
        registry.clients("workspace").as_slice(),
        std::slice::from_ref(&live_client)
    );
    // Retries after durable binding must not masquerade as client heartbeats.
    registry.reconcile_bound("workspace", &stale).unwrap();
    registry.reconcile_bound("workspace", &stale).unwrap();
    assert_eq!(
        registry.clients("workspace").as_slice(),
        std::slice::from_ref(&live_client)
    );

    registry.renew("workspace", &stale).unwrap();
    let clients = registry.clients("workspace");
    assert_eq!(clients.len(), 2);
    assert!(clients.contains(&stale_client) && clients.contains(&live_client));
    registry.release("workspace", &stale).unwrap();
    assert_eq!(registry.clients("workspace"), [live_client]);
    registry.release("workspace", &live).unwrap();
    assert!(registry.clients("workspace").is_empty());
}

#[test]
fn adoption_preserves_the_client_deadline_until_an_explicit_heartbeat() {
    for bound in [true, false] {
        let mut registry = JumpLeaseRegistry::default();
        let lease = registry.acquire("workspace", true, None).unwrap();
        let client_deadline = Instant::now() + Duration::from_secs(1);
        registry.leases.get_mut(&lease).unwrap().client_expires_at = client_deadline;

        registry
            .begin_adoption("workspace", &lease, "thread")
            .unwrap();
        assert_eq!(registry.leases[&lease].client_expires_at, client_deadline);
        registry
            .finish_adoption("workspace", &lease, bound)
            .unwrap();
        assert_eq!(registry.leases[&lease].client_expires_at, client_deadline);
        registry.reconcile_bound("workspace", &lease).unwrap();
        assert_eq!(registry.leases[&lease].client_expires_at, client_deadline);

        registry.renew("workspace", &lease).unwrap();
        let renewed = &registry.leases[&lease];
        assert!(renewed.client_expires_at > client_deadline);
        assert_eq!(renewed.client_expires_at, renewed.expires_at);
    }
}

#[test]
fn generation_clear_hides_pinned_presence_even_after_a_late_heartbeat() {
    let mut registry = JumpLeaseRegistry::default();
    let lease = registry.acquire("workspace", true, None).unwrap();
    registry
        .begin_adoption("workspace", &lease, "thread")
        .unwrap();
    registry.clear();
    assert!(registry.leases.contains_key(&lease));
    assert!(registry.clients("workspace").is_empty());

    registry.renew("workspace", &lease).unwrap();
    assert!(registry.clients("workspace").is_empty());
    registry.reconcile_bound("workspace", &lease).unwrap();
    assert!(registry.clients("workspace").is_empty());
    assert!(!registry.leases.contains_key(&lease));
    assert!(registry.renew("workspace", &lease).is_err());
}
