use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextMode {
    Fresh,
    Fork,
    Handoff,
}

impl ContextMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::Fork => "fork",
            Self::Handoff => "handoff",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "fresh" => Some(Self::Fresh),
            "fork" => Some(Self::Fork),
            "handoff" => Some(Self::Handoff),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeMode {
    NewBranch,
    ExistingBranch,
    Detached,
}

impl WorktreeMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NewBranch => "new_branch",
            Self::ExistingBranch => "existing_branch",
            Self::Detached => "detached",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "new_branch" => Some(Self::NewBranch),
            "existing_branch" => Some(Self::ExistingBranch),
            "detached" => Some(Self::Detached),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceLifecycle {
    Provisioning,
    Starting,
    Ready,
    Completed,
    Failed,
}

impl WorkspaceLifecycle {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Provisioning => "provisioning",
            Self::Starting => "starting",
            Self::Ready => "ready",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "provisioning" => Some(Self::Provisioning),
            "starting" => Some(Self::Starting),
            "ready" => Some(Self::Ready),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

/// The exact thread runtime state reported by Codex App Server.
///
/// Active flags intentionally remain strings so a newer App Server can add a
/// flag without CoCo dropping it while persisting or forwarding the snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CodexThreadStatus {
    NotLoaded,
    Idle,
    SystemError,
    Active {
        #[serde(rename = "activeFlags")]
        active_flags: Vec<String>,
    },
}

