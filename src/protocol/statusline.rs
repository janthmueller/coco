use super::*;

/// Repository identity is resolved once by the host-owned MCP scope, not
/// supplied by the native footer's model/client metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspaceStatuslineParams {
    pub repository_id: String,
    pub thread_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspaceStatuslineSnapshot {
    pub workspace_id: String,
    pub name: String,
    pub resources: Option<WorkspaceRuntimeResources>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspaceStatuslineResult {
    pub workspace: Option<WorkspaceStatuslineSnapshot>,
}

impl DaemonRequest for WorkspaceStatuslineParams {
    type Response = WorkspaceStatuslineResult;
    const METHOD: DaemonMethod = DaemonMethod::WorkspaceStatusline;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn footer_query_has_no_mutation_or_history_inputs() {
        let input = json!({"repositoryId":"repo", "threadId":"thread"});
        let params: WorkspaceStatuslineParams = serde_json::from_value(input.clone()).unwrap();
        assert_eq!(serde_json::to_value(params).unwrap(), input);
        assert_eq!(
            WorkspaceStatuslineParams::METHOD.as_str(),
            "workspace.statusline"
        );
        for key in ["workspace", "repositoryPath", "includeTurns", "start"] {
            let mut invalid = input.clone();
            invalid[key] = json!(true);
            assert!(serde_json::from_value::<WorkspaceStatuslineParams>(invalid).is_err());
        }
    }
}
