use std::collections::hash_map::Entry;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::oneshot;
use tokio::time::timeout;

use super::{Coordinator, CoordinatorError};

const COMPACTION_TIMEOUT: Duration = Duration::from_secs(15 * 60);

pub(super) struct PendingCompaction {
    completion: oneshot::Sender<Result<(), String>>,
    turn_id: Option<String>,
    item_completed: bool,
}

impl Coordinator {
    pub(super) async fn compact_thread(&self, thread_id: &str) -> Result<(), CoordinatorError> {
        let (completion, receiver) = oneshot::channel();
        {
            let mut pending = self
                .pending_compactions
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let Entry::Vacant(entry) = pending.entry(thread_id.to_owned()) else {
                return Err(CoordinatorError::CompactionFailed(
                    "another compaction is already pending for the thread".to_owned(),
                ));
            };
            entry.insert(PendingCompaction {
                completion,
                turn_id: None,
                item_completed: false,
            });
        }

        if let Err(source) = self.worker.compact_thread(thread_id).await {
            self.remove_pending_compaction(thread_id);
            return Err(CoordinatorError::CompactionFailed(source.to_string()));
        }

        match timeout(COMPACTION_TIMEOUT, receiver).await {
            Ok(Ok(Ok(()))) => Ok(()),
            Ok(Ok(Err(message))) => Err(CoordinatorError::CompactionFailed(message)),
            Ok(Err(_)) => Err(CoordinatorError::CompactionFailed(
                "the App Server event stream closed before compaction completed".to_owned(),
            )),
            Err(_) => {
                self.remove_pending_compaction(thread_id);
                Err(CoordinatorError::CompactionTimedOut)
            }
        }
    }

    /// Observes and consumes the native turn used solely to prepare a compacted
    /// fork. It must not become a user-visible CoCo turn.
    pub(super) fn observe_pending_compaction(&self, method: &str, params: &Value) -> bool {
        let Some(thread_id) = params.get("threadId").and_then(Value::as_str) else {
            return false;
        };
        let mut pending = self
            .pending_compactions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(compaction) = pending.get_mut(thread_id) else {
            return false;
        };

        let mut outcome = None;
        match method {
            "turn/started" => {
                compaction.turn_id = params
                    .pointer("/turn/id")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
            }
            "item/completed"
                if params.pointer("/item/type").and_then(Value::as_str)
                    == Some("contextCompaction") =>
            {
                compaction.item_completed = true;
                if compaction.turn_id.is_none() {
                    compaction.turn_id = params
                        .get("turnId")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned);
                }
            }
            "error"
                if !params
                    .get("willRetry")
                    .and_then(Value::as_bool)
                    .unwrap_or(false) =>
            {
                outcome = Some(Err("Codex reported a terminal compaction error".to_owned()));
            }
            "turn/completed" => {
                let turn_id = params.pointer("/turn/id").and_then(Value::as_str);
                if compaction
                    .turn_id
                    .as_deref()
                    .is_none_or(|known| Some(known) == turn_id)
                {
                    let status = params
                        .pointer("/turn/status")
                        .and_then(Value::as_str)
                        .unwrap_or("failed");
                    outcome = Some(if status == "completed" && compaction.item_completed {
                        Ok(())
                    } else if status == "completed" {
                        Err(
                            "Codex completed the compaction turn without a context-compaction item"
                                .to_owned(),
                        )
                    } else {
                        Err(format!("Codex compaction ended with status {status:?}"))
                    });
                }
            }
            _ => {}
        }

        if let Some(outcome) = outcome {
            let compaction = pending
                .remove(thread_id)
                .expect("the pending compaction was found above");
            let _ = compaction.completion.send(outcome);
        }
        true
    }

    pub(super) fn fail_pending_compactions(&self) {
        let pending = std::mem::take(
            &mut *self
                .pending_compactions
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        );
        for (_, compaction) in pending {
            let _ = compaction.completion.send(Err(
                "the App Server disconnected before compaction completed".to_owned(),
            ));
        }
    }

    fn remove_pending_compaction(&self, thread_id: &str) {
        self.pending_compactions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(thread_id);
    }
}
