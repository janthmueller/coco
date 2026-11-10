use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value, json};

use super::{CodexWorker, WorkerError};
use crate::domain::control::{InheritedMcpServer, McpTransportKind};
use crate::protocol::CONTROL_CODEX_VERSION;

const MAX_CONTROL_MCP_SERVERS: usize = 256;
const MAX_CONTROL_CONFIG_LAYERS: usize = 256;

impl CodexWorker {
    pub(super) async fn read_control_mcp_servers(
        &self,
        cwd: &Path,
    ) -> Result<Vec<InheritedMcpServer>, WorkerError> {
        if self.client.server_version() != Some(CONTROL_CODEX_VERSION) {
            return Err(WorkerError::InvalidResponse(
                "verified control session Codex version",
            ));
        }
        let response = self
            .client
            .request(
                "config/read",
                json!({
                    "cwd": cwd, "includeLayers": true,
                }),
            )
            .await
            .map_err(|_| WorkerError::InvalidResponse("control session configuration"))?;
        inherited_servers(&response)
    }
}

fn inherited_servers(response: &Value) -> Result<Vec<InheritedMcpServer>, WorkerError> {
    let invalid = || WorkerError::InvalidResponse("control session configuration layers");
    let config = response
        .get("config")
        .and_then(Value::as_object)
        .ok_or_else(invalid)?;
    let layers = response
        .get("layers")
        .and_then(Value::as_array)
        .filter(|layers| layers.len() <= MAX_CONTROL_CONFIG_LAYERS)
        .ok_or_else(invalid)?;
    let mut servers = BTreeMap::new();
    merge_servers(config.get("mcp_servers"), &mut servers)?;
    // Effective config omits untrusted project layers. Native folder trust
    // can activate them after preparation, so disable their names as well.
    // Only names/transport kinds leave here, never layer recipes or diagnostics.
    for layer in layers {
        let config = layer
            .get("config")
            .and_then(Value::as_object)
            .ok_or_else(invalid)?;
        merge_servers(config.get("mcp_servers"), &mut servers)?;
    }
    servers
        .into_iter()
        .map(|(name, transport)| {
            Ok(InheritedMcpServer {
                name,
                transport: transport.ok_or_else(invalid)?,
            })
        })
        .collect()
}

