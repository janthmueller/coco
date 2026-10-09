use std::collections::BTreeMap;

use crate::domain::clients::WorkspaceClient;

pub(super) fn client_label(clients: &[WorkspaceClient]) -> String {
    let mut labels = BTreeMap::new();
    for client in clients {
        let metadata = &client.metadata;
        let label = match metadata.integration.as_ref() {
            Some(integration) => format!(
                "{} {}",
                integration.kind,
                integration.label.as_deref().unwrap_or(&integration.locator),
            ),
            None => match metadata.kind.as_str() {
                "native_tui" => "TUI".to_owned(),
                "unknown" => "Client".to_owned(),
                kind => kind.to_owned(),
            },
        };
        *labels.entry(super::safe_line(&label)).or_insert(0_usize) += 1;
    }
    if labels.is_empty() {
        return "—".to_owned();
    }
    labels
        .into_iter()
        .map(|(label, count)| {
            if count == 1 {
                label
            } else {
                format!("{label} ×{count}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests;
