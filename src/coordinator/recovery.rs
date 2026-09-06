use serde_json::json;
use tracing::warn;

use super::{Coordinator, CoordinatorError, WorkerError};
use crate::domain::{EventKind, EventSource, ProfileSnapshot, Task, TaskLifecycle};
use crate::profile::load_profile;
use crate::store::EventDraft;

const RECOVERY_ERROR_CODE: &str = "THREAD_RECOVERY_FAILED";

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ThreadRecoveryReport {
    pub(crate) attempted: usize,
    pub(crate) recovered: usize,
    pub(crate) failed: usize,
}

impl Coordinator {
    pub(crate) async fn recover_ready_threads(
        &self,
    ) -> Result<ThreadRecoveryReport, CoordinatorError> {
        let tasks = self.store.list_tasks(None)?;
        let mut report = ThreadRecoveryReport::default();
        for task in tasks.into_iter().filter(|task| {
            task.lifecycle == TaskLifecycle::Ready
                && task.thread_runtime.as_ref().is_none_or(|snapshot| {
                    !snapshot.is_fresh || snapshot.runtime_generation != self.runtime_generation
                })
        }) {
            report.attempted += 1;
            match self.resume_task_thread(&task).await {
                Ok(resumed) => {
                    self.store.observe_thread_status_with_event(
                        &task.id,
                        resumed.status.clone(),
                        &self.runtime_generation,
                        EventDraft {
                            task_id: Some(task.id.clone()),
                            turn_id: None,
                            kind: EventKind::ThreadStatusChanged,
                            source: EventSource::Codex,
                            source_method: Some("thread/resume".to_owned()),
                            occurred_at_ms: None,
                            payload: json!({
                                "status": resumed.status,
                                "reason": "daemon_recovery",
                            }),
                        },
                    )?;
                    report.recovered += 1;
                }
                Err(source) => {
                    let cause_code = source.code();
                    warn!(
                        task_id = %task.id,
                        thread_id = task.codex_thread_id.as_deref().unwrap_or("(missing)"),
                        cause_code,
                        "could not resume a persisted Codex thread"
                    );
                    self.store.record_thread_recovery_failure_with_event(
                        &task.id,
                        RECOVERY_ERROR_CODE,
                        recovery_message(&source),
                        EventDraft {
                            task_id: Some(task.id.clone()),
                            turn_id: None,
                            kind: EventKind::AgentFailed,
                            source: EventSource::Coco,
                            source_method: Some("startup.recover".to_owned()),
                            occurred_at_ms: None,
                            payload: json!({
                                "stage": "thread.resume",
                                "code": RECOVERY_ERROR_CODE,
                                "causeCode": cause_code,
                            }),
                        },
                    )?;
                    report.failed += 1;
                }
            }
        }
        Ok(report)
    }

    async fn resume_task_thread(
        &self,
        task: &Task,
    ) -> Result<super::StartedThread, CoordinatorError> {
        let thread_id = task
            .codex_thread_id
            .as_deref()
            .ok_or(CoordinatorError::IncompleteTask("Codex thread"))?;
        let worktree = task
            .worktree_path
            .as_deref()
            .ok_or(CoordinatorError::IncompleteTask("worktree"))?;
        let profile = load_profile(&task.profile.name, &self.codex_home)?;
        if !same_profile_source(&profile.snapshot, &task.profile) {
            return Err(CoordinatorError::ProfileChanged(task.profile.name.clone()));
        }
        let resumed = self
            .worker
            .resume_thread(thread_id, worktree, profile.thread_config)
            .await?;
        if resumed.id != thread_id {
            return Err(CoordinatorError::Worker(WorkerError::ThreadIdMismatch {
                expected: thread_id.to_owned(),
                actual: resumed.id,
            }));
        }
        if resumed.cwd != worktree {
            return Err(CoordinatorError::Worker(WorkerError::CwdMismatch {
                expected: worktree.to_owned(),
                actual: resumed.cwd,
            }));
        }
        Ok(resumed)
    }
}

fn same_profile_source(current: &ProfileSnapshot, stored: &ProfileSnapshot) -> bool {
    current.name == stored.name
        && current.source_path == stored.source_path
        && current.source_hash == stored.source_hash
}

fn recovery_message(error: &CoordinatorError) -> &'static str {
    match error {
        CoordinatorError::IncompleteTask(_) => {
            "The task is missing information required to resume its Codex thread"
        }
        CoordinatorError::ProfileChanged(_) => {
            "The task profile changed after the thread was created"
        }
        CoordinatorError::Profile(_) => "The task profile is unavailable or invalid",
        CoordinatorError::Worker(_) => "Codex could not resume the stored thread",
        _ => "CoCo could not recover the stored thread",
    }
}
