use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspacePathGetParams {
    pub scope: RepositoryScope,
    pub workspace: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspacePathListParams {
    pub scope: RepositoryScope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkspacePathEntry {
    pub workspace_id: String,
    pub name: String,
    pub repository: RepositorySummary,
    pub path: PathBuf,
    pub branch_name: Option<String>,
    pub worktree_mode: crate::domain::WorktreeMode,
}

impl DaemonRequest for WorkspacePathGetParams {
    type Response = PathBuf;
    const METHOD: DaemonMethod = DaemonMethod::WorkspacePathGet;
}

impl DaemonRequest for WorkspacePathListParams {
    type Response = Vec<WorkspacePathEntry>;
    const METHOD: DaemonMethod = DaemonMethod::WorkspacePathList;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn navigation_contract_has_only_scope_identity_and_saved_path_metadata() {
        let params: WorkspacePathGetParams = serde_json::from_value(json!({
            "scope": {"kind": "repository", "path": "/repo"},
            "workspace": "fix/login",
        }))
        .unwrap();
        assert_eq!(
            WorkspacePathGetParams::METHOD.as_str(),
            "workspace.path.get"
        );
        assert_eq!(params.workspace, "fix/login");
        assert!(
            serde_json::from_value::<WorkspacePathGetParams>(json!({
                "scope": {"kind": "allRepositories"}, "workspace": "id", "includeHistory": true,
            }))
            .is_err()
        );
        let params: WorkspacePathListParams = serde_json::from_value(json!({
            "scope": {"kind": "allRepositories"},
        }))
        .unwrap();
        assert_eq!(
            serde_json::to_value(params).unwrap(),
            json!({"scope": {"kind": "allRepositories"}})
        );
        assert_eq!(
            WorkspacePathListParams::METHOD.as_str(),
            "workspace.path.list"
        );
        let path: <WorkspacePathGetParams as DaemonRequest>::Response =
            serde_json::from_value(json!("/worktrees/fix/login")).unwrap();
        assert_eq!(path, PathBuf::from("/worktrees/fix/login"));
    }
}
