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
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub include_recency: bool,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recency_at_ms: Option<i64>,
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
    fn navigation_lookup_is_metadata_only_and_list_defaults_to_no_enrichment() {
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
        assert!(!params.include_recency);
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

    #[test]
    fn optional_list_recency_is_additive_without_requiring_it_from_old_daemons() {
        let params: WorkspacePathListParams = serde_json::from_value(json!({
            "scope":{"kind":"allRepositories"}, "includeRecency":true,
        }))
        .unwrap();
        assert!(params.include_recency);
        assert_eq!(
            serde_json::to_value(params).unwrap()["includeRecency"],
            true
        );
        let legacy = json!({
            "workspaceId":"id", "name":"fix/login",
            "repository":{"id":"repo", "displayName":"repo", "rootPath":"/repo"},
            "path":"/worktrees/fix/login", "branchName":"fix/login", "worktreeMode":"new_branch",
        });
        let mut entry: WorkspacePathEntry = serde_json::from_value(legacy.clone()).unwrap();
        assert!(entry.recency_at_ms.is_none());
        assert_eq!(serde_json::to_value(&entry).unwrap(), legacy);
        entry.recency_at_ms = Some(1_800_000_000_000);
        assert_eq!(
            serde_json::to_value(&entry).unwrap()["recencyAtMs"],
            1_800_000_000_000_i64
        );
    }
}
