use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::runtime::WorkspaceRuntimeResources;
use crate::domain::{
    Audit, AuditOutcome, CodexModel, ContextMode, Decision, GitObservation, NormalizedEvent,
    Repository, Workspace,
};

mod hooks;
mod signals;
pub(crate) use hooks::*;
pub(crate) use signals::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DaemonMethod {
    Health,
    ModelList,
    RepositoryRegister,
    RepositoryResolve,
    RepositoryList,
    WorkspaceCreate,
    WorkspaceClose,
    WorkspaceReopen,
    WorkspaceDelete,
    WorkspaceList,
    WorkspaceGet,
    WorkspaceAttach,
    WorkspaceAttachRenew,
    WorkspaceAttachAdopt,
    WorkspaceAttachRelease,
    TurnStart,
    TurnResult,
    EventList,
    WorkspaceDiff,
    DecisionGet,
    DecisionRespond,
    AuditRecord,
    SignalCatalogLoad,
    SignalTypeList,
    SignalEmit,
    SignalList,
    HookList,
    HookReload,
    HookDeliveryList,
}

impl DaemonMethod {
    #[cfg(test)]
    pub const ALL: [Self; 29] = [
        Self::Health,
        Self::ModelList,
        Self::RepositoryRegister,
        Self::RepositoryResolve,
        Self::RepositoryList,
        Self::WorkspaceCreate,
        Self::WorkspaceClose,
        Self::WorkspaceReopen,
        Self::WorkspaceDelete,
        Self::WorkspaceList,
        Self::WorkspaceGet,
        Self::WorkspaceAttach,
        Self::WorkspaceAttachRenew,
        Self::WorkspaceAttachAdopt,
        Self::WorkspaceAttachRelease,
        Self::TurnStart,
        Self::TurnResult,
        Self::EventList,
        Self::WorkspaceDiff,
        Self::DecisionGet,
        Self::DecisionRespond,
        Self::AuditRecord,
        Self::SignalCatalogLoad,
        Self::SignalTypeList,
        Self::SignalEmit,
        Self::SignalList,
        Self::HookList,
        Self::HookReload,
        Self::HookDeliveryList,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Health => "health",
            Self::ModelList => "model.list",
            Self::RepositoryRegister => "repository.register",
            Self::RepositoryResolve => "repository.resolve",
            Self::RepositoryList => "repository.list",
            Self::WorkspaceCreate => "workspace.create",
            Self::WorkspaceClose => "workspace.close",
            Self::WorkspaceReopen => "workspace.reopen",
            Self::WorkspaceDelete => "workspace.delete",
            Self::WorkspaceList => "workspace.list",
            Self::WorkspaceGet => "workspace.get",
            Self::WorkspaceAttach => "workspace.attach",
            Self::WorkspaceAttachRenew => "workspace.attach.renew",
            Self::WorkspaceAttachAdopt => "workspace.attach.adopt",
            Self::WorkspaceAttachRelease => "workspace.attach.release",
            Self::TurnStart => "turn.start",
            Self::TurnResult => "turn.result",
            Self::EventList => "event.list",
            Self::WorkspaceDiff => "workspace.diff",
            Self::DecisionGet => "decision.get",
            Self::DecisionRespond => "decision.respond",
            Self::AuditRecord => "audit.record",
            Self::SignalCatalogLoad => "signal.catalog.load",
            Self::SignalTypeList => "signal.type.list",
            Self::SignalEmit => "signal.emit",
            Self::SignalList => "signal.list",
            Self::HookList => "hook.list",
            Self::HookReload => "hook.reload",
            Self::HookDeliveryList => "hook.delivery.list",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "health" => Some(Self::Health),
            "model.list" => Some(Self::ModelList),
            "repository.register" => Some(Self::RepositoryRegister),
            "repository.resolve" => Some(Self::RepositoryResolve),
            "repository.list" => Some(Self::RepositoryList),
            "workspace.create" => Some(Self::WorkspaceCreate),
            "workspace.close" => Some(Self::WorkspaceClose),
            "workspace.reopen" => Some(Self::WorkspaceReopen),
            "workspace.delete" => Some(Self::WorkspaceDelete),
            "workspace.list" => Some(Self::WorkspaceList),
            "workspace.get" => Some(Self::WorkspaceGet),
            "workspace.attach" => Some(Self::WorkspaceAttach),
            "workspace.attach.renew" => Some(Self::WorkspaceAttachRenew),
            "workspace.attach.adopt" => Some(Self::WorkspaceAttachAdopt),
            "workspace.attach.release" => Some(Self::WorkspaceAttachRelease),
            "turn.start" => Some(Self::TurnStart),
            "turn.result" => Some(Self::TurnResult),
            "event.list" => Some(Self::EventList),
            "workspace.diff" => Some(Self::WorkspaceDiff),
            "decision.get" => Some(Self::DecisionGet),
            "decision.respond" => Some(Self::DecisionRespond),
            "audit.record" => Some(Self::AuditRecord),
            "signal.catalog.load" => Some(Self::SignalCatalogLoad),
            "signal.type.list" => Some(Self::SignalTypeList),
            "signal.emit" => Some(Self::SignalEmit),
            "signal.list" => Some(Self::SignalList),
            "hook.list" => Some(Self::HookList),
            "hook.reload" => Some(Self::HookReload),
            "hook.delivery.list" => Some(Self::HookDeliveryList),
            _ => None,
        }
    }
}

