use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde_json::json;
use uuid::Uuid;

use super::{Coordinator, CoordinatorError, validate_non_empty};
use crate::domain::{
    ContextMode, EventKind, EventSource, Workspace, WorkspaceAvailability, WorkspaceLifecycle,
};
use crate::profile::load_profile;
use crate::protocol::{
    WorkspaceAttachAdoptParams, WorkspaceAttachAdoptResult, WorkspaceAttachLaunch,
    WorkspaceAttachParams, WorkspaceAttachReleaseParams, WorkspaceAttachReleaseResult,
    WorkspaceAttachRenewParams, WorkspaceAttachRenewResult, WorkspaceAttachResult,
};
use crate::store::{EventDraft, NewThreadBinding};

const JUMP_LEASE_TTL: Duration = Duration::from_secs(30);

struct JumpLease {
    workspace_id: String,
    expires_at: Instant,
    candidate_thread_id: Option<String>,
    pending_adoption: bool,
}

#[derive(Default)]
pub(super) struct JumpLeaseRegistry {
    leases: HashMap<String, JumpLease>,
}

impl JumpLeaseRegistry {
    fn acquire(
        &mut self,
        workspace_id: &str,
        pending_adoption: bool,
    ) -> Result<String, CoordinatorError> {
        self.prune();
        if self.leases.values().any(|lease| {
            lease.workspace_id == workspace_id && (pending_adoption || lease.pending_adoption)
        }) {
            return Err(CoordinatorError::WorkspaceAttachInProgress);
        }
        let id = Uuid::new_v4().to_string();
        self.leases.insert(
            id.clone(),
            JumpLease {
                workspace_id: workspace_id.to_owned(),
                expires_at: Instant::now() + JUMP_LEASE_TTL,
                candidate_thread_id: None,
                pending_adoption,
            },
        );
        Ok(id)
    }

    fn claim_candidate(
        &mut self,
        workspace_id: &str,
        lease_id: &str,
        thread_id: &str,
    ) -> Result<(), CoordinatorError> {
        self.prune();
        let lease = self
            .leases
            .get_mut(lease_id)
            .filter(|lease| lease.workspace_id == workspace_id && lease.pending_adoption)
            .ok_or(CoordinatorError::InvalidWorkspaceAttachLease)?;
        if lease
            .candidate_thread_id
            .as_deref()
            .is_some_and(|candidate| candidate != thread_id)
        {
            return Err(CoordinatorError::InvalidWorkspaceAttachLease);
        }
        lease.candidate_thread_id = Some(thread_id.to_owned());
        lease.expires_at = Instant::now() + JUMP_LEASE_TTL;
        Ok(())
    }

    fn reject_active(&mut self, workspace_id: &str) -> Result<(), CoordinatorError> {
        self.prune();
        if self
            .leases
            .values()
            .any(|lease| lease.workspace_id == workspace_id)
        {
            Err(CoordinatorError::WorkspaceAttachInProgress)
        } else {
            Ok(())
        }
    }

    fn reject_pending_adoption(&mut self, workspace_id: &str) -> Result<(), CoordinatorError> {
        self.prune();
        if self
            .leases
            .values()
            .any(|lease| lease.workspace_id == workspace_id && lease.pending_adoption)
        {
            Err(CoordinatorError::WorkspaceAttachInProgress)
        } else {
            Ok(())
        }
    }

    fn mark_bound(&mut self, workspace_id: &str, lease_id: &str) -> Result<(), CoordinatorError> {
        self.renew(workspace_id, lease_id)?;
        self.leases
            .get_mut(lease_id)
            .expect("renewed lease")
            .pending_adoption = false;
        Ok(())
    }

    fn renew(&mut self, workspace_id: &str, lease_id: &str) -> Result<(), CoordinatorError> {
        self.prune();
        let lease = self
            .leases
            .get_mut(lease_id)
            .filter(|lease| lease.workspace_id == workspace_id)
            .ok_or(CoordinatorError::InvalidWorkspaceAttachLease)?;
        lease.expires_at = Instant::now() + JUMP_LEASE_TTL;
        Ok(())
    }

    fn release(&mut self, workspace_id: &str, lease_id: &str) -> Result<(), CoordinatorError> {
        self.prune();
        match self.leases.get(lease_id) {
            Some(lease) if lease.workspace_id != workspace_id => {
                return Err(CoordinatorError::InvalidWorkspaceAttachLease);
            }
            Some(_) => {
                self.leases.remove(lease_id);
            }
            None if self
                .leases
                .values()
                .any(|lease| lease.workspace_id == workspace_id) =>
            {
                return Err(CoordinatorError::InvalidWorkspaceAttachLease);
            }
            None => {}
        }
        Ok(())
    }

