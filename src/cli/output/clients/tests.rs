use super::*;
use crate::domain::clients::{ClientIntegration, ClientMetadata};

fn client(kind: &str, integration: Option<ClientIntegration>) -> WorkspaceClient {
    WorkspaceClient {
        id: "presence-1".to_owned(),
        metadata: ClientMetadata {
            kind: kind.to_owned(),
            integration,
        },
    }
}

#[test]
fn labels_are_short_sorted_and_count_duplicate_locations() {
    assert_eq!(client_label(&[]), "—");
    assert_eq!(client_label(&[client("native_tui", None)]), "TUI");
    assert_eq!(client_label(&[client("unknown", None)]), "Client");
    let tmux = client(
        "native_tui",
        Some(ClientIntegration {
            kind: "tmux".to_owned(),
            scope: "server-1".to_owned(),
            locator: "%7".to_owned(),
            label: Some("dev:2.1".to_owned()),
        }),
    );
    assert_eq!(
        client_label(&[tmux.clone(), client("native_tui", None), tmux]),
        "TUI, tmux dev:2.1 ×2"
    );
}

#[test]
fn missing_labels_use_locators_and_untrusted_text_is_sanitized() {
    let tmux = client(
        "native_tui",
        Some(ClientIntegration {
            kind: "tmux".to_owned(),
            scope: "server-1".to_owned(),
            locator: "%7\n\x1b[31m".to_owned(),
            label: None,
        }),
    );
    assert_eq!(client_label(&[tmux]), "tmux %7  [31m");
}
