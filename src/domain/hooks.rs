use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(crate) const HOOK_EVENT_SCHEMA_VERSION: u32 = 1;
pub(crate) const GUARD_REQUEST_SCHEMA_VERSION: u32 = 1;
pub(crate) const MAX_HOOKS: usize = 64;
pub(crate) const MAX_GUARDS: usize = 64;
pub(crate) const MAX_HOOK_CONFIG_BYTES: u64 = 64 * 1024;
pub(crate) const MAX_HOOK_COMMAND_PARTS: usize = 33;
pub(crate) const MAX_HOOK_COMMAND_BYTES: usize = 16 * 1024;
pub(crate) const MAX_HOOK_TIMEOUT_SECONDS: u64 = 300;
pub(crate) const MAX_HOOK_ATTEMPTS: u32 = 5;
pub(crate) const MAX_GUARD_TIMEOUT_SECONDS: u64 = 30;
pub(crate) const MAX_GUARD_OUTPUT_BYTES: usize = 8 * 1024;
pub(crate) const MAX_GUARD_REASON_CHARS: usize = 512;
pub(crate) const HOOK_DELIVERY_RETENTION: i64 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum HookEventKind {
    #[serde(rename = "signal.emitted")]
    SignalEmitted,
    #[serde(rename = "workspace.created")]
    WorkspaceCreated,
    #[serde(rename = "workspace.closed")]
    WorkspaceClosed,
    #[serde(rename = "workspace.reopened")]
    WorkspaceReopened,
    #[serde(rename = "workspace.deleted")]
    WorkspaceDeleted,
}

impl HookEventKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::SignalEmitted => "signal.emitted",
            Self::WorkspaceCreated => "workspace.created",
            Self::WorkspaceClosed => "workspace.closed",
            Self::WorkspaceReopened => "workspace.reopened",
            Self::WorkspaceDeleted => "workspace.deleted",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "signal.emitted" => Some(Self::SignalEmitted),
            "workspace.created" => Some(Self::WorkspaceCreated),
            "workspace.closed" => Some(Self::WorkspaceClosed),
            "workspace.reopened" => Some(Self::WorkspaceReopened),
            "workspace.deleted" => Some(Self::WorkspaceDeleted),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HookEvent {
    pub schema_version: u32,
    pub id: String,
    pub kind: HookEventKind,
    pub occurred_at_ms: i64,
    pub repository: HookRepository,
    pub workspace: HookWorkspace,
    pub data: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HookRepository {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HookWorkspace {
    pub id: String,
    pub name: String,
    pub thread_id: Option<String>,
    pub worktree_path: Option<PathBuf>,
    pub branch_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum GuardAction {
    #[serde(rename = "workspace.close")]
    WorkspaceClose,
    #[serde(rename = "workspace.delete")]
    WorkspaceDelete,
}

impl GuardAction {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::WorkspaceClose => "workspace.close",
            Self::WorkspaceDelete => "workspace.delete",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GuardErrorPolicy {
    Allow,
    Deny,
}

impl GuardErrorPolicy {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GuardRequest {
    pub schema_version: u32,
    pub id: String,
    pub action: GuardAction,
    pub requested_at_ms: i64,
    pub repository: HookRepository,
    pub workspace: HookWorkspace,
    pub data: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GuardSummary {
    pub id: String,
    pub action: GuardAction,
    pub timeout_seconds: u64,
    pub on_error: GuardErrorPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HookRegistrySummary {
    pub hooks: Vec<HookSummary>,
    pub guards: Vec<GuardSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HookTarget {
    pub hook_id: String,
    pub definition_hash: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HookDispatch {
    pub event: HookEvent,
    pub targets: Vec<HookTarget>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HookDeliveryState {
    Pending,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl HookDeliveryState {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "running" => Some(Self::Running),
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HookSummary {
    pub id: String,
    pub event: HookEventKind,
    pub signal: Option<String>,
    pub timeout_seconds: u64,
    pub max_attempts: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HookDeliverySummary {
    pub id: String,
    pub event_id: String,
    pub hook_id: String,
    pub event: HookEventKind,
    pub state: HookDeliveryState,
    pub attempts: u32,
    pub created_at_ms: i64,
    pub next_attempt_at_ms: Option<i64>,
    pub started_at_ms: Option<i64>,
    pub finished_at_ms: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ClaimedHookDelivery {
    pub summary: HookDeliverySummary,
    pub definition_hash: String,
    pub event_body: Vec<u8>,
}