fn merge_servers(
    servers: Option<&Value>,
    output: &mut BTreeMap<String, Option<McpTransportKind>>,
) -> Result<(), WorkerError> {
    let invalid = || WorkerError::InvalidResponse("control MCP server metadata");
    let Some(servers) = servers.filter(|value| !value.is_null()) else {
        return Ok(());
    };
    let servers = servers.as_object().ok_or_else(invalid)?;
    if servers.len() > MAX_CONTROL_MCP_SERVERS {
        return Err(invalid());
    }
    for (name, server) in servers {
        if name.is_empty() || name.len() > 256 || name.chars().any(char::is_control) {
            return Err(invalid());
        }
        let server = server.as_object().ok_or_else(invalid)?;
        let present = |value: &Value| value.as_str().is_some_and(|value| !value.is_empty());
        let transport = match (server.get("command"), server.get("url")) {
            (Some(command), None) if present(command) => Some(McpTransportKind::Stdio),
            (None, Some(url)) if present(url) => Some(McpTransportKind::StreamableHttp),
            (None, None) => None, // A partial layer can inherit its transport.
            _ => return Err(invalid()),
        };
        let existing = output.entry(name.clone()).or_default();
        if existing.is_some() && transport.is_some() && *existing != transport {
            return Err(invalid());
        }
        *existing = existing.or(transport);
        if output.len() > MAX_CONTROL_MCP_SERVERS {
            return Err(invalid());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_only_are_returned_and_malformed_or_unbounded_data_fails_closed() {
        let mut names = BTreeMap::new();
        merge_servers(None, &mut names).unwrap();
        merge_servers(Some(&json!(null)), &mut names).unwrap();
        assert!(names.is_empty());
        merge_servers(Some(&json!({"z": {"command":"native-z", "token": "private"}, "a": {"url":"https://example.invalid"}})), &mut names).unwrap();
        assert_eq!(
            names.into_iter().collect::<Vec<_>>(),
            [
                ("a".into(), Some(McpTransportKind::StreamableHttp)),
                ("z".into(), Some(McpTransportKind::Stdio))
            ]
        );
        for invalid in [
            json!([]),
            json!(false),
            json!({"bad\nname": {}}),
            json!({"": {}}),
        ] {
            assert!(merge_servers(Some(&invalid), &mut BTreeMap::new()).is_err());
        }
    }

    #[test]
    fn disabled_project_layers_are_included_without_recipes_or_duplicate_names() {
        let response = json!({"config":{"mcp_servers":{"active":{"command":"native"}}}, "layers":[
            {"config":{"mcp_servers":{"active":{},"z":{"url":"https://example.invalid"}}}},
            {"disabledReason":"untrusted project", "config":{"mcp_servers":{
                "project.server\"x":{"command":"private-command", "env":{"TOKEN":"synthetic-private-recipe"}}
            }}},
            {"config":{}}
        ]});
        assert_eq!(
            inherited_servers(&response).unwrap(),
            [
                InheritedMcpServer {
                    name: "active".into(),
                    transport: McpTransportKind::Stdio
                },
                InheritedMcpServer {
                    name: "project.server\"x".into(),
                    transport: McpTransportKind::Stdio
                },
                InheritedMcpServer {
                    name: "z".into(),
                    transport: McpTransportKind::StreamableHttp
                }
            ]
        );
        assert!(
            inherited_servers(&json!({"config":{},"layers":[]}))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn missing_or_malformed_layer_evidence_fails_closed_without_diagnostics() {
        for invalid in [
            json!({"config":{}}),
            json!({"config":{},"layers":null}),
            json!({"config":{},"layers":{}}),
            json!({"config":null,"layers":[]}),
            json!({"config":{},"layers":[{}]}),
            json!({"config":{},"layers":[{"config":null}]}),
            json!({"config":{},"layers":[{"config":{"mcp_servers":[]}}]}),
            json!({"config":{},"layers":[{"config":{"mcp_servers":{"unknown":{}}}}]}),
            json!({"config":{},"layers":[{"config":{"mcp_servers":{"bad":{"command":false}}}}]}),
            json!({"config":{"mcp_servers":{"conflict":{"command":"native"}}},"layers":[
                {"config":{"mcp_servers":{"conflict":{"url":"https://example.invalid"}}}}
            ]}),
            json!({"config":{},"layers":[{"disabledReason":"untrusted", "config":{
                "mcp_servers":{"bad\nname":{"token":"synthetic-private-recipe"}}
            }}]}),
        ] {
            let error = inherited_servers(&invalid).unwrap_err();
            assert!(!format!("{error} {error:?}").contains("synthetic-private-recipe"));
        }
    }

    #[test]
    fn aggregate_name_and_layer_limits_apply_even_to_disabled_configuration() {
        let servers = (0..MAX_CONTROL_MCP_SERVERS)
            .map(|index| (format!("server-{index}"), json!({"command":"native"})))
            .collect::<serde_json::Map<_, _>>();
        let mut response = json!({"config":{"mcp_servers":servers}, "layers":[
            {"disabledReason":"untrusted", "config":{"mcp_servers":servers}}
        ]});
        assert_eq!(
            inherited_servers(&response).unwrap().len(),
            MAX_CONTROL_MCP_SERVERS
        );
        response["layers"][0]["config"]["mcp_servers"] = json!({"one-more":{"command":"native"}});
        assert!(inherited_servers(&response).is_err());
        response["layers"] = json!(vec![json!({"config":{}}); MAX_CONTROL_CONFIG_LAYERS + 1]);
        assert!(inherited_servers(&response).is_err());
    }
}
