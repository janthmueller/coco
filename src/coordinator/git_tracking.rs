use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::{StreamExt, stream};
use tokio::sync::Mutex as AsyncMutex;

use super::Coordinator;
use crate::domain::{Workspace, WorkspaceAvailability, WorkspaceLifecycle, WorktreeMode};
use crate::git::TrackingSnapshot;
use crate::protocol::{
    GitTracking, RepositoryGitInspectParams, RepositoryGitState, WorkspaceBaseRequest,
    WorkspaceListItem, WorkspacePathEntry,
};

const TRACKING_TTL: Duration = Duration::from_secs(5);
const TRACKING_BUDGET: Duration = Duration::from_millis(750);
const MAX_TRACKING_REPOSITORIES: usize = 64;
const MAX_TRACKING_READS: usize = 2;

type RepositoryKey = (String, PathBuf);
type Snapshots = HashMap<RepositoryKey, Option<Arc<TrackingSnapshot>>>;

#[derive(Default)]
pub(super) struct TrackingCache {
    entries: Mutex<HashMap<RepositoryKey, Arc<AsyncMutex<CachedTracking>>>>,
}

#[derive(Default)]
struct CachedTracking {
    read_at: Option<Instant>,
    snapshot: Option<Arc<TrackingSnapshot>>,
}

#[cfg(test)]
impl TrackingCache {
    pub(super) fn clear(&self) {
        self.entries.lock().unwrap().clear();
    }
}

impl Coordinator {
    async fn read_tracking(&self, key: &RepositoryKey) -> Option<Arc<TrackingSnapshot>> {
        let entry = {
            let mut entries = self.git_tracking.entries.lock().unwrap();
            if entries.len() >= MAX_TRACKING_REPOSITORIES && !entries.contains_key(key) {
                entries.clear();
            }
            Arc::clone(entries.entry(key.clone()).or_default())
        };
        // Only callers for this repository wait for its one in-flight read.
        // The outer view budget also bounds waiting for this mutex.
        let mut entry = entry.lock().await;
        if entry
            .read_at
            .is_some_and(|time| time.elapsed() < TRACKING_TTL)
        {
            return entry.snapshot.clone();
        }
        let snapshot = tokio::time::timeout(
            Duration::from_millis(500),
            self.git.tracking_snapshot(&key.1, &key.0),
        )
        .await
        .ok()
        .and_then(Result::ok)
        .map(Arc::new);
        entry.read_at = Some(Instant::now());
        entry.snapshot = snapshot.clone();
        snapshot
    }

    async fn tracking_snapshots(&self, repositories: Vec<RepositoryKey>) -> Snapshots {
        let unique = repositories.into_iter().collect::<BTreeSet<_>>();
        let mut reads = stream::iter(unique.into_iter().take(MAX_TRACKING_REPOSITORIES))
            .map(|key| async move {
                let snapshot = self.read_tracking(&key).await;
                (key, snapshot)
            })
            .buffer_unordered(MAX_TRACKING_READS);
        let mut snapshots = HashMap::new();
        let _ = tokio::time::timeout(TRACKING_BUDGET, async {
            while let Some((key, snapshot)) = reads.next().await {
                snapshots.insert(key, snapshot);
            }
        })
        .await;
        snapshots
    }

    pub(super) async fn enrich_workspace_tracking(&self, rows: &mut [WorkspaceListItem]) {
        let repositories = rows
            .iter()
            .filter(|row| eligible(&row.workspace))
            .map(|row| (row.repository.id.clone(), row.repository.root_path.clone()))
            .collect();
        let snapshots = self.tracking_snapshots(repositories).await;
        for row in rows {
            row.git_tracking = eligible(&row.workspace).then(|| {
                let key = (row.repository.id.clone(), row.repository.root_path.clone());
                workspace_tracking(
                    snapshots.get(&key).and_then(Option::as_deref),
                    &row.workspace,
                    None,
                )
            });
        }
    }