    fn prune(&mut self) {
        let now = Instant::now();
        self.leases.retain(|_, lease| lease.expires_at > now);
    }

    fn clear(&mut self) {
        self.leases.clear();
    }
}

impl Coordinator {
    pub(crate) async fn attach_workspace(
        &self,
        params: WorkspaceAttachParams,
    ) -> Result<WorkspaceAttachResult, CoordinatorError> {
        let resolved = self.resolve_workspace(&params.scope, &params.workspace)?;
        let repository_lock = self.repository_lock(&resolved.repository_id).await;
        let _guard = repository_lock.lock().await;
        let workspace = self.resolve_workspace(&params.scope, &params.workspace)?;

        if workspace.availability != WorkspaceAvailability::Open {
            return Err(CoordinatorError::InvalidWorkspaceState {
                expected: "open",
                actual: workspace.phase,
            });
        }

        if workspace.codex_thread_id.is_some() {
            return self.resume_launch(workspace).await;
        }
        if workspace.lifecycle != WorkspaceLifecycle::Ready {
            return Err(CoordinatorError::InvalidWorkspaceState {
                expected: "ready",
                actual: workspace.phase,
            });
        }
        if workspace.context_mode == ContextMode::Fork {
            let workspace = self.materialize_workspace_thread(workspace).await?;
            return self.resume_launch(workspace).await;
        }
        if workspace.context_mode != ContextMode::Fresh {
            return Err(CoordinatorError::InvalidParams(
                "only fresh or fork context can be opened in the Codex terminal UI".to_owned(),
            ));
        }

        self.validate_workspace_profile(&workspace)?;
        let lease_id = self
            .jump_leases
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .acquire(&workspace.id, true)?;
        Ok(WorkspaceAttachResult {
            workspace,
            launch: WorkspaceAttachLaunch::Start { lease_id },
        })
    }

    pub(crate) async fn adopt_workspace_thread(
        &self,
        params: WorkspaceAttachAdoptParams,
    ) -> Result<WorkspaceAttachAdoptResult, CoordinatorError> {
        validate_non_empty("workspaceId", &params.workspace_id)?;
        validate_non_empty("leaseId", &params.lease_id)?;
        validate_non_empty("threadId", &params.thread_id)?;
        let workspace = self
            .store
            .workspace_by_id(&params.workspace_id)?
            .ok_or_else(|| CoordinatorError::WorkspaceNotFound {
                reference: params.workspace_id.clone(),
                candidates: Vec::new(),
            })?;
        let repository_lock = self.repository_lock(&workspace.repository_id).await;
        let _guard = repository_lock.lock().await;
        let workspace = self
            .store
            .workspace_by_id(&params.workspace_id)?
            .ok_or_else(|| CoordinatorError::WorkspaceNotFound {
                reference: params.workspace_id.clone(),
                candidates: Vec::new(),
            })?;

        if let Some(bound) = workspace.codex_thread_id.as_deref() {
            if bound != params.thread_id {
                return Err(CoordinatorError::InvalidWorkspaceAttachLease);
            }
            return self.finish_adoption(workspace, &params.lease_id).await;
        }
        self.jump_leases
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .claim_candidate(&params.workspace_id, &params.lease_id, &params.thread_id)?;
        validate_fresh_adoption_target(&workspace)?;
        self.validate_workspace_profile(&workspace)?;
        let worktree = workspace
            .worktree_path
            .as_deref()
            .ok_or(CoordinatorError::IncompleteWorkspace("worktree"))?;
        let Some(native) = self
            .worker
            .find_materialized_thread(&params.thread_id, worktree)
            .await?
        else {
            return Ok(WorkspaceAttachAdoptResult::Pending);
        };
        if native.id != params.thread_id {
            return Err(CoordinatorError::Worker(
                super::WorkerError::ThreadIdMismatch {
                    expected: params.thread_id,
                    actual: native.id,
                },
            ));
        }
        if native.cwd != worktree {
            return Err(CoordinatorError::Worker(super::WorkerError::CwdMismatch {
                expected: worktree.to_owned(),
                actual: native.cwd,
            }));
        }
        if native.forked_from_id.is_some() {
            return Err(CoordinatorError::InvalidParams(
                "the fresh TUI launch unexpectedly created a forked thread".to_owned(),
            ));
        }

        self.worker
            .set_thread_name(&params.thread_id, &workspace.name)
            .await?;
        let workspace = self
            .store
            .bind_thread_with_event(
                &workspace.id,
                WorkspaceLifecycle::Ready,
                WorkspaceLifecycle::Ready,
                NewThreadBinding {
                    thread_id: params.thread_id.clone(),
                    parent_thread_id: None,
                },
                EventDraft::workspace(
                    EventKind::AgentStarted,
                    EventSource::Codex,
                    json!({
                        "threadId": params.thread_id,
                        "contextMode": ContextMode::Fresh,
                        "source": "remoteTui",
                    }),
                ),
            )?
            .0;
        self.finish_adoption(workspace, &params.lease_id).await
    }

