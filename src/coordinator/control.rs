use super::{Coordinator, CoordinatorError};
use crate::git::GitError;
use crate::protocol::{ControlRepository, ControlSessionPreparation, ControlSessionPrepareParams};

const MAX_CONTROL_REPOSITORIES: usize = 32;

impl Coordinator {
    /// Read-only preparation; launching a control dialog never registers a
    /// repository, creates a workspace, or starts a worker executor.
    pub(crate) async fn prepare_control_session(
        &self,
        params: ControlSessionPrepareParams,
    ) -> Result<ControlSessionPreparation, CoordinatorError> {
        let working_directory = std::fs::canonicalize(&params.repository_path).map_err(|_| {
            CoordinatorError::InvalidParams("control directory is unavailable".into())
        })?;
        let current = match self.git.discover(&working_directory) {
            Ok(repository) => Some(repository),
            Err(GitError::NotAWorktree(_)) => None,
            Err(error) => return Err(error.into()),
        };
        let default_repository_id = current.as_ref().map(|repo| repo.id.clone());
        let mut repositories = current
            .into_iter()
            .map(|repo| ControlRepository {
                id: repo.id,
                name: repo.display_name,
                path: repo.root_path,
            })
            .collect::<Vec<_>>();
        let mut unavailable_repositories = Vec::new();
        let registered = self.store.list_repositories()?;
        if registered.len() > MAX_CONTROL_REPOSITORIES {
            return Err(CoordinatorError::InvalidParams(
                "CoCo AI currently supports up to 32 registered repositories".into(),
            ));
        }
        for repo in registered {
            if default_repository_id.as_deref() == Some(&repo.id) {
                if let Some(current) = repositories.first_mut() {
                    current.name = repo.display_name;
                }
                continue;
            }
            match self.git.discover(&repo.root_path) {
                Ok(discovered) if discovered.id == repo.id => {
                    repositories.push(ControlRepository {
                        id: repo.id,
                        name: repo.display_name,
                        path: discovered.root_path,
                    })
                }
                _ => unavailable_repositories.push(repo.root_path),
            }
        }
        if repositories.is_empty() {
            return Err(CoordinatorError::InvalidParams(
                "no available repositories; run coco /path/to/repository ai or register one with coco repo add".into(),
            ));
        }
        repositories.sort_by(|a, b| {
            let default =
                |repo: &ControlRepository| Some(&repo.id) != default_repository_id.as_ref();
            default(a).cmp(&default(b)).then(a.path.cmp(&b.path))
        });
        let mcp_servers = self.worker.control_mcp_servers(&working_directory).await?;
        Ok(ControlSessionPreparation {
            working_directory,
            default_repository_id,
            repositories,
            unavailable_repositories,
            codex_home: self.codex_home.clone(),
            coco_version: env!("CARGO_PKG_VERSION").into(),
            mcp_servers,
        })
    }
}
