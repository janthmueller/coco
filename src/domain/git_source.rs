use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GitChangeSummary {
    pub tracked_files: usize,
    pub untracked_files: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GitBranch {
    pub name: String,
    pub reference: String,
    pub commit: String,
    pub remote: bool,
    pub checkout_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RepositoryGitState {
    pub repository_id: String,
    pub root_path: PathBuf,
    pub branch: Option<String>,
    pub head_sha: Option<String>,
    pub changes: GitChangeSummary,
    pub branches: Vec<GitBranch>,
    pub base_sha: Option<String>,
}
