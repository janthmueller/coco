use super::*;

pub(super) fn validate_creation_replay(workspace: &Workspace) -> Result<(), CoordinatorError> {
    if workspace
        .context
        .pointer("/resolved/context/captureTiming")
        .and_then(Value::as_str)
        == Some("create")
        && workspace.codex_thread_id.is_none()
    {
        return Err(CoordinatorError::InvalidWorkspaceState {
            expected: "a confirmed context capture; inspect the workspace before retrying",
            actual: workspace.phase,
        });
    }
    Ok(())
}

impl Coordinator {
    pub(super) async fn capture_created_context(
        &self,
        workspace: Workspace,
        context: &CreationContext,
        profile: crate::profile::LoadedProfile,
    ) -> Result<Workspace, CoordinatorError> {
        let Some(fork) = &context.fork else {
            return Ok(workspace);
        };
        // Git completion already keeps inherited creation in Starting. Record
        // dispatch intent without a Ready gap before the native side effect;
        // restart reconciliation cannot mistake an unfinished capture for a
        // fresh prepared workspace or potentially fork it twice.
        let (workspace, _) = self.store.transition_workspace_lifecycle_with_event(
            &workspace.id,
            WorkspaceLifecycle::Starting,
            WorkspaceLifecycle::Starting,
            None,
            EventDraft::workspace(
                EventKind::ControlCallStarted,
                EventSource::Coco,
                json!({
                    "method": "thread/fork", "sourceThreadId": fork.thread_id,
                    "lastTurnId": fork.last_turn_id,
                }),
            ),
        )?;
        let captured = self
            .worker
            .fork_thread(
                &workspace.name,
                &fork.thread_id,
                fork.last_turn_id.as_deref(),
                workspace
                    .worktree_path
                    .as_deref()
                    .ok_or(CoordinatorError::IncompleteWorkspace("worktree"))?,
                profile.thread_config,
                workspace.profile.model_override.as_deref(),
            )
            .await;
        let captured = match captured {
            Ok(captured) => captured,
            Err(source) => {
                let error = CoordinatorError::Worker(source);
                self.mark_workspace_failed(
                    &workspace.id,
                    "thread.fork",
                    &error,
                    EventSource::Codex,
                );
                return Err(error);
            }
        };
        self.bind_context_thread(
            &workspace.id,
            context,
            profile.snapshot,
            &captured,
            false,
            WorkspaceLifecycle::Starting,
        )
    }
}