impl std::fmt::Display for DaemonMethod {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

pub trait DaemonRequest: Serialize {
    type Response: Serialize + DeserializeOwned;

    const METHOD: DaemonMethod;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppServerEndpoint {
    pub schema_version: u32,
    pub url: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HealthParams {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelListParams {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepositoryRegisterParams {
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepositoryResolveParams {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryListParams {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum RepositoryScope {
    Repository { path: PathBuf },
    AllRepositories,
}

impl RepositoryScope {
    pub fn repository(path: impl Into<PathBuf>) -> Self {
        Self::Repository { path: path.into() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum WorkspaceBaseRequest {
    Revision { revision: String },
    Workspace { workspace: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum WorkspaceContextSource {
    Reference { reference: String },
    Workspace { workspace: String },
    Thread { thread_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum WorkspaceContextRequest {
    Fresh,
    Fork {
        source: WorkspaceContextSource,
        #[serde(default)]
        compact: bool,
    },
}

impl WorkspaceContextRequest {
    pub const fn mode(&self) -> ContextMode {
        match self {
            Self::Fresh => ContextMode::Fresh,
            Self::Fork { .. } => ContextMode::Fork,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum WorkspaceWorktreeRequest {
    NewBranch {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        base: WorkspaceBaseRequest,
    },
    ExistingBranch {
        branch: String,
    },
    Detached {
        base: WorkspaceBaseRequest,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceChangesRequest {
    Reject,
    CarryTracked,
    CarryTrackedAndUntracked,
}

impl WorkspaceChangesRequest {
    pub const fn carries_tracked(self) -> bool {
        !matches!(self, Self::Reject)
    }

    pub const fn carries_untracked(self) -> bool {
        matches!(self, Self::CarryTrackedAndUntracked)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceCreateParams {
    pub repository_path: PathBuf,
    pub name: String,
    pub context: WorkspaceContextRequest,
    pub worktree: WorkspaceWorktreeRequest,
    pub changes: WorkspaceChangesRequest,
    #[serde(default = "default_profile")]
    pub profile: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub operation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceListParams {
    pub scope: RepositoryScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phases: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceCloseParams {
    pub scope: RepositoryScope,
    pub workspace: String,
    #[serde(default)]
    pub archive_thread: bool,
    #[serde(default)]
    pub discard_changes: bool,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_plan: Option<WorkspaceRetirementPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceReopenParams {
    pub scope: RepositoryScope,
    pub workspace: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceDeleteParams {
    pub scope: RepositoryScope,
    pub workspace: String,
    #[serde(default)]
    pub delete_thread: bool,
    #[serde(default)]
    pub delete_branch: bool,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_plan: Option<WorkspaceRetirementPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceGetParams {
    pub scope: RepositoryScope,
    pub workspace: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceAttachParams {
    pub scope: RepositoryScope,
    pub workspace: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceAttachRenewParams {
    pub workspace_id: String,
    pub lease_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceAttachAdoptParams {
    pub workspace_id: String,
    pub lease_id: String,
    pub thread_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceAttachReleaseParams {
    pub workspace_id: String,
    pub lease_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TurnStartParams {
    pub scope: RepositoryScope,
    pub workspace: String,
    pub message: String,
    pub operation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TurnResultParams {
    pub operation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventListParams {
    pub scope: RepositoryScope,
    pub workspace: String,
    #[serde(default)]
    pub after_sequence: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceDiffParams {
    pub scope: RepositoryScope,
    pub workspace: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecisionGetParams {
    pub decision_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum DecisionSubmission {
    Choice { choice: u32 },
    Answers { answers: BTreeMap<String, String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecisionRespondParams {
    pub decision_id: String,
    pub submission: DecisionSubmission,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuditRecordParams {
    pub source: String,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    pub outcome: AuditOutcome,
    #[serde(default)]
    pub details: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HealthResult {
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceResult {
    pub workspace: Workspace,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex_turn_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TurnTerminalStatus {
    Completed,
    Interrupted,
    Failed,
}

impl TurnTerminalStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Interrupted => "interrupted",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum TurnResult {
    Pending {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        codex_turn_id: Option<String>,
    },
    Finished {
        codex_turn_id: String,
        status: TurnTerminalStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        response: Option<String>,
        response_truncated: bool,
    },
    Unavailable {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceAttachResult {
    pub workspace: Workspace,
    pub launch: WorkspaceAttachLaunch,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_environment: Option<WorkspaceExecutionEnvironment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceExecutionEnvironment {
    pub environment_id: String,
    pub cwd: PathBuf,
    pub runtime_workspace_roots: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum WorkspaceAttachLaunch {
    Resume { thread_id: String, lease_id: String },
    Start { lease_id: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum WorkspaceAttachAdoptResult {
    Pending,
    Bound { workspace: Box<Workspace> },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceAttachRenewResult {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceAttachReleaseResult {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepositorySummary {
    pub id: String,
    pub display_name: String,
    pub root_path: PathBuf,
}

impl From<&Repository> for RepositorySummary {
    fn from(repository: &Repository) -> Self {
        Self {
            id: repository.id.clone(),
            display_name: repository.display_name.clone(),
            root_path: repository.root_path.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceListItem {
    #[serde(flatten)]
    pub workspace: Workspace,
    pub repository: RepositorySummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceThreadDisposition {
    Retain,
    Archive,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceRetirementPlan {
    pub workspace_id: String,
    pub workspace_name: String,
    pub worktree_path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_sha: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    pub thread_disposition: WorkspaceThreadDisposition,
    pub delete_branch: bool,
    pub tracked_changes: bool,
    pub untracked_file_count: usize,
    pub ignored_file_count: usize,
    pub detached_commits: bool,
    pub descendant_thread_count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blockers: Vec<String>,
}

impl WorkspaceRetirementPlan {
    pub const fn has_local_changes(&self) -> bool {
        self.tracked_changes || self.untracked_file_count > 0 || self.ignored_file_count > 0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceCloseResult {
    pub workspace: Workspace,
    pub plan: WorkspaceRetirementPlan,
    pub applied: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceReopenResult {
    pub workspace: Workspace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceDeleteResult {
    pub plan: WorkspaceRetirementPlan,
    pub applied: bool,
}

impl WorkspaceResult {
    pub fn prepared(workspace: Workspace) -> Self {
        Self {
            workspace,
            turn_id: None,
            codex_turn_id: None,
        }
    }

    pub fn with_operation(
        workspace: Workspace,
        operation_id: &str,
        native_result_id: Option<&str>,
    ) -> Self {
        Self {
            workspace,
            turn_id: Some(operation_id.to_owned()),
            codex_turn_id: native_result_id.map(ToOwned::to_owned),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WorkspaceGitStatus {
    Observed(GitObservation),
    Unavailable(GitUnavailable),
    Incomplete(GitIncomplete),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GitUnavailable {
    pub observed: bool,
    pub error: GitObservationError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GitObservationError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GitIncomplete {
    pub observed: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceStatusResult {
    pub workspace: Workspace,
    pub git: WorkspaceGitStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_resources: Option<WorkspaceRuntimeResources>,
    pub open_decisions: Vec<Decision>,
    pub next_sequence: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventListResult {
    pub workspace: Workspace,
    pub events: Vec<NormalizedEvent>,
    pub open_decisions: Vec<Decision>,
    pub next_sequence: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecisionResult {
    pub decision: Decision,
    pub workspace: Workspace,
    pub repository: RepositorySummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceDiffResult {
    pub patch: String,
    pub patch_truncated: bool,
    pub untracked_paths: Vec<PathBuf>,
}

macro_rules! daemon_request {
    ($request:ty, $method:ident, $response:ty) => {
        impl DaemonRequest for $request {
            type Response = $response;

            const METHOD: DaemonMethod = DaemonMethod::$method;
        }
    };
}

daemon_request!(HealthParams, Health, HealthResult);
daemon_request!(ModelListParams, ModelList, Vec<CodexModel>);
daemon_request!(RepositoryRegisterParams, RepositoryRegister, Repository);
daemon_request!(
    RepositoryResolveParams,
    RepositoryResolve,
    RepositorySummary
);
daemon_request!(RepositoryListParams, RepositoryList, Vec<RepositorySummary>);
daemon_request!(WorkspaceCreateParams, WorkspaceCreate, WorkspaceResult);
daemon_request!(WorkspaceCloseParams, WorkspaceClose, WorkspaceCloseResult);
daemon_request!(
    WorkspaceReopenParams,
    WorkspaceReopen,
    WorkspaceReopenResult
);
daemon_request!(
    WorkspaceDeleteParams,
    WorkspaceDelete,
    WorkspaceDeleteResult
);
daemon_request!(WorkspaceListParams, WorkspaceList, Vec<WorkspaceListItem>);
daemon_request!(WorkspaceGetParams, WorkspaceGet, WorkspaceStatusResult);
daemon_request!(
    WorkspaceAttachParams,
    WorkspaceAttach,
    WorkspaceAttachResult
);
daemon_request!(
    WorkspaceAttachRenewParams,
    WorkspaceAttachRenew,
    WorkspaceAttachRenewResult
);
daemon_request!(
    WorkspaceAttachAdoptParams,
    WorkspaceAttachAdopt,
    WorkspaceAttachAdoptResult
);
daemon_request!(
    WorkspaceAttachReleaseParams,
    WorkspaceAttachRelease,
    WorkspaceAttachReleaseResult
);
daemon_request!(TurnStartParams, TurnStart, WorkspaceResult);
daemon_request!(TurnResultParams, TurnResult, TurnResult);
daemon_request!(EventListParams, EventList, EventListResult);
daemon_request!(WorkspaceDiffParams, WorkspaceDiff, WorkspaceDiffResult);
daemon_request!(DecisionGetParams, DecisionGet, DecisionResult);
daemon_request!(DecisionRespondParams, DecisionRespond, DecisionResult);
daemon_request!(AuditRecordParams, AuditRecord, Audit);

fn default_profile() -> String {
    "default".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn method_registry_is_closed_and_round_trips_every_wire_name() {
        let names = DaemonMethod::ALL.map(DaemonMethod::as_str);
        assert_eq!(
            names,
            [
                "health",
                "model.list",
                "repository.register",
                "repository.resolve",
                "repository.list",
                "workspace.create",
                "workspace.close",
                "workspace.reopen",
                "workspace.delete",
                "workspace.list",
                "workspace.get",
                "workspace.attach",
                "workspace.attach.renew",
                "workspace.attach.adopt",
                "workspace.attach.release",
                "turn.start",
                "turn.result",
                "event.list",
                "workspace.diff",
                "decision.get",
                "decision.respond",
                "audit.record",
                "signal.catalog.load",
                "signal.type.list",
                "signal.emit",
                "signal.list",
                "hook.list",
                "hook.reload",
                "hook.delivery.list",
            ]
        );
        for method in DaemonMethod::ALL {
            assert_eq!(DaemonMethod::parse(method.as_str()), Some(method));
            assert_eq!(method.to_string(), method.as_str());
        }
        assert_eq!(DaemonMethod::parse("workspace.unknown"), None);
    }

    #[test]
    fn request_dtos_preserve_all_wire_field_names_and_defaults() {
        assert_request(HealthParams {}, DaemonMethod::Health, json!({}));
        assert_request(ModelListParams {}, DaemonMethod::ModelList, json!({}));
        assert_request(HookListParams {}, DaemonMethod::HookList, json!({}));
        assert_request(HookReloadParams {}, DaemonMethod::HookReload, json!({}));
        assert_request(
            HookDeliveryListParams { limit: 20 },
            DaemonMethod::HookDeliveryList,
            json!({"limit": 20}),
        );
        assert_request(
            RepositoryRegisterParams {
                path: PathBuf::from("/repo"),
            },
            DaemonMethod::RepositoryRegister,
            json!({"path": "/repo"}),
        );
        assert_request(
            RepositoryListParams {},
            DaemonMethod::RepositoryList,
            json!({}),
        );
        assert_request(
            RepositoryResolveParams {
                path: PathBuf::from("/repo/worktree"),
            },
            DaemonMethod::RepositoryResolve,
            json!({"path": "/repo/worktree"}),
        );
        assert_request(
            WorkspaceCreateParams {
                repository_path: PathBuf::from("/repo"),
                name: "workspace".to_owned(),
                context: WorkspaceContextRequest::Fresh,
                worktree: WorkspaceWorktreeRequest::NewBranch {
                    branch: None,
                    base: WorkspaceBaseRequest::Revision {
                        revision: "HEAD".to_owned(),
                    },
                },
                changes: WorkspaceChangesRequest::Reject,
                profile: "dev".to_owned(),
                model: Some("gpt-explicit".to_owned()),
                operation_id: "create-1".to_owned(),
            },
            DaemonMethod::WorkspaceCreate,
            json!({
                "repositoryPath": "/repo",
                "name": "workspace",
                "context": {"kind": "fresh"},
                "worktree": {
                    "kind": "newBranch",
                    "base": {"kind": "revision", "revision": "HEAD"}
                },
                "changes": "reject",
                "profile": "dev",
                "model": "gpt-explicit",
                "operationId": "create-1",
            }),
        );
        assert_request(
            WorkspaceCreateParams {
                repository_path: PathBuf::from("/repo"),
                name: "child".to_owned(),
                context: WorkspaceContextRequest::Fork {
                    source: WorkspaceContextSource::Reference {
                        reference: "thread-source".to_owned(),
                    },
                    compact: true,
                },
                worktree: WorkspaceWorktreeRequest::Detached {
                    base: WorkspaceBaseRequest::Workspace {
                        workspace: "source".to_owned(),
                    },
                },
                changes: WorkspaceChangesRequest::CarryTrackedAndUntracked,
                profile: "default".to_owned(),
                model: None,
                operation_id: "create-fork".to_owned(),
            },
            DaemonMethod::WorkspaceCreate,
            json!({
                "repositoryPath": "/repo",
                "name": "child",
                "context": {
                    "kind": "fork",
                    "source": {"kind": "reference", "reference": "thread-source"},
                    "compact": true
                },
                "worktree": {
                    "kind": "detached",
                    "base": {"kind": "workspace", "workspace": "source"}
                },
                "changes": "carryTrackedAndUntracked",
                "profile": "default",
                "operationId": "create-fork",
            }),
        );
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "one exhaustive assertion keeps every scoped request wire shape together"
    )]
    fn scoped_request_dtos_preserve_all_wire_field_names() {
        assert_request(
            WorkspaceListParams {
                scope: RepositoryScope::repository("/repo"),
                phases: None,
            },
            DaemonMethod::WorkspaceList,
            json!({"scope": {"kind": "repository", "path": "/repo"}}),
        );
        assert_request(
            WorkspaceCloseParams {
                scope: RepositoryScope::repository("/repo"),
                workspace: "workspace".to_owned(),
                archive_thread: true,
                discard_changes: true,
                dry_run: true,
                expected_plan: None,
            },
            DaemonMethod::WorkspaceClose,
            json!({
                "scope": {"kind": "repository", "path": "/repo"},
                "workspace": "workspace",
                "archiveThread": true,
                "discardChanges": true,
                "dryRun": true,
            }),
        );
        assert_request(
            WorkspaceReopenParams {
                scope: RepositoryScope::repository("/repo"),
                workspace: "workspace".to_owned(),
            },
            DaemonMethod::WorkspaceReopen,
            json!({
                "scope": {"kind": "repository", "path": "/repo"},
                "workspace": "workspace",
            }),
        );
        assert_request(
            WorkspaceDeleteParams {
                scope: RepositoryScope::AllRepositories,
                workspace: "workspace".to_owned(),
                delete_thread: true,
                delete_branch: true,
                dry_run: false,
                expected_plan: None,
            },
            DaemonMethod::WorkspaceDelete,
            json!({
                "scope": {"kind": "allRepositories"},
                "workspace": "workspace",
                "deleteThread": true,
                "deleteBranch": true,
                "dryRun": false,
            }),
        );
        assert_request(
            WorkspaceGetParams {
                scope: RepositoryScope::AllRepositories,
                workspace: "workspace".to_owned(),
            },
            DaemonMethod::WorkspaceGet,
            json!({"scope": {"kind": "allRepositories"}, "workspace": "workspace"}),
        );
        assert_request(
            TurnResultParams {
                operation_id: "send-1".to_owned(),
            },
            DaemonMethod::TurnResult,
            json!({"operationId": "send-1"}),
        );
        assert_request(
            WorkspaceAttachParams {
                scope: RepositoryScope::repository("/repo"),
                workspace: "workspace".to_owned(),
            },
            DaemonMethod::WorkspaceAttach,
            json!({
                "scope": {"kind": "repository", "path": "/repo"},
                "workspace": "workspace",
            }),
        );
        assert_request(
            WorkspaceAttachRenewParams {
                workspace_id: "workspace-1".to_owned(),
                lease_id: "lease-1".to_owned(),
            },
            DaemonMethod::WorkspaceAttachRenew,
            json!({"workspaceId": "workspace-1", "leaseId": "lease-1"}),
        );
        assert_request(
            WorkspaceAttachAdoptParams {
                workspace_id: "workspace-1".to_owned(),
                lease_id: "lease-1".to_owned(),
                thread_id: "thread-1".to_owned(),
            },
            DaemonMethod::WorkspaceAttachAdopt,
            json!({
                "workspaceId": "workspace-1",
                "leaseId": "lease-1",
                "threadId": "thread-1",
            }),
        );
        assert_request(
            WorkspaceAttachReleaseParams {
                workspace_id: "workspace-1".to_owned(),
                lease_id: "lease-1".to_owned(),
            },
            DaemonMethod::WorkspaceAttachRelease,
            json!({"workspaceId": "workspace-1", "leaseId": "lease-1"}),
        );
        assert_request(
            TurnStartParams {
                scope: RepositoryScope::repository("/repo"),
                workspace: "workspace".to_owned(),
                message: "continue".to_owned(),
                operation_id: "send-1".to_owned(),
            },
            DaemonMethod::TurnStart,
            json!({
                "scope": {"kind": "repository", "path": "/repo"},
                "workspace": "workspace",
                "message": "continue",
                "operationId": "send-1",
            }),
        );
        assert_request(
            EventListParams {
                scope: RepositoryScope::repository("/repo"),
                workspace: "workspace".to_owned(),
                after_sequence: 7,
            },
            DaemonMethod::EventList,
            json!({
                "scope": {"kind": "repository", "path": "/repo"},
                "workspace": "workspace",
                "afterSequence": 7,
            }),
        );
        assert_request(
            WorkspaceDiffParams {
                scope: RepositoryScope::repository("/repo"),
                workspace: "workspace".to_owned(),
                max_bytes: Some(4096),
            },
            DaemonMethod::WorkspaceDiff,
            json!({
                "scope": {"kind": "repository", "path": "/repo"},
                "workspace": "workspace",
                "maxBytes": 4096,
            }),
        );
        assert_request(
            DecisionGetParams {
                decision_id: "decision-1".to_owned(),
            },
            DaemonMethod::DecisionGet,
            json!({"decisionId": "decision-1"}),
        );
        assert_request(
            DecisionRespondParams {
                decision_id: "decision-1".to_owned(),
                submission: DecisionSubmission::Choice { choice: 2 },
            },
            DaemonMethod::DecisionRespond,
            json!({
                "decisionId": "decision-1",
                "submission": {"type": "choice", "choice": 2},
            }),
        );
        assert_request(
            AuditRecordParams {
                source: "mcp".to_owned(),
                action: "workspaces.list".to_owned(),
                workspace_id: None,
                operation_id: Some("operation-1".to_owned()),
                outcome: AuditOutcome::Succeeded,
                details: json!({"repositoryPath": "/repo"}),
            },
            DaemonMethod::AuditRecord,
            json!({
                "source": "mcp",
                "action": "workspaces.list",
                "operationId": "operation-1",
                "outcome": "succeeded",
                "details": {"repositoryPath": "/repo"},
            }),
        );
    }

    #[test]
    fn workspace_creation_defaults_to_the_default_profile() {
        let params: WorkspaceCreateParams = serde_json::from_value(json!({
            "repositoryPath": "/repo",
            "name": "workspace",
            "context": {"kind": "fresh"},
            "worktree": {
                "kind": "newBranch",
                "base": {"kind": "revision", "revision": "HEAD"}
            },
            "changes": "reject",
            "operationId": "create-1",
        }))
        .unwrap();
        assert_eq!(params.profile, "default");
        assert_eq!(params.model, None);
        assert_eq!(params.context, WorkspaceContextRequest::Fresh);
    }

    #[test]
    fn event_listing_defaults_the_cursor() {
        let params: EventListParams = serde_json::from_value(json!({
            "scope": {"kind": "repository", "path": "/repo"},
            "workspace": "workspace",
        }))
        .unwrap();

        assert_eq!(params.after_sequence, 0);
    }

    #[test]
    fn request_dtos_reject_unknown_fields() {
        let error = serde_json::from_value::<WorkspaceGetParams>(json!({
            "repositoryPath": "/repo",
            "workspace": "workspace",
            "goal": "retired",
        }))
        .unwrap_err();
        assert!(error.to_string().contains("unknown field `goal`"));
    }

    #[test]
    fn model_response_preserves_catalog_fields() {
        assert_response::<ModelListParams>(json!([{
            "id": "gpt-test",
            "model": "gpt-test",
            "displayName": "GPT Test",
            "description": "Test model",
            "isDefault": true,
            "defaultReasoningEffort": "medium",
            "supportedReasoningEfforts": [{
                "reasoningEffort": "medium",
                "description": "Balanced",
            }],
            "inputModalities": ["text", "image"],
            "supportsPersonality": true,
        }]));
    }

    #[test]
    fn turn_result_preserves_exact_output_correlation_fields() {
        assert_response::<TurnResultParams>(json!({
            "state": "finished",
            "codexTurnId": "codex-turn-1",
            "status": "completed",
            "response": "Done",
            "responseTruncated": false,
        }));
    }

    #[test]
    fn response_dtos_preserve_every_wrapper_wire_field() {
        assert_response::<HealthParams>(json!({"status": "ok"}));
        assert_response::<RepositoryRegisterParams>(json!({
            "id": "repo-1",
            "rootPath": "/repo",
            "gitCommonDir": "/repo/.git",
            "displayName": "repo",
            "isLinkedWorktree": false,
            "createdAtMs": 1,
            "updatedAtMs": 2,
        }));
        assert_response::<RepositoryListParams>(json!([{
            "id": "repo-1",
            "rootPath": "/repo",
            "displayName": "repo",
        }]));
        assert_response::<RepositoryResolveParams>(json!({
            "id": "repo-1",
            "rootPath": "/repo",
            "displayName": "repo",
        }));
        assert_response::<WorkspaceCreateParams>(json!({"workspace": workspace()}));
        let mut listed = workspace();
        listed.as_object_mut().unwrap().insert(
            "repository".to_owned(),
            json!({"id": "repo-1", "displayName": "repo", "rootPath": "/repo"}),
        );
        assert_response::<WorkspaceListParams>(json!([listed]));
        assert_workspace_status_response();
        assert_attach_responses();
        assert_response::<TurnStartParams>(json!({
            "workspace": workspace(),
            "turnId": "turn-1",
            "codexTurnId": "codex-turn-1",
        }));
        assert_response::<EventListParams>(json!({
            "workspace": workspace(),
            "events": [{
                "sequence": 3,
                "id": "event-3",
                "workspaceId": "workspace-1",
                "turnId": null,
                "kind": "agent.started",
                "source": "codex",
                "sourceMethod": "thread/start",
                "occurredAtMs": null,
                "recordedAtMs": 3,
                "payload": {"threadId": "thread-1"},
            }],
            "openDecisions": [],
            "nextSequence": 3,
        }));
        assert_response::<WorkspaceDiffParams>(json!({
            "patch": "diff --git a/a b/a\n",
            "patchTruncated": false,
            "untrackedPaths": ["new.txt"],
        }));
        let decision_result = json!({
            "decision": decision(),
            "workspace": workspace(),
            "repository": {"id": "repo-1", "displayName": "repo", "rootPath": "/repo"},
        });
        assert_response::<DecisionGetParams>(decision_result.clone());
        assert_response::<DecisionRespondParams>(decision_result);
        assert_response::<AuditRecordParams>(json!({
            "sequence": 4,
            "id": "audit-4",
            "source": "mcp",
            "action": "workspaces.list",
            "workspaceId": null,
            "operationId": null,
            "outcome": "succeeded",
            "details": {"repositoryPath": "/repo"},
            "occurredAtMs": 4,
        }));
        let endpoint = AppServerEndpoint {
            schema_version: 1,
            url: "ws://127.0.0.1:45123".to_owned(),
        };
        assert_eq!(
            serde_json::to_value(endpoint).unwrap(),
            json!({"schemaVersion": 1, "url": "ws://127.0.0.1:45123"})
        );
    }

    #[test]
    fn hook_response_dtos_preserve_every_wire_field() {
        let registry = json!({
            "hooks": [{
                "id": "review-notify",
                "event": "signal.emitted",
                "signal": "review.requested@1",
                "timeoutSeconds": 30,
                "maxAttempts": 3,
            }],
            "guards": [{
                "id": "protect-delete",
                "action": "workspace.delete",
                "timeoutSeconds": 5,
                "onError": "deny",
            }],
        });
        assert_response::<HookListParams>(registry.clone());
        assert_response::<HookReloadParams>(registry);
        assert_response::<HookDeliveryListParams>(json!([{
            "id": "delivery-1",
            "eventId": "event-1",
            "hookId": "review-notify",
            "event": "signal.emitted",
            "state": "succeeded",
            "attempts": 1,
            "createdAtMs": 1,
            "nextAttemptAtMs": null,
            "startedAtMs": 2,
            "finishedAtMs": 3,
            "lastError": null,
        }]));
    }

    fn assert_attach_responses() {
        assert_response::<WorkspaceAttachParams>(json!({
            "workspace": workspace(),
            "launch": {
                "kind": "resume",
                "threadId": "thread-1",
                "leaseId": "lease-1"
            },
            "executionEnvironment": {
                "environmentId": "coco-workspace",
                "cwd": "/worktrees/workspace",
                "runtimeWorkspaceRoots": ["/worktrees/workspace"]
            },
        }));
        assert_response::<WorkspaceAttachRenewParams>(json!({}));
        assert_response::<WorkspaceAttachAdoptParams>(json!({"state": "pending"}));
        assert_response::<WorkspaceAttachAdoptParams>(json!({
            "state": "bound",
            "workspace": workspace(),
        }));
        assert_response::<WorkspaceAttachReleaseParams>(json!({}));
    }

    fn assert_workspace_status_response() {
        assert_response::<WorkspaceGetParams>(json!({
            "workspace": workspace(),
            "git": {
                "observed": true,
                "canonicalPath": "/worktree",
                "branchName": "coco/workspace",
                "headSha": "head",
                "baseSha": "base",
                "dirty": true,
                "aheadBy": 1,
                "behindBy": 0,
                "baseRelation": "descendant",
                "bindingValid": true,
                "untrackedPaths": ["new.txt"],
            },
            "runtimeResources": {
                "backend": "exec_server",
                "state": "running",
                "scope": "process_tree",
                "processId": 42,
                "processCount": 3,
                "residentMemoryBytes": 25165824,
                "cpuPercent": 12.5,
                "sampledAtMs": 4
            },
            "openDecisions": [],
            "nextSequence": 3,
        }));
    }

    fn assert_request<R>(request: R, expected_method: DaemonMethod, expected_params: Value)
    where
        R: DaemonRequest,
    {
        assert_eq!(R::METHOD, expected_method);
        assert_eq!(serde_json::to_value(request).unwrap(), expected_params);
    }

    fn assert_response<R>(expected: Value)
    where
        R: DaemonRequest,
    {
        let decoded: R::Response = serde_json::from_value(expected.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), expected);
    }

    fn workspace() -> Value {
        json!({
            "id": "workspace-1",
            "createOperationId": "create-1",
            "repositoryId": "repo-1",
            "name": "workspace",
            "contextMode": "fresh",
            "context": {"version": 1, "mode": "fresh", "baseRef": "HEAD"},
            "profile": {
                "name": "default",
                "sourcePath": null,
                "sourceHash": "profile-hash",
                "effectiveSettings": {},
            },
            "lifecycle": "ready",
            "availability": "open",
            "threadRuntime": {
                "status": {"type": "idle"},
                "runtimeGeneration": "runtime-1",
                "observedAtMs": 2,
                "isFresh": true,
            },
            "phase": "idle",
            "waitReasons": [],
            "worktreeMode": "new_branch",
            "branchName": "coco/workspace",
            "baseSha": "base",
            "worktreePath": "/worktree",
            "codexThreadId": "thread-1",
            "parentThreadId": null,
            "activeTurnId": null,
            "lastErrorCode": null,
            "lastErrorMessage": null,
            "createdAtMs": 1,
            "updatedAtMs": 2,
            "completedAtMs": null,
            "threadArchived": false,
            "closedHeadSha": null,
            "closedAtMs": null,
        })
    }

    fn decision() -> Value {
        json!({
            "id": "decision-1",
            "workspaceId": "workspace-1",
            "turnId": "turn-1",
            "kind": "command_approval",
            "state": "pending",
            "prompt": {
                "type": "approval",
                "title": "Run command",
                "command": "git status",
                "changes": [],
                "options": [{"label": "Approve once"}],
            },
            "receivedAtMs": 3,
        })
    }
}
