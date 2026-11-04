use super::*;
use crate::protocol::{
    TurnDispatchState, TurnGetParams, TurnInterruptAttemptState, TurnInterruptMode,
    TurnInterruptParams, TurnInterruptionState, TurnObservationState, TurnUnavailableReason,
};

mod best_effort;
mod budgets;
mod inspection;
mod interruption;
mod pending;
mod reconciliation;
mod recovery;

#[derive(Default)]
pub(super) struct WorkerState {
    outcomes: StdMutex<BTreeMap<(String, String), TurnTerminalStatus>>,
    in_progress: StdMutex<Option<(String, String)>>,
    lookups: StdMutex<Vec<(String, String)>>,
    interrupts: StdMutex<Vec<(String, String)>>,
    lookup_error: StdMutex<bool>,
    interrupt_error: StdMutex<bool>,
    interrupt_supported: Option<bool>,
    best_effort_supported: bool,
    interrupt_rejected: StdMutex<bool>,
    lookup_release: Option<Arc<Notify>>,
    lookup_entered: Option<Arc<Notify>>,
    history_unavailable: bool,
    interrupt_entered: Option<Arc<Notify>>,
    interrupt_release: Option<Arc<Notify>>,
}

impl WorkerState {
    pub(super) fn supports_interrupt(&self) -> bool {
        self.interrupt_supported.unwrap_or(true)
    }

    pub(super) fn supports_best_effort(&self) -> bool {
        self.best_effort_supported
    }

    pub(super) async fn read(
        &self,
        thread: &str,
        turn: &str,
    ) -> Result<NativeTurnOutcome, WorkerError> {
        if let Some(entered) = &self.lookup_entered {
            entered.notify_one();
        }
        if let Some(release) = &self.lookup_release {
            release.notified().await;
        }
        self.lookups
            .lock()
            .unwrap()
            .push((thread.to_owned(), turn.to_owned()));
        if *self.lookup_error.lock().unwrap() {
            return Err(WorkerError::runtime(std::io::Error::other(
                "metadata disconnected",
            )));
        }
        if self.history_unavailable {
            return Ok(NativeTurnOutcome::HistoryUnavailable);
        }
        if self.in_progress.lock().unwrap().as_ref() == Some(&(thread.to_owned(), turn.to_owned()))
        {
            return Ok(NativeTurnOutcome::InProgress);
        }
        Ok(self
            .outcomes
            .lock()
            .unwrap()
            .get(&(thread.to_owned(), turn.to_owned()))
            .copied()
            .map_or(NativeTurnOutcome::Unavailable, NativeTurnOutcome::Finished))
    }

    pub(super) async fn interrupt(&self, thread: &str, turn: &str) -> Result<(), WorkerError> {
        self.interrupts
            .lock()
            .unwrap()
            .push((thread.to_owned(), turn.to_owned()));
        if let Some(entered) = &self.interrupt_entered {
            entered.notify_one();
        }
        if let Some(release) = &self.interrupt_release {
            release.notified().await;
        }
        if *self.interrupt_error.lock().unwrap() {
            return Err(WorkerError::runtime(std::io::Error::other(
                "interrupt reply lost",
            )));
        }
        if *self.interrupt_rejected.lock().unwrap() {
            return Err(WorkerError::TurnInterruptRejected);
        }
        Ok(())
    }
}

fn get_params(fixture: &Fixture, workspace: &Workspace, key: &str) -> TurnGetParams {
    TurnGetParams {
        scope: RepositoryScope::repository(fixture.source.clone()),
        workspace: workspace.id.clone(),
        operation_id: key.to_owned(),
    }
}

fn interrupt_params(fixture: &Fixture, workspace: &Workspace, key: &str) -> TurnInterruptParams {
    let get = get_params(fixture, workspace, key);
    TurnInterruptParams {
        scope: get.scope,
        workspace: get.workspace,
        operation_id: get.operation_id,
        mode: TurnInterruptMode::Strict,
    }
}

async fn send(fixture: &Fixture, workspace: &Workspace, key: &str) -> WorkspaceResult {
    fixture
        .coordinator
        .start_turn(TurnStartParams {
            scope: RepositoryScope::repository(fixture.source.clone()),
            workspace: workspace.id.clone(),
            message: "Frozen instruction".to_owned(),
            operation_id: key.to_owned(),
        })
        .await
        .unwrap()
}

fn complete(fixture: &Fixture, sent: &WorkspaceResult, status: TurnTerminalStatus) {
    let thread = sent.workspace.codex_thread_id.as_deref().unwrap();
    fixture
        .worker
        .set_native_status(thread, CodexThreadStatus::Idle);
    fixture
        .coordinator
        .record_codex_event(CodexEvent::Notification {
            method: "turn/completed".to_owned(),
            params: json!({"threadId": thread,
            "turn": {"id": sent.codex_turn_id, "status": status.as_str()}}),
        })
        .unwrap();
}

async fn setup(worker: FakeWorker) -> (Fixture, Workspace) {
    let fixture = Fixture::new(worker);
    fixture.register().await;
    let workspace = fixture
        .create_and_materialize(fixture.create_params())
        .await;
    (fixture, workspace)
}
