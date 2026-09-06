use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::{
    Audit, AuditOutcome, ContextMode, GitObservation, NormalizedEvent, Repository, Task, Turn,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DaemonMethod {
    Health,
    RepositoryRegister,
    TaskCreate,
    TaskList,
    TaskGet,
    TurnStart,
    EventList,
    TaskDiff,
    AuditRecord,
}

impl DaemonMethod {
    #[cfg(test)]
    pub const ALL: [Self; 9] = [
        Self::Health,
        Self::RepositoryRegister,
        Self::TaskCreate,
        Self::TaskList,
        Self::TaskGet,
        Self::TurnStart,
        Self::EventList,
        Self::TaskDiff,
        Self::AuditRecord,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Health => "health",
            Self::RepositoryRegister => "repository.register",
            Self::TaskCreate => "task.create",
            Self::TaskList => "task.list",
            Self::TaskGet => "task.get",
            Self::TurnStart => "turn.start",
            Self::EventList => "event.list",
            Self::TaskDiff => "task.diff",
            Self::AuditRecord => "audit.record",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "health" => Some(Self::Health),
            "repository.register" => Some(Self::RepositoryRegister),
            "task.create" => Some(Self::TaskCreate),
            "task.list" => Some(Self::TaskList),
            "task.get" => Some(Self::TaskGet),
            "turn.start" => Some(Self::TurnStart),
            "event.list" => Some(Self::EventList),
            "task.diff" => Some(Self::TaskDiff),
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskCreateParams {
    pub repository_path: PathBuf,
    pub name: String,
    pub base_ref: String,
    pub context_mode: ContextMode,
    #[serde(default = "default_profile")]
    pub profile: String,
    pub operation_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskListParams {
    pub repository_path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phases: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskGetParams {
    pub repository_path: PathBuf,
    pub task: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TurnStartParams {
    pub repository_path: PathBuf,
    pub task: String,
    pub message: String,
    pub operation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventListParams {
    pub repository_path: PathBuf,
    pub task: String,
    #[serde(default)]
    pub after_sequence: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskDiffParams {
    pub repository_path: PathBuf,
    pub task: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuditRecordParams {
    pub source: String,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
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
pub struct TaskResult {
    pub task: Task,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex_turn_id: Option<String>,
}

impl TaskResult {
    pub fn prepared(task: Task) -> Self {
        Self {
            task,
            turn_id: None,
            codex_turn_id: None,
        }
    }

    pub fn with_turn(task: Task, turn: &Turn) -> Self {
        Self {
            task,
            turn_id: Some(turn.id.clone()),
            codex_turn_id: turn.codex_turn_id.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TaskGitStatus {
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
pub struct TaskStatusResult {
    pub task: Task,
    pub git: TaskGitStatus,
    pub next_sequence: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventListResult {
    pub task: Task,
    pub events: Vec<NormalizedEvent>,
    pub next_sequence: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskDiffResult {
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
daemon_request!(TaskCreateParams, TaskCreate, TaskResult);
daemon_request!(TaskListParams, TaskList, Vec<Task>);
daemon_request!(TaskGetParams, TaskGet, TaskStatusResult);
daemon_request!(TurnStartParams, TurnStart, TaskResult);
daemon_request!(EventListParams, EventList, EventListResult);
daemon_request!(TaskDiffParams, TaskDiff, TaskDiffResult);
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
                "task.create",
                "task.list",
                "task.get",
                "turn.start",
                "event.list",
                "task.diff",
                "audit.record",
            ]
        );
        for method in DaemonMethod::ALL {
            assert_eq!(DaemonMethod::parse(method.as_str()), Some(method));
            assert_eq!(method.to_string(), method.as_str());
        }
        assert_eq!(DaemonMethod::parse("task.unknown"), None);
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
            TaskCreateParams {
                repository_path: PathBuf::from("/repo"),
                name: "task".to_owned(),
                base_ref: "HEAD".to_owned(),
                context_mode: ContextMode::Fresh,
                profile: "dev".to_owned(),
                operation_id: "create-1".to_owned(),
            },
            DaemonMethod::TaskCreate,
            json!({
                "repositoryPath": "/repo",
                "name": "task",
                "baseRef": "HEAD",
                "contextMode": "fresh",
                "profile": "dev",
                "operationId": "create-1",
            }),
        );
        assert_request(
            TaskListParams {
                repository_path: PathBuf::from("/repo"),
                phases: None,
            },
            DaemonMethod::TaskList,
            json!({"repositoryPath": "/repo"}),
        );
        assert_request(
            TaskGetParams {
                repository_path: PathBuf::from("/repo"),
                task: "task".to_owned(),
            },
            DaemonMethod::TaskGet,
            json!({"repositoryPath": "/repo", "task": "task"}),
        );
        assert_request(
            TurnStartParams {
                repository_path: PathBuf::from("/repo"),
                task: "task".to_owned(),
                message: "continue".to_owned(),
                operation_id: "send-1".to_owned(),
            },
            DaemonMethod::TurnStart,
            json!({
                "repositoryPath": "/repo",
                "task": "task",
                "message": "continue",
                "operationId": "send-1",
            }),
        );
        assert_request(
            EventListParams {
                repository_path: PathBuf::from("/repo"),
                task: "task".to_owned(),
                after_sequence: 7,
            },
            DaemonMethod::EventList,
            json!({"repositoryPath": "/repo", "task": "task", "afterSequence": 7}),
        );
        assert_request(
            TaskDiffParams {
                repository_path: PathBuf::from("/repo"),
                task: "task".to_owned(),
                max_bytes: Some(4096),
            },
            DaemonMethod::TaskDiff,
            json!({"repositoryPath": "/repo", "task": "task", "maxBytes": 4096}),
        );
        assert_request(
            AuditRecordParams {
                source: "mcp".to_owned(),
                action: "tasks.list".to_owned(),
                task_id: None,
                operation_id: Some("operation-1".to_owned()),
                outcome: AuditOutcome::Succeeded,
                details: json!({"repositoryPath": "/repo"}),
            },
            DaemonMethod::AuditRecord,
            json!({
                "source": "mcp",
                "action": "tasks.list",
                "operationId": "operation-1",
                "outcome": "succeeded",
                "details": {"repositoryPath": "/repo"},
            }),
        );
    }

    #[test]
    fn task_creation_defaults_to_the_default_profile() {
        let params: TaskCreateParams = serde_json::from_value(json!({
            "repositoryPath": "/repo",
            "name": "task",
            "baseRef": "HEAD",
            "contextMode": "fresh",
            "operationId": "create-1",
        }))
        .unwrap();
        assert_eq!(params.profile, "default");
    }

    #[test]
    fn request_dtos_reject_unknown_fields() {
        let error = serde_json::from_value::<TaskGetParams>(json!({
            "repositoryPath": "/repo",
            "task": "task",
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
        assert_response::<TaskCreateParams>(json!({"task": task()}));
        assert_response::<TaskListParams>(json!([task()]));
        assert_response::<TaskGetParams>(json!({
            "task": task(),
            "git": {
                "observed": true,
                "canonicalPath": "/worktree",
                "branchName": "coco/task",
                "headSha": "head",
                "baseSha": "base",
                "dirty": true,
                "aheadBy": 1,
                "behindBy": 0,
                "baseRelation": "descendant",
                "bindingValid": true,
                "untrackedPaths": ["new.txt"],
            },
            "nextSequence": 3,
        }));
        assert_response::<TurnStartParams>(json!({
            "task": task(),
            "turnId": "turn-1",
            "codexTurnId": "codex-turn-1",
        }));
        assert_response::<EventListParams>(json!({
            "task": task(),
            "events": [{
                "sequence": 3,
                "id": "event-3",
                "taskId": "task-1",
                "turnId": null,
                "kind": "agent.started",
                "source": "codex",
                "sourceMethod": "thread/start",
                "occurredAtMs": null,
                "recordedAtMs": 3,
                "payload": {"threadId": "thread-1"},
            }],
            "nextSequence": 3,
        }));
        assert_response::<TaskDiffParams>(json!({
            "patch": "diff --git a/a b/a\n",
            "patchTruncated": false,
            "untrackedPaths": ["new.txt"],
        }));
        assert_response::<AuditRecordParams>(json!({
            "sequence": 4,
            "id": "audit-4",
            "source": "mcp",
            "action": "tasks.list",
            "taskId": null,
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

    fn task() -> Value {
        json!({
            "id": "task-1",
            "createOperationId": "create-1",
            "repositoryId": "repo-1",
            "name": "task",
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
            "branchName": "coco/task",
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
}