impl CodexThreadStatus {
    pub fn canonicalized(self) -> Self {
        match self {
            Self::Active { mut active_flags } => {
                active_flags.sort_unstable();
                active_flags.dedup();
                Self::Active { active_flags }
            }
            status => status,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRuntimeSnapshot {
    pub status: CodexThreadStatus,
    pub runtime_generation: String,
    pub observed_at_ms: i64,
    pub is_fresh: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspacePhase {
    Active,
    WaitingForApproval,
    WaitingForInput,
    Idle,
    NotLoaded,
    SystemError,
    Unavailable,
    Provisioning,
    Starting,
    Completed,
    Failed,
}

impl WorkspacePhase {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::WaitingForApproval => "waiting_for_approval",
            Self::WaitingForInput => "waiting_for_input",
            Self::Idle => "idle",
            Self::NotLoaded => "not_loaded",
            Self::SystemError => "system_error",
            Self::Unavailable => "unavailable",
            Self::Provisioning => "provisioning",
            Self::Starting => "starting",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "active" => Some(Self::Active),
            "waiting_for_approval" => Some(Self::WaitingForApproval),
            "waiting_for_input" => Some(Self::WaitingForInput),
            "idle" => Some(Self::Idle),
            "not_loaded" => Some(Self::NotLoaded),
            "system_error" => Some(Self::SystemError),
            "unavailable" => Some(Self::Unavailable),
            "provisioning" => Some(Self::Provisioning),
            "starting" => Some(Self::Starting),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceWaitReason {
    Approval,
    UserInput,
}

pub fn derive_workspace_runtime(
    lifecycle: WorkspaceLifecycle,
    thread_runtime: Option<&ThreadRuntimeSnapshot>,
    has_active_turn: bool,
) -> (WorkspacePhase, Vec<WorkspaceWaitReason>) {
    let lifecycle_phase = match lifecycle {
        WorkspaceLifecycle::Provisioning => Some(WorkspacePhase::Provisioning),
        WorkspaceLifecycle::Starting => Some(WorkspacePhase::Starting),
        WorkspaceLifecycle::Completed => Some(WorkspacePhase::Completed),
        WorkspaceLifecycle::Failed => Some(WorkspacePhase::Failed),
        WorkspaceLifecycle::Ready => None,
    };
    if let Some(phase) = lifecycle_phase {
        return (phase, Vec::new());
    }

    let Some(snapshot) = thread_runtime.filter(|snapshot| snapshot.is_fresh) else {
        return (WorkspacePhase::Unavailable, Vec::new());
    };
    match &snapshot.status {
        CodexThreadStatus::NotLoaded => (WorkspacePhase::NotLoaded, Vec::new()),
        CodexThreadStatus::Idle if has_active_turn => (WorkspacePhase::Active, Vec::new()),
        CodexThreadStatus::Idle => (WorkspacePhase::Idle, Vec::new()),
        CodexThreadStatus::SystemError => (WorkspacePhase::SystemError, Vec::new()),
        CodexThreadStatus::Active { active_flags } => {
            let approval = active_flags.iter().any(|flag| flag == "waitingOnApproval");
            let user_input = active_flags.iter().any(|flag| flag == "waitingOnUserInput");
            let mut reasons = Vec::with_capacity(usize::from(approval) + usize::from(user_input));
            if approval {
                reasons.push(WorkspaceWaitReason::Approval);
            }
            if user_input {
                reasons.push(WorkspaceWaitReason::UserInput);
            }
            let phase = if approval {
                WorkspacePhase::WaitingForApproval
            } else if user_input {
                WorkspacePhase::WaitingForInput
            } else {
                WorkspacePhase::Active
            };
            (phase, reasons)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnPhase {
    Starting,
    InProgress,
    Completed,
    Failed,
    Interrupted,
}

impl TurnPhase {
    #[cfg(test)]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "starting" => Some(Self::Starting),
            "in_progress" => Some(Self::InProgress),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "interrupted" => Some(Self::Interrupted),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSnapshot {
    pub name: String,
    pub source_path: Option<PathBuf>,
    pub source_hash: String,
    /// Explicit per-thread model requested outside the profile, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_override: Option<String>,
    /// Effective, non-secret worker settings only. Callers must redact before storing.
    pub effective_settings: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexReasoningEffort {
    pub reasoning_effort: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexModel {
    pub id: String,
    /// Exact value accepted by the App Server's thread model override.
    pub model: String,
    pub display_name: String,
    pub description: String,
    pub is_default: bool,
    pub default_reasoning_effort: String,
    pub supported_reasoning_efforts: Vec<CodexReasoningEffort>,
    #[serde(default = "default_model_input_modalities")]
    pub input_modalities: Vec<String>,
    #[serde(default)]
    pub supports_personality: bool,
}

fn default_model_input_modalities() -> Vec<String> {
    vec!["text".to_owned(), "image".to_owned()]
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    pub id: String,
    pub root_path: PathBuf,
    pub git_common_dir: PathBuf,
    pub display_name: String,
    pub is_linked_worktree: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub create_operation_id: Option<String>,
    pub repository_id: String,
    pub name: String,
    pub context_mode: ContextMode,
    pub context: Value,
    pub profile: ProfileSnapshot,
    pub lifecycle: WorkspaceLifecycle,
    pub thread_runtime: Option<ThreadRuntimeSnapshot>,
    /// Derived on every storage read; never persisted as mutable state.
    pub phase: WorkspacePhase,
    /// Derived from the complete native active-flag set.
    pub wait_reasons: Vec<WorkspaceWaitReason>,
    pub worktree_mode: WorktreeMode,
    pub branch_name: Option<String>,
    pub base_sha: Option<String>,
    pub worktree_path: Option<PathBuf>,
    pub codex_thread_id: Option<String>,
    pub parent_thread_id: Option<String>,
    pub active_turn_id: Option<String>,
    pub last_error_code: Option<String>,
    pub last_error_message: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub completed_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Turn {
    pub id: String,
    pub workspace_id: String,
    pub operation_id: Option<String>,
    pub client_message_id: String,
    pub codex_turn_id: Option<String>,
    pub phase: TurnPhase,
    pub requested_at_ms: i64,
    pub started_at_ms: Option<i64>,
    pub completed_at_ms: Option<i64>,
    pub error: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionKind {
    CommandApproval,
    FileChangeApproval,
    UserInput,
}

impl DecisionKind {
    #[cfg(test)]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CommandApproval => "command_approval",
            Self::FileChangeApproval => "file_change_approval",
            Self::UserInput => "user_input",
        }
    }

    #[cfg(test)]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "command_approval" => Some(Self::CommandApproval),
            "file_change_approval" => Some(Self::FileChangeApproval),
            "user_input" => Some(Self::UserInput),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionState {
    Pending,
    Submitted,
    Resolved,
    Orphaned,
}

impl DecisionState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Submitted => "submitted",
            Self::Resolved => "resolved",
            Self::Orphaned => "orphaned",
        }
    }

    #[cfg(test)]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "submitted" => Some(Self::Submitted),
            "resolved" => Some(Self::Resolved),
            "orphaned" => Some(Self::Orphaned),
            _ => None,
        }
    }

    pub const fn is_open(self) -> bool {
        matches!(self, Self::Pending | Self::Submitted)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecisionOption {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecisionQuestion {
    pub id: String,
    pub header: String,
    pub question: String,
    pub options: Vec<DecisionOption>,
    pub allows_other: bool,
    pub is_secret: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecisionFileChange {
    pub path: PathBuf,
    pub kind: String,
    pub diff: String,
    pub diff_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecisionPermission {
    pub access: String,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum DecisionPrompt {
    Approval(Box<DecisionApprovalPrompt>),
    UserInput { questions: Vec<DecisionQuestion> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecisionApprovalPrompt {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_protocol: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grant_root: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_permissions: Vec<DecisionPermission>,
    pub changes: Vec<DecisionFileChange>,
    pub options: Vec<DecisionOption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Decision {
    pub id: String,
    pub workspace_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    pub kind: DecisionKind,
    pub state: DecisionState,
    pub prompt: DecisionPrompt,
    pub received_at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submitted_at_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSource {
    Coco,
    Git,
    Codex,
}

impl EventSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Coco => "coco",
            Self::Git => "git",
            Self::Codex => "codex",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "coco" => Some(Self::Coco),
            "git" => Some(Self::Git),
            "codex" => Some(Self::Codex),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    #[serde(rename = "workspace.created")]
    WorkspaceCreated,
    #[serde(rename = "worktree.created")]
    WorktreeCreated,
    #[serde(rename = "agent.started")]
    AgentStarted,
    #[serde(rename = "message.received")]
    MessageReceived,
    #[serde(rename = "turn.started")]
    TurnStarted,
    #[serde(rename = "plan.updated")]
    PlanUpdated,
    #[serde(rename = "approval.requested")]
    ApprovalRequested,
    #[serde(rename = "approval.resolved")]
    ApprovalResolved,
    #[serde(rename = "decision.requested")]
    DecisionRequested,
    #[serde(rename = "decision.resolved")]
    DecisionResolved,
    #[serde(rename = "thread.status.changed")]
    ThreadStatusChanged,
    #[serde(rename = "context.compacted")]
    ContextCompacted,
    #[serde(rename = "server_request.received")]
    ServerRequestReceived,
    #[serde(rename = "diff.updated")]
    DiffUpdated,
    #[serde(rename = "agent.message.completed")]
    AgentMessageCompleted,
    #[serde(rename = "turn.completed")]
    TurnCompleted,
    #[serde(rename = "agent.failed")]
    AgentFailed,
    #[serde(rename = "workspace.completed")]
    WorkspaceCompleted,
    #[serde(rename = "control.call.started")]
    ControlCallStarted,
    #[serde(rename = "control.call.completed")]
    ControlCallCompleted,
}

impl EventKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WorkspaceCreated => "workspace.created",
            Self::WorktreeCreated => "worktree.created",
            Self::AgentStarted => "agent.started",
            Self::MessageReceived => "message.received",
            Self::TurnStarted => "turn.started",
            Self::PlanUpdated => "plan.updated",
            Self::ApprovalRequested => "approval.requested",
            Self::ApprovalResolved => "approval.resolved",
            Self::DecisionRequested => "decision.requested",
            Self::DecisionResolved => "decision.resolved",
            Self::ThreadStatusChanged => "thread.status.changed",
            Self::ContextCompacted => "context.compacted",
            Self::ServerRequestReceived => "server_request.received",
            Self::DiffUpdated => "diff.updated",
            Self::AgentMessageCompleted => "agent.message.completed",
            Self::TurnCompleted => "turn.completed",
            Self::AgentFailed => "agent.failed",
            Self::WorkspaceCompleted => "workspace.completed",
            Self::ControlCallStarted => "control.call.started",
            Self::ControlCallCompleted => "control.call.completed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "workspace.created" => Some(Self::WorkspaceCreated),
            "worktree.created" => Some(Self::WorktreeCreated),
            "agent.started" => Some(Self::AgentStarted),
            "message.received" => Some(Self::MessageReceived),
            "turn.started" => Some(Self::TurnStarted),
            "plan.updated" => Some(Self::PlanUpdated),
            "approval.requested" => Some(Self::ApprovalRequested),
            "approval.resolved" => Some(Self::ApprovalResolved),
            "decision.requested" => Some(Self::DecisionRequested),
            "decision.resolved" => Some(Self::DecisionResolved),
            "thread.status.changed" => Some(Self::ThreadStatusChanged),
            "context.compacted" => Some(Self::ContextCompacted),
            "server_request.received" => Some(Self::ServerRequestReceived),
            "diff.updated" => Some(Self::DiffUpdated),
            "agent.message.completed" => Some(Self::AgentMessageCompleted),
            "turn.completed" => Some(Self::TurnCompleted),
            "agent.failed" => Some(Self::AgentFailed),
            "workspace.completed" => Some(Self::WorkspaceCompleted),
            "control.call.started" => Some(Self::ControlCallStarted),
            "control.call.completed" => Some(Self::ControlCallCompleted),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedEvent {
    pub sequence: i64,
    pub id: String,
    pub workspace_id: Option<String>,
    pub turn_id: Option<String>,
    pub kind: EventKind,
    pub source: EventSource,
    pub source_method: Option<String>,
    pub occurred_at_ms: Option<i64>,
    pub recorded_at_ms: i64,
    pub payload: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BaseRelation {
    AtBase,
    Descendant,
    Diverged,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitObservation {
    pub observed: bool,
    pub canonical_path: Option<PathBuf>,
    pub branch_name: Option<String>,
    pub head_sha: Option<String>,
    pub base_sha: String,
    pub dirty: bool,
    pub ahead_by: Option<u64>,
    pub behind_by: Option<u64>,
    pub base_relation: BaseRelation,
    pub binding_valid: bool,
    pub untracked_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditOutcome {
    Succeeded,
    Failed,
}

impl AuditOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }

    #[cfg(test)]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Audit {
    pub sequence: i64,
    pub id: String,
    pub source: String,
    pub action: String,
    pub workspace_id: Option<String>,
    pub operation_id: Option<String>,
    pub outcome: AuditOutcome,
    /// Sanitized metadata only; raw prompts, credentials, and environments are forbidden.
    pub details: Value,
    pub occurred_at_ms: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_names_are_stable() {
        assert_eq!(
            serde_json::to_string(&WorkspacePhase::WaitingForApproval).unwrap(),
            "\"waiting_for_approval\""
        );
        assert_eq!(
            serde_json::to_string(&EventKind::AgentMessageCompleted).unwrap(),
            "\"agent.message.completed\""
        );
        assert_eq!(ContextMode::parse("handoff"), Some(ContextMode::Handoff));
        let profile = ProfileSnapshot {
            name: "default".to_owned(),
            source_path: None,
            source_hash: "sha256:test".to_owned(),
            model_override: None,
            effective_settings: serde_json::json!({"networkAccess": false}),
        };
        let wire = serde_json::to_value(profile).unwrap();
        assert_eq!(wire["sourceHash"], "sha256:test");
        assert!(wire.get("effectiveSettings").is_some());
        assert!(wire.get("source_hash").is_none());
    }

    #[test]
    fn derives_runtime_without_losing_native_flags_or_freshness() {
        let snapshot = ThreadRuntimeSnapshot {
            status: CodexThreadStatus::Active {
                active_flags: vec![
                    "waitingOnUserInput".to_owned(),
                    "futureFlag".to_owned(),
                    "waitingOnApproval".to_owned(),
                ],
            },
            runtime_generation: "runtime-1".to_owned(),
            observed_at_ms: 7,
            is_fresh: true,
        };
        assert_eq!(
            derive_workspace_runtime(WorkspaceLifecycle::Ready, Some(&snapshot), true),
            (
                WorkspacePhase::WaitingForApproval,
                vec![
                    WorkspaceWaitReason::Approval,
                    WorkspaceWaitReason::UserInput
                ]
            )
        );
        assert_eq!(
            derive_workspace_runtime(
                WorkspaceLifecycle::Ready,
                Some(&ThreadRuntimeSnapshot {
                    is_fresh: false,
                    ..snapshot
                }),
                true,
            ),
            (WorkspacePhase::Unavailable, Vec::new())
        );
    }
}
