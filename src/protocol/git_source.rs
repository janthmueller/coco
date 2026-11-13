use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{DaemonMethod, DaemonRequest, WorkspaceBaseRequest};
pub(crate) use crate::domain::git_source::{GitBranch, GitChangeSummary, RepositoryGitState};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RepositoryGitInspectParams {
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<WorkspaceBaseRequest>,
    #[serde(default, skip_serializing_if = "super::is_false")]
    pub include_branches: bool,
}

impl DaemonRequest for RepositoryGitInspectParams {
    type Response = RepositoryGitState;
    const METHOD: DaemonMethod = DaemonMethod::RepositoryGitInspect;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn git_inspection_is_a_read_only_typed_request_with_lazy_branch_discovery() {
        let params: RepositoryGitInspectParams =
            serde_json::from_value(json!({"path": "/repo/worktree"})).unwrap();
        assert!(!params.include_branches);
        assert!(params.base.is_none());
        assert_eq!(
            serde_json::to_value(params).unwrap(),
            json!({"path": "/repo/worktree"})
        );
        let value = json!({
            "path": "/repo/worktree", "includeBranches": true,
            "base": {"kind": "revision", "revision": "refs/heads/main"},
        });
        let params: RepositoryGitInspectParams = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(params).unwrap(), value);
        assert_eq!(
            RepositoryGitInspectParams::METHOD.as_str(),
            "repository.git.inspect"
        );
        assert!(
            serde_json::from_value::<RepositoryGitInspectParams>(json!({
                "path": "/repo", "register": true,
            }))
            .is_err()
        );
    }
}
