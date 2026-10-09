//! Ephemeral client presentation. These identifiers carry no attach authority.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ClientMetadata {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integration: Option<ClientIntegration>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ClientIntegration {
    pub kind: String,
    /// Opaque integration scope, such as one tmux server instance.
    pub scope: String,
    pub locator: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspaceClient {
    /// Public presence ID, deliberately different from the renewable lease ID.
    pub id: String,
    pub metadata: ClientMetadata,
}

impl Default for ClientMetadata {
    fn default() -> Self {
        Self {
            kind: "unknown".to_owned(),
            integration: None,
        }
    }
}

impl ClientMetadata {
    pub(crate) fn is_valid(&self) -> bool {
        valid_kind(&self.kind)
            && self.integration.as_ref().is_none_or(|integration| {
                valid_kind(&integration.kind)
                    && valid_text(&integration.scope, 128)
                    && valid_text(&integration.locator, 128)
                    && integration
                        .label
                        .as_ref()
                        .is_none_or(|label| valid_text(label, 96))
            })
    }
}

fn valid_kind(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn valid_text(value: &str, limit: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= limit
        && !value.chars().any(|character| {
            character.is_control()
                || matches!(character, '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_groups_stay_generic_but_supplied_identifiers_are_bounded() {
        assert!(ClientMetadata::default().is_valid());
        for kind in ["", "bad kind", "native\ntui", "TUI", &"x".repeat(33)] {
            assert!(
                !ClientMetadata {
                    kind: kind.to_owned(),
                    integration: None
                }
                .is_valid()
            );
        }
        let mut metadata = ClientMetadata {
            kind: "native_tui".to_owned(),
            integration: Some(ClientIntegration {
                kind: "tmux".to_owned(),
                scope: "server".to_owned(),
                locator: "%7".to_owned(),
                label: None,
            }),
        };
        assert!(metadata.is_valid());
        metadata.integration.as_mut().unwrap().scope = "x".repeat(129);
        assert!(!metadata.is_valid());
        metadata.integration.as_mut().unwrap().scope = "server".to_owned();
        metadata.integration.as_mut().unwrap().locator = "  ".to_owned();
        assert!(!metadata.is_valid());
    }

    #[test]
    fn labels_reject_controls_bidi_and_byte_overflow() {
        for label in ["", " ", "\n", "\x1b[31m", "a\u{202e}b", &"é".repeat(49)] {
            assert!(!valid_text(label, 96));
        }
        assert!(valid_text(&"é".repeat(48), 96));
        assert!(valid_text("dev:2.1", 96));
    }
}
