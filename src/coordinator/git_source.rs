use super::{Coordinator, CoordinatorError};
use crate::protocol::{RepositoryGitInspectParams, RepositoryGitState, WorkspaceBaseRequest};

impl Coordinator {
    pub(crate) fn inspect_repository_git(
        &self,
        params: RepositoryGitInspectParams,
    ) -> Result<RepositoryGitState, CoordinatorError> {
        let repository = self.git.discover(&params.path)?;
        let mut state = self
            .git
            .inspect_source(&repository, params.include_branches)?;
        state.base_sha = params
            .base
            .as_ref()
            .map(|base| match base {
                WorkspaceBaseRequest::Revision { revision } => {
                    super::validate_non_empty("base.revision", revision)?;
                    Ok(self.git.resolve_commit(&repository, revision)?)
                }
                WorkspaceBaseRequest::Workspace { .. } => {
                    self.preview_workspace_base(base, &repository)
                }
            })
            .transpose()?;
        if matches!(&params.base, Some(WorkspaceBaseRequest::Revision { revision }) if revision == "HEAD")
            && state.base_sha != state.head_sha
        {
            return Err(CoordinatorError::CreationBaseChanged);
        }
        Ok(state)
    }
}
