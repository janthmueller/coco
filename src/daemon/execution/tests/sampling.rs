use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

use super::support::{Fixture, TEST_TIMEOUT, finish_activation};
use super::*;

type Observation = Result<WorkspaceRuntimeResources, WorkspaceExecutionError>;

struct BlockingGate {
    release: Option<mpsc::Sender<()>>,
    started: tokio::sync::oneshot::Receiver<()>,
}

impl BlockingGate {
    fn new(
        calls: Arc<AtomicUsize>,
    ) -> (
        Self,
        impl FnOnce(&mut ManagedExecServer) -> Observation + Send + 'static,
    ) {
        let (release, wait) = mpsc::channel();
        let (started, notified) = tokio::sync::oneshot::channel();
        let sample = move |runtime: &mut ManagedExecServer| {
            calls.fetch_add(1, Ordering::SeqCst);
            let _ = started.send(());
            // Finite even if a regression blocks the single Tokio worker:
            // a failed assertion/unwind must never hang runtime shutdown.
            wait.recv_timeout(TEST_TIMEOUT).map_err(|error| {
                WorkspaceExecutionError::ProcessInspection(std::io::Error::other(error))
            })?;
            runtime.resources()
        };
        (
            Self {
                release: Some(release),
                started: notified,
            },
            sample,
        )
    }

    async fn started(&mut self) {
        timeout(TEST_TIMEOUT, &mut self.started)
            .await
            .expect("blocking sampler never started")
            .expect("blocking sampler dropped its startup signal");
    }
}

impl Drop for BlockingGate {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}

#[tokio::test]
async fn blocking_sampling_times_out_without_stalling_or_queueing_jobs_and_close_waits_for_it() {
    // The single-thread runtime makes accidental synchronous sampling block
    // the timeout itself; an async sleep fake would not detect this regression.
    let mut fixture = Fixture::new().await;
    for workspace in ["sampled", "other"] {
        let activation = fixture.activate(workspace);
        fixture.complete_activation(workspace).await;
        finish_activation(activation).await;
    }
    let pid = fixture.pid("sampled").await;
    let calls = Arc::new(AtomicUsize::new(0));
    let (mut gate, sample) = BlockingGate::new(Arc::clone(&calls));
    let result = timeout(
        Duration::from_millis(200),
        fixture.executors.sample_resources("sampled", sample),
    )
    .await;
    assert!(
        result.is_err(),
        "a blocking sample defeated its caller timeout"
    );
    gate.started().await;

    let other = timeout(TEST_TIMEOUT, fixture.executors.resources("other"))
        .await
        .expect("another workspace stalled behind the blocking sampler")
        .unwrap();
    assert_eq!(other.state, WorkspaceRuntimeState::Running);
    for _ in 0..3 {
        let calls = Arc::clone(&calls);
        assert!(
            timeout(
                Duration::from_millis(20),
                fixture
                    .executors
                    .sample_resources("sampled", move |runtime| {
                        calls.fetch_add(1, Ordering::SeqCst);
                        runtime.resources()
                    }),
            )
            .await
            .is_err()
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let close = fixture.executors.close();
    tokio::pin!(close);
    assert!(futures_util::poll!(&mut close).is_pending());
    drop(gate);
    timeout(TEST_TIMEOUT, close).await.unwrap();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "cancelled reads queued sampling jobs"
    );
    Fixture::assert_stopped(pid).await;
    fixture.finish().await;
}

#[tokio::test]
async fn cancelled_sampling_retains_the_operation_lock_until_stop_can_safely_run() {
    let mut fixture = Fixture::new().await;
    let activation = fixture.activate("sampled");
    fixture.complete_activation("sampled").await;
    finish_activation(activation).await;
    let pid = fixture.pid("sampled").await;
    let (mut gate, sample) = BlockingGate::new(Arc::new(AtomicUsize::new(0)));
    assert!(
        timeout(
            Duration::from_millis(200),
            fixture.executors.sample_resources("sampled", sample),
        )
        .await
        .is_err()
    );
    gate.started().await;
    assert!(
        fixture
            .executors
            .existing_entry("sampled")
            .await
            .unwrap()
            .try_lock()
            .is_err(),
        "caller cancellation released the physical sample's operation lock"
    );
    let stop = fixture.executors.stop("sampled");
    tokio::pin!(stop);
    assert!(futures_util::poll!(&mut stop).is_pending());
    drop(gate);
    timeout(TEST_TIMEOUT, stop).await.unwrap().unwrap();
    Fixture::assert_stopped(pid).await;
    assert_eq!(
        fixture.executors.resources("sampled").await.unwrap().state,
        WorkspaceRuntimeState::Inactive
    );
    fixture.finish().await;
}

#[tokio::test]
async fn failed_or_panicking_sampling_releases_both_guards_for_later_reads_and_cleanup() {
    let mut fixture = Fixture::new().await;
    let activation = fixture.activate("sampled");
    fixture.complete_activation("sampled").await;
    finish_activation(activation).await;
    let failure = fixture
        .executors
        .sample_resources("sampled", |_| {
            Err(WorkspaceExecutionError::ProcessInspection(
                std::io::Error::other("synthetic sampling error"),
            ))
        })
        .await;
    assert!(matches!(
        failure,
        Err(WorkspaceExecutionError::ProcessInspection(_))
    ));
    let panic = fixture
        .executors
        .sample_resources("sampled", |_| panic!("synthetic sampler panic"))
        .await;
    assert!(
        matches!(panic, Err(WorkspaceExecutionError::OperationTask(error)) if error.is_panic())
    );
    assert_eq!(
        timeout(TEST_TIMEOUT, fixture.executors.resources("sampled"))
            .await
            .unwrap()
            .unwrap()
            .state,
        WorkspaceRuntimeState::Running
    );
    fixture.finish().await;
}

#[tokio::test]
async fn inactive_sampling_never_calls_the_sampler_or_creates_a_runtime() {
    let mut fixture = Fixture::new().await;
    for workspace in ["unknown", "prepared"] {
        if workspace == "prepared" {
            fixture.executors.entry(workspace).await;
        }
        let result = fixture
            .executors
            .sample_resources(workspace, |_| {
                panic!("inactive runtime must not be sampled")
            })
            .await
            .unwrap();
        assert_eq!(result.state, WorkspaceRuntimeState::Inactive);
        assert!(result.process_id.is_none());
    }
    assert_eq!(fixture.executors.inner.entries.lock().await.len(), 1);
    fixture.assert_no_more_requests();
    fixture.finish().await;
}
