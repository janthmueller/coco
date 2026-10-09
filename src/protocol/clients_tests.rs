use super::*;
use crate::domain::clients::ClientIntegration;
use serde_json::json;

#[test]
fn old_attach_and_status_requests_remain_valid_and_omit_new_fields() {
    let attach: WorkspaceAttachParams = serde_json::from_value(json!({
        "scope": {"kind": "allRepositories"}, "workspace": "fix/login"
    }))
    .unwrap();
    assert_eq!(attach.client, None);
    assert!(
        serde_json::to_value(attach)
            .unwrap()
            .get("client")
            .is_none()
    );
    let get: WorkspaceGetParams = serde_json::from_value(json!({
        "scope": {"kind": "allRepositories"}, "workspace": "fix/login"
    }))
    .unwrap();
    assert!(!get.include_clients);
    assert!(
        serde_json::to_value(get)
            .unwrap()
            .get("includeClients")
            .is_none()
    );
}

#[test]
fn generic_metadata_and_projection_requests_round_trip() {
    let params = WorkspaceAttachParams {
        scope: RepositoryScope::repository("/repo"),
        workspace: "fix/login".to_owned(),
        client: Some(ClientMetadata {
            kind: "native_tui".to_owned(),
            integration: Some(ClientIntegration {
                kind: "tmux".to_owned(),
                scope: "opaque-server".to_owned(),
                locator: "%7".to_owned(),
                label: Some("dev:2.1".to_owned()),
            }),
        }),
    };
    let value = serde_json::to_value(&params).unwrap();
    assert_eq!(value["client"]["integration"]["locator"], "%7");
    assert_eq!(
        serde_json::from_value::<WorkspaceAttachParams>(value).unwrap(),
        params
    );
    let get = WorkspaceGetParams {
        scope: RepositoryScope::AllRepositories,
        workspace: "fix/login".to_owned(),
        include_resources: false,
        include_clients: true,
    };
    assert_eq!(serde_json::to_value(get).unwrap()["includeClients"], true);
    let list = WorkspaceListParams {
        scope: RepositoryScope::AllRepositories,
        phases: None,
        include_resources: false,
        include_activity: false,
        include_clients: true,
    };
    assert_eq!(serde_json::to_value(list).unwrap()["includeClients"], true);
}
