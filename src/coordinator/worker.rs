use std::error::Error as StdError;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_json::Value;
use thiserror::Error;

use crate::domain::CodexThreadStatus;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StartedThread {
    pub(crate) id: String,
    pub(crate) status: CodexThreadStatus,
    pub(crate) cwd: PathBuf,
    pub(crate) response: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StartedTurn {
    pub(crate) id: String,
}

#[derive(Debug, Error)]
pub(crate) enum WorkerError {
    #[error(transparent)]
    Runtime(Box<dyn StdError + Send + Sync>),
    #[error("Codex response is missing required field {0}")]
    InvalidResponse(&'static str),
    #[error("Codex response contains an invalid native thread status: {0}")]
    InvalidThreadStatus(String),
    #[error("Codex thread ID mismatch: expected {expected}, received {actual}")]
    ThreadIdMismatch { expected: String, actual: String },
    #[error("Codex thread cwd mismatch: expected {expected}, received {actual}")]
    CwdMismatch { expected: PathBuf, actual: PathBuf },
}

impl WorkerError {
    pub(crate) fn runtime(source: impl StdError + Send + Sync + 'static) -> Self {
        Self::Runtime(Box::new(source))
    }
}

#[async_trait]
pub(crate) trait WorkerRuntime: Send + Sync + 'static {
    async fn start_thread(
        &self,
        name: &str,
        cwd: &Path,
        config: Value,
    ) -> Result<StartedThread, WorkerError>;

    async fn resume_thread(
        &self,
        thread_id: &str,
        cwd: &Path,
        config: Value,
    ) -> Result<StartedThread, WorkerError>;

    async fn start_turn(
        &self,
        thread_id: &str,
        cwd: &Path,
        client_message_id: &str,
        message: &str,
    ) -> Result<StartedTurn, WorkerError>;

    async fn respond_to_request(&self, id: Value, result: Value) -> Result<(), WorkerError>;
}