    pub(super) async fn enrich_path_tracking(&self, entries: &mut [WorkspacePathEntry]) {
        let repositories = entries
            .iter()
            .map(|entry| {
                (
                    entry.repository.id.clone(),
                    entry.repository.root_path.clone(),
                )
            })
            .collect();
        let snapshots = self.tracking_snapshots(repositories).await;
        for entry in entries {
            let key = (
                entry.repository.id.clone(),
                entry.repository.root_path.clone(),
            );
            entry.git_tracking = Some(
                snapshots
                    .get(&key)
                    .and_then(Option::as_deref)
                    .map(|snapshot| {
                        snapshot.worktree(
                            &entry.path,
                            entry.worktree_mode,
                            entry.branch_name.as_deref(),
                            None,
                        )
                    })
                    .unwrap_or_else(GitTracking::unknown),
            );
        }
    }

    pub(super) async fn tracking_for_workspace(
        &self,
        workspace: &Workspace,
        repository_path: &Path,
        expected_commit: &str,
    ) -> GitTracking {
        let key = (workspace.repository_id.clone(), repository_path.to_owned());
        let snapshots = self.tracking_snapshots(vec![key.clone()]).await;
        workspace_tracking(
            snapshots.get(&key).and_then(Option::as_deref),
            workspace,
            Some(expected_commit),
        )
    }

    pub(crate) async fn enrich_git_inspection(
        &self,
        state: &mut RepositoryGitState,
        params: &RepositoryGitInspectParams,
    ) {
        let key = (state.repository_id.clone(), state.root_path.clone());
        let snapshots = self.tracking_snapshots(vec![key.clone()]).await;
        let snapshot = snapshots.get(&key).and_then(Option::as_deref);
        state.tracking = Some(
            snapshot
                .map(|snapshot| {
                    snapshot.worktree(
                        &state.root_path,
                        if state.branch.is_some() {
                            WorktreeMode::ExistingBranch
                        } else {
                            WorktreeMode::Detached
                        },
                        state.branch.as_deref(),
                        state.head_sha.as_deref(),
                    )
                })
                .unwrap_or_else(GitTracking::unknown),
        );
        for branch in &mut state.branches {
            if !branch.remote {
                branch.tracking = Some(
                    snapshot
                        .map(|snapshot| snapshot.branch(&branch.reference, &branch.commit))
                        .unwrap_or_else(GitTracking::unknown),
                );
            }
        }
        state.base_tracking = self.base_tracking(state, params, snapshot);
    }

    fn base_tracking(
        &self,
        state: &RepositoryGitState,
        params: &RepositoryGitInspectParams,
        snapshot: Option<&TrackingSnapshot>,
    ) -> Option<GitTracking> {
        let commit = state.base_sha.as_deref()?;
        match params.base.as_ref()? {
            WorkspaceBaseRequest::Revision { revision } if revision == "HEAD" => {
                state.tracking.clone()
            }
            WorkspaceBaseRequest::Revision { revision } if revision.starts_with("refs/heads/") => {
                Some(
                    snapshot
                        .map(|snapshot| snapshot.branch(revision, commit))
                        .unwrap_or_else(GitTracking::unknown),
                )
            }
            WorkspaceBaseRequest::Workspace { workspace } => {
                // The mandatory inspection already validated this scoped base;
                // optional metadata must not repeat synchronous Git discovery.
                let workspace = self.store.workspace_by_id(workspace).ok()?.or_else(|| {
                    self.store
                        .workspace_by_name(&state.repository_id, workspace)
                        .ok()
                        .flatten()
                })?;
                if workspace.repository_id != state.repository_id {
                    return None;
                }
                Some(workspace_tracking(snapshot, &workspace, Some(commit)))
            }
            _ => None,
        }
    }
}

fn eligible(workspace: &Workspace) -> bool {
    workspace.availability == WorkspaceAvailability::Open
        && workspace.lifecycle == WorkspaceLifecycle::Ready
        && workspace.worktree_path.is_some()
}

fn workspace_tracking(
    snapshot: Option<&TrackingSnapshot>,
    workspace: &Workspace,
    commit: Option<&str>,
) -> GitTracking {
    snapshot
        .zip(workspace.worktree_path.as_deref())
        .map(|(snapshot, path)| {
            snapshot.worktree(
                path,
                workspace.worktree_mode,
                workspace.branch_name.as_deref(),
                commit,
            )
        })
        .unwrap_or_else(GitTracking::unknown)
}
