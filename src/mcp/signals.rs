use super::*;
use crate::protocol::{
    SignalCatalogLoadParams, SignalEmitParams, SignalListParams, SignalTypeListParams,
};
use rmcp::{RoleServer, service::RequestContext};

pub(super) fn validate_grants(grants: &[String]) -> anyhow::Result<()> {
    anyhow::ensure!(
        grants.len() <= 128,
        "at most 128 signal version grants are supported"
    );
    anyhow::ensure!(
        grants.iter().all(|grant| parse_grant(grant).is_some()),
        "--allow-emit requires NAME or NAME@VERSION (a bare name grants only version 1)"
    );
    Ok(())
}

fn parse_grant(grant: &str) -> Option<(&str, u32)> {
    let (name, version) = match grant.split_once('@') {
        Some((name, version)) => (name, version.parse::<u32>().ok()?),
        None => (grant, 1),
    };
    (crate::domain::signals::valid_signal_name(name) && version > 0).then_some((name, version))
}
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SignalEmitInput {
    /// Registered and explicitly granted signal name.
    name: String,
    /// Exact registered immutable schema version, starting at 1.
    version: u32,
    /// JSON claim. Shape validation does not verify its truth.
    payload: Value,
    /// Reuse for retries of this emission; choose a new key for a new emission.
    idempotency_key: String,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SignalReadInput {
    /// Optional exact workspace UUID, including a previously deleted workspace.
    workspace_id: Option<String>,
    /// Optional exact registered signal name.
    name: Option<String>,
    /// Opaque nextCursor from a previous page with identical filters.
    after: Option<String>,
    /// Page size between 1 and 100 (default 100).
    limit: Option<u32>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct SignalTypesInput {}

impl McpServer {
    pub(super) async fn load_signal_catalog(&mut self, directory: PathBuf) -> anyhow::Result<()> {
        let definitions = self
            .dispatcher
            .client
            .request(SignalCatalogLoadParams {
                repository: self.dispatcher.repository.clone(),
                directory: std::path::absolute(directory)
                    .context("could not resolve signal catalog path")?,
            })
            .await?;
        for grant in &self.allowed_signals {
            anyhow::ensure!(
                definitions
                    .iter()
                    .any(|definition| parse_grant(grant)
                        == Some((&definition.name, definition.version))),
                "--allow-emit {grant} has no matching schema in --signal-catalog"
            );
        }
        self.signal_catalog = Some(Arc::new(definitions));
        Ok(())
    }
}

#[tool_router(router = signal_tool_router, vis = "pub(super)")]
impl McpServer {
    fn signal_granted(&self, name: &str, version: u32) -> bool {
        self.signal_catalog.as_ref().is_some_and(|definitions| {
            definitions
                .iter()
                .any(|definition| definition.name == name && definition.version == version)
        }) && self
            .allowed_signals
            .iter()
            .any(|grant| parse_grant(grant) == Some((name, version)))
    }

    #[tool(
        name = "signals.types",
        description = "Discover selected signal definitions and payload schemas. emitAllowed says whether this server grants that exact version. Without a selected catalog, inspect previously loaded versions read-only.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn signals_types(
        &self,
        Parameters(_input): Parameters<SignalTypesInput>,
    ) -> CallToolResult {
        let definitions = match &self.signal_catalog {
            Some(definitions) => Ok(definitions.as_ref().clone()),
            None => self
                .dispatcher
                .client
                .request(SignalTypeListParams {
                    repository: self.dispatcher.repository.clone(),
                })
                .await
                .map_err(DaemonFailure::from),
        };
        let response = definitions.map(|definitions| {
            Value::Array(
                definitions
                    .into_iter()
                    .map(|definition| {
                        let allowed = self.signal_catalog.is_some()
                            && self.signal_granted(&definition.name, definition.version);
                        let mut value = json!(definition);
                        value["emitAllowed"] = json!(allowed);
                        value
                    })
                    .collect(),
            )
        });
        self.dispatcher
            .audit("signals.types", None, None, &response)
            .await;
        match response {
            Ok(value) => CallToolResult::structured(value),
            Err(error) => CallToolResult::structured_error(error.as_json()),
        }
    }

    #[tool(
        name = "signals.list",
        description = "Read a bounded page of retained signals. Save nextCursor to resume independently; no messages are consumed and no agents are awakened. Expired or differently scoped cursors fail explicitly.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn signals_list(&self, Parameters(input): Parameters<SignalReadInput>) -> CallToolResult {
        self.dispatcher
            .call(
                "signals.list",
                SignalListParams {
                    scope: RepositoryScope::repository(self.dispatcher.repository.clone()),
                    workspace_id: input.workspace_id,
                    name: input.name,
                    after: input.after,
                    limit: input.limit.unwrap_or(100),
                },
                None,
                None,
            )
            .await
    }

    #[tool(
        name = "signals.emit",
        description = "Persist a claim under an explicitly granted signal name and registered version. Requires native Codex thread metadata bound to this repository. Reuse idempotencyKey only for identical retries while the record is retained. Does not authorize actions, change status, or wake another agent.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn signals_emit(
        &self,
        Parameters(input): Parameters<SignalEmitInput>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        if !self.signal_granted(&input.name, input.version) {
            return signal_error(
                "SIGNAL_NOT_GRANTED",
                "operator has not granted this signal name/version",
            );
        }
        let Some(thread_id) = context
            .meta
            .get("threadId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty() && id.len() <= 256)
        else {
            return signal_error(
                "SIGNAL_SENDER_INVALID",
                "signals.emit requires native Codex threadId request metadata",
            );
        };
        self.dispatcher
            .call(
                "signals.emit",
                SignalEmitParams {
                    repository: self.dispatcher.repository.clone(),
                    thread_id: thread_id.to_owned(),
                    name: input.name,
                    version: input.version,
                    payload: input.payload,
                    idempotency_key: input.idempotency_key.clone(),
                },
                None,
                Some(input.idempotency_key),
            )
            .await
    }
}

fn signal_error(code: &str, message: &str) -> CallToolResult {
    CallToolResult::structured_error(DaemonFailure::new(code, message).as_json())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grants_pin_versions_and_never_expand_with_the_catalog() {
        let mut server = McpServer::new(
            "/repo".into(),
            false,
            "/tmp/cocod.sock".into(),
            vec!["review.requested@2".into(), "tests.finished".into()],
        );
        assert!(
            !server.signal_granted("review.requested", 2),
            "grants alone must not enable unselected definitions"
        );
        server.signal_catalog = Some(Arc::new(
            [
                ("review.requested", 1),
                ("review.requested", 2),
                ("review.requested", 3),
                ("tests.finished", 1),
                ("tests.finished", 2),
            ]
            .into_iter()
            .map(|(name, version)| SignalType {
                repository_id: "repo".into(),
                name: name.into(),
                version,
                description: name.into(),
                payload_schema: Some(json!(true)),
                registered_at_ms: 0,
            })
            .collect(),
        ));
        assert!(server.signal_granted("review.requested", 2));
        assert!(!server.signal_granted("review.requested", 1));
        assert!(!server.signal_granted("review.requested", 3));
        assert!(server.signal_granted("tests.finished", 1));
        assert!(!server.signal_granted("tests.finished", 2));
        for grant in [
            "",
            "review.requested@0",
            "review.requested@latest",
            "has space",
            "a@1@2",
        ] {
            assert!(validate_grants(&[grant.into()]).is_err());
        }
    }
}