    async fn finish_adoption(
        &self,
        workspace: Workspace,
        lease_id: &str,
    ) -> Result<WorkspaceAttachAdoptResult, CoordinatorError> {
        self.jump_leases
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .mark_bound(&workspace.id, lease_id)?;
        let workspace = self.ensure_workspace_thread_loaded(workspace).await?;
        Ok(WorkspaceAttachAdoptResult::Bound {
            workspace: Box::new(workspace),
        })
    }

    pub(crate) fn renew_workspace_attach(
        &self,
        params: WorkspaceAttachRenewParams,
    ) -> Result<WorkspaceAttachRenewResult, CoordinatorError> {
        validate_non_empty("workspaceId", &params.workspace_id)?;
        validate_non_empty("leaseId", &params.lease_id)?;
        self.jump_leases
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .renew(&params.workspace_id, &params.lease_id)?;
        Ok(WorkspaceAttachRenewResult {})
    }

    pub(crate) fn release_workspace_attach(
        &self,
        params: WorkspaceAttachReleaseParams,
    ) -> Result<WorkspaceAttachReleaseResult, CoordinatorError> {
        validate_non_empty("workspaceId", &params.workspace_id)?;
        validate_non_empty("leaseId", &params.lease_id)?;
        self.jump_leases
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .release(&params.workspace_id, &params.lease_id)?;
        Ok(WorkspaceAttachReleaseResult {})
    }

    pub(super) fn reject_workspace_attachment(
        &self,
        workspace_id: &str,
    ) -> Result<(), CoordinatorError> {
        self.jump_leases
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .reject_active(workspace_id)
    }

    pub(super) fn reject_pending_adoption(
        &self,
        workspace_id: &str,
    ) -> Result<(), CoordinatorError> {
        self.jump_leases
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .reject_pending_adoption(workspace_id)
    }

    pub(super) fn clear_jump_leases(&self) {
        self.jump_leases
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }

    async fn resume_launch(
        &self,
        workspace: Workspace,
    ) -> Result<WorkspaceAttachResult, CoordinatorError> {
        let workspace = self.ensure_workspace_thread_loaded(workspace).await?;
        let thread_id = workspace
            .codex_thread_id
            .clone()
            .ok_or(CoordinatorError::IncompleteWorkspace("Codex thread"))?;
        let lease_id = self
            .jump_leases
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .acquire(&workspace.id, false)?;
        Ok(WorkspaceAttachResult {
            workspace,
            launch: WorkspaceAttachLaunch::Resume {
                thread_id,
                lease_id,
            },
        })
    }

    fn validate_workspace_profile(&self, workspace: &Workspace) -> Result<(), CoordinatorError> {
        let current = load_profile(&workspace.profile.name, &self.codex_home)?;
        if super::recovery::same_profile_source(&current.snapshot, &workspace.profile) {
            Ok(())
        } else {
            Err(CoordinatorError::ProfileChanged(
                workspace.profile.name.clone(),
            ))
        }
    }
}

fn validate_fresh_adoption_target(workspace: &Workspace) -> Result<(), CoordinatorError> {
    if workspace.lifecycle != WorkspaceLifecycle::Ready {
        return Err(CoordinatorError::InvalidWorkspaceState {
            expected: "ready",
            actual: workspace.phase,
        });
    }
    if workspace.context_mode != ContextMode::Fresh {
        return Err(CoordinatorError::InvalidParams(
            "only a fresh workspace may adopt a TUI-created thread".to_owned(),
        ));
    }
    Ok(())
}
