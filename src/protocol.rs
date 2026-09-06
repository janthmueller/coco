use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::{
    Audit, AuditOutcome, ContextMode, Decision, GitObservation, NormalizedEvent, Repository, Turn,
    Workspace,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DaemonMethod {
    Health,
    RepositoryRegister,
    RepositoryList,
    WorkspaceCreate,
    WorkspaceList,
    WorkspaceGet,
    TurnStart,
    EventList,
    WorkspaceDiff,
    DecisionGet,
    DecisionRespond,
    AuditRecord,
}

impl DaemonMethod {
    #[cfg(test)]
    pub const ALL: [Self; 12] = [
        Self::Health,
        Self::RepositoryRegister,
        Self::RepositoryList,
        Self::WorkspaceCreate,
        Self::WorkspaceList,
        Self::WorkspaceGet,
        Self::TurnStart,
        Self::EventList,
        Self::WorkspaceDiff,
        Self::DecisionGet,
        Self::DecisionRespond,
        Self::AuditRecord,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Health => "health",
            Self::RepositoryRegister => "repository.register",
            Self::RepositoryList => "repository.list",
            Self::WorkspaceCreate => "workspace.create",
            Self::WorkspaceList => "workspace.list",
            Self::WorkspaceGet => "workspace.get",
            Self::TurnStart => "turn.start",
            Self::EventList => "event.list",
            Self::WorkspaceDiff => "workspace.diff",
            Self::DecisionGet => "decision.get",
            Self::DecisionRespond => "decision.respond",
            Self::AuditRecord => "audit.record",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "health" => Some(Self::Health),
            "repository.register" => Some(Self::RepositoryRegister),
            "repository.list" => Some(Self::RepositoryList),
            "workspace.create" => Some(Self::WorkspaceCreate),
            "workspace.list" => Some(Self::WorkspaceList),
            "workspace.get" => Some(Self::WorkspaceGet),
            "turn.start" => Some(Self::TurnStart),
            "event.list" => Some(Self::EventList),
            "workspace.diff" => Some(Self::WorkspaceDiff),
            "decision.get" => Some(Self::DecisionGet),
            "decision.respond" => Some(Self::DecisionRespond),
            "audit.record" => Some(Self::AuditRecord),
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepositoryRegisterParams {
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceCreateParams {
    pub repository_path: PathBuf,
    pub name: String,
    pub base_ref: String,
    pub context_mode: ContextMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fork_from: Option<String>,
    #[serde(default)]
    pub compact: bool,
    #[serde(default = "default_profile")]
    pub profile: String,
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
pub struct WorkspaceGetParams {
    pub scope: RepositoryScope,
    pub workspace: String,
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

impl WorkspaceResult {
    pub fn prepared(workspace: Workspace) -> Self {
        Self {
            workspace,
            turn_id: None,
            codex_turn_id: None,
        }
    }

    pub fn with_turn(workspace: Workspace, turn: &Turn) -> Self {
        Self {
            workspace,
            turn_id: Some(turn.id.clone()),
            codex_turn_id: turn.codex_turn_id.clone(),
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
daemon_request!(RepositoryRegisterParams, RepositoryRegister, Repository);
daemon_request!(RepositoryListParams, RepositoryList, Vec<RepositorySummary>);
daemon_request!(WorkspaceCreateParams, WorkspaceCreate, WorkspaceResult);
daemon_request!(WorkspaceListParams, WorkspaceList, Vec<WorkspaceListItem>);
daemon_request!(WorkspaceGetParams, WorkspaceGet, WorkspaceStatusResult);
daemon_request!(TurnStartParams, TurnStart, WorkspaceResult);
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
                "repository.register",
                "repository.list",
                "workspace.create",
                "workspace.list",
                "workspace.get",
                "turn.start",
                "event.list",
                "workspace.diff",
                "decision.get",
                "decision.respond",
                "audit.record",
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
            WorkspaceCreateParams {
                repository_path: PathBuf::from("/repo"),
                name: "workspace".to_owned(),
                base_ref: "HEAD".to_owned(),
                context_mode: ContextMode::Fresh,
                fork_from: None,
                compact: false,
                profile: "dev".to_owned(),
                operation_id: "create-1".to_owned(),
            },
            DaemonMethod::WorkspaceCreate,
            json!({
                "repositoryPath": "/repo",
                "name": "workspace",
                "baseRef": "HEAD",
                "contextMode": "fresh",
                "compact": false,
                "profile": "dev",
                "operationId": "create-1",
            }),
        );
        assert_request(
            WorkspaceCreateParams {
                repository_path: PathBuf::from("/repo"),
                name: "child".to_owned(),
                base_ref: "HEAD".to_owned(),
                context_mode: ContextMode::Fork,
                fork_from: Some("source".to_owned()),
                compact: true,
                profile: "default".to_owned(),
                operation_id: "create-fork".to_owned(),
            },
            DaemonMethod::WorkspaceCreate,
            json!({
                "repositoryPath": "/repo",
                "name": "child",
                "baseRef": "HEAD",
                "contextMode": "fork",
                "forkFrom": "source",
                "compact": true,
                "profile": "default",
                "operationId": "create-fork",
            }),
        );
    }

    #[test]
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
            WorkspaceGetParams {
                scope: RepositoryScope::AllRepositories,
                workspace: "workspace".to_owned(),
            },
            DaemonMethod::WorkspaceGet,
            json!({"scope": {"kind": "allRepositories"}, "workspace": "workspace"}),
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
            "baseRef": "HEAD",
            "contextMode": "fresh",
            "operationId": "create-1",
        }))
        .unwrap();
        assert_eq!(params.profile, "default");
        assert_eq!(params.fork_from, None);
        assert!(!params.compact);
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
        assert_response::<WorkspaceCreateParams>(json!({"workspace": workspace()}));
        let mut listed = workspace();
        listed.as_object_mut().unwrap().insert(
            "repository".to_owned(),
            json!({"id": "repo-1", "displayName": "repo", "rootPath": "/repo"}),
        );
        assert_response::<WorkspaceListParams>(json!([listed]));
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
            "openDecisions": [],
            "nextSequence": 3,
        }));
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
            "threadRuntime": {
                "status": {"type": "idle"},
                "runtimeGeneration": "runtime-1",
                "observedAtMs": 2,
                "isFresh": true,
            },
            "phase": "idle",
            "waitReasons": [],
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
