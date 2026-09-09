use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::*;
use crate::protocol::DaemonMethod;

#[derive(Clone, Default)]
struct RecordingDaemon {
    calls: Arc<Mutex<Vec<(String, Value)>>>,
    failures: Arc<Mutex<HashMap<String, DaemonFailure>>>,
}

impl RecordingDaemon {
    fn fail(&self, method: &str, error: DaemonFailure) {
        self.failures
            .lock()
            .unwrap()
            .insert(method.to_owned(), error);
    }

    fn calls(&self) -> Vec<(String, Value)> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl DaemonRpc for RecordingDaemon {
    async fn request<R>(&self, request: R) -> Result<R::Response, DaemonFailure>
    where
        R: DaemonRequest + Send,
        R::Response: Send,
    {
        let method = R::METHOD.as_str();
        let params = serde_json::to_value(request).unwrap();
        self.calls.lock().unwrap().push((method.to_owned(), params));
        if let Some(error) = self.failures.lock().unwrap().get(method).cloned() {
            Err(error)
        } else {
            serde_json::from_value(fake_response(R::METHOD)).map_err(|error| {
                DaemonFailure::new("TEST_RESPONSE", format!("invalid fake response: {error}"))
            })
        }
    }
}

fn fake_response(method: DaemonMethod) -> Value {
    match method {
        DaemonMethod::SignalCatalogLoad
        | DaemonMethod::SignalTypeList
        | DaemonMethod::SignalEmit
        | DaemonMethod::SignalList => unreachable!("signals use the dedicated process tests"),
        DaemonMethod::Health => json!({"status": "ok"}),
        DaemonMethod::ModelList => json!([]),
        DaemonMethod::RepositoryRegister => json!({
            "id": "repo-test",
            "rootPath": "/repo",
            "gitCommonDir": "/repo/.git",
            "displayName": "repo",
            "isLinkedWorktree": false,
            "createdAtMs": 1,
            "updatedAtMs": 1,
        }),
        DaemonMethod::RepositoryResolve => json!({
            "id": "repo-test",
            "rootPath": "/repo",
            "displayName": "repo",
        }),
        DaemonMethod::RepositoryList => json!([]),
        DaemonMethod::WorkspaceCreate => json!({"workspace": fake_workspace()}),
        DaemonMethod::WorkspaceClose => json!({
            "workspace": fake_workspace(),
            "plan": fake_retirement_plan(),
            "applied": true,
        }),
        DaemonMethod::WorkspaceReopen => json!({"workspace": fake_workspace()}),
        DaemonMethod::WorkspaceDelete => json!({
            "plan": fake_retirement_plan(),
            "applied": true,
        }),
        DaemonMethod::WorkspaceList => json!([]),
        DaemonMethod::WorkspaceAttach => json!({
            "workspace": fake_workspace(),
            "launch": {
                "kind": "resume",
                "threadId": "thread-test",
                "leaseId": "lease-test"
            },
        }),
        DaemonMethod::WorkspaceAttachRenew => json!({}),
        DaemonMethod::WorkspaceAttachAdopt => json!({"state": "pending"}),
        DaemonMethod::WorkspaceAttachRelease => json!({}),
        DaemonMethod::WorkspaceGet => json!({
            "workspace": fake_workspace(),
            "git": {"observed": false, "reason": "test fixture"},
            "openDecisions": [],
            "nextSequence": 0,
        }),
        DaemonMethod::TurnStart => json!({
            "workspace": fake_workspace(),
            "turnId": "turn-test",
            "codexTurnId": "codex-turn-test",
        }),
        DaemonMethod::TurnResult => json!({
            "state": "pending",
            "codexTurnId": "codex-turn-test",
        }),
        DaemonMethod::EventList => json!({
            "workspace": fake_workspace(),
            "events": [],
            "openDecisions": [],
            "nextSequence": 0,
        }),
        DaemonMethod::WorkspaceDiff => json!({
            "patch": "",
            "patchTruncated": false,
            "untrackedPaths": [],
        }),
        DaemonMethod::DecisionGet | DaemonMethod::DecisionRespond => json!(null),
        DaemonMethod::AuditRecord => json!({
            "sequence": 1,
            "id": "audit-test",
            "source": "mcp",
            "action": "test",
            "workspaceId": null,
            "operationId": null,
            "outcome": "succeeded",
            "details": {},
            "occurredAtMs": 1,
        }),
    }
}

fn fake_workspace() -> Value {
    json!({
        "id": "workspace-test",
        "createOperationId": "create-test",
        "repositoryId": "repo-test",
        "name": "workspace",
        "contextMode": "fresh",
        "context": {},
        "profile": {
            "name": "default",
            "sourcePath": null,
            "sourceHash": "test",
            "effectiveSettings": {},
        },
        "lifecycle": "ready",
        "availability": "open",
        "threadRuntime": {
            "status": {"type": "idle"},
            "runtimeGeneration": "runtime-test",
            "observedAtMs": 1,
            "isFresh": true,
        },
        "phase": "idle",
        "waitReasons": [],
        "worktreeMode": "new_branch",
        "branchName": "coco/workspace",
        "baseSha": "base-test",
        "worktreePath": "/worktree",
        "codexThreadId": "thread-test",
        "parentThreadId": null,
        "activeTurnId": null,
        "lastErrorCode": null,
        "lastErrorMessage": null,
        "createdAtMs": 1,
        "updatedAtMs": 1,
        "completedAtMs": null,
        "threadArchived": false,
        "closedHeadSha": null,
        "closedAtMs": null,
    })
}

fn fake_retirement_plan() -> Value {
    json!({
        "workspaceId": "workspace-test",
        "workspaceName": "workspace",
        "worktreePath": "/worktree",
        "branchName": "coco/workspace",
        "threadId": "thread-test",
        "threadDisposition": "retain",
        "deleteBranch": false,
        "trackedChanges": false,
        "untrackedFileCount": 0,
        "ignoredFileCount": 0,
        "detachedCommits": false,
        "descendantThreadCount": 0,
    })
}

#[test]
fn workspaces_send_is_only_exposed_when_enabled() {
    let socket = PathBuf::from("/tmp/cocod-test.sock");
    let read_only = McpServer::new(PathBuf::from("/repo"), false, socket.clone(), vec![]);
    let writable = McpServer::new(PathBuf::from("/repo"), true, socket, vec![]);

    let read_only_names: Vec<_> = read_only
        .tool_router
        .list_all()
        .into_iter()
        .map(|tool| tool.name.into_owned())
        .collect();
    assert_eq!(
        read_only_names,
        vec![
            "signals.list",
            "signals.types",
            WORKSPACES_DIFF,
            WORKSPACES_LIST,
            WORKSPACES_STATUS
        ]
    );
    assert!(!read_only.tool_router.has_route(WORKSPACES_SEND));
    assert!(read_only.get_tool(WORKSPACES_SEND).is_none());
    assert!(writable.get_tool(WORKSPACES_SEND).is_some());

    let writable_names: Vec<_> = writable
        .tool_router
        .list_all()
        .into_iter()
        .map(|tool| tool.name.into_owned())
        .collect();
    assert_eq!(
        writable_names,
        vec![
            "signals.list",
            "signals.types",
            WORKSPACES_DIFF,
            WORKSPACES_LIST,
            WORKSPACES_SEND,
            WORKSPACES_STATUS,
        ]
    );
    assert!(writable.tool_router.has_route(WORKSPACES_SEND));
}

#[tokio::test]
async fn maps_all_tools_to_repository_scoped_wire_calls() {
    let daemon = RecordingDaemon::default();
    let dispatcher = Dispatcher::new(daemon.clone(), PathBuf::from("/fixed/repository"));

    dispatcher
        .workspaces_list(WorkspacesListInput {
            phases: Some(vec!["running".into(), "idle".into()]),
        })
        .await;
    dispatcher
        .workspaces_status(WorkspaceStatusInput {
            workspace: "workspace-one".into(),
        })
        .await;
    dispatcher
        .workspaces_diff(WorkspaceDiffInput {
            workspace: "workspace-two".into(),
            max_bytes: Some(4_096),
        })
        .await;
    dispatcher
        .workspaces_send(WorkspaceSendInput {
            workspace: "workspace-three".into(),
            message: "private prompt text".into(),
            operation_id: Some("operation-7".into()),
        })
        .await;

    let calls = daemon.calls();
    assert_eq!(calls.len(), 8);
    assert_eq!(calls[0].0, "workspace.list");
    assert_eq!(
        calls[0].1,
        json!({
            "scope": {"kind": "repository", "path": "/fixed/repository"},
            "phases": ["running", "idle"]
        })
    );
    assert_eq!(calls[2].0, "workspace.get");
    assert_eq!(
        calls[2].1,
        json!({
            "scope": {"kind": "repository", "path": "/fixed/repository"},
            "workspace": "workspace-one"
        })
    );
    assert_eq!(calls[4].0, "workspace.diff");
    assert_eq!(
        calls[4].1,
        json!({
            "scope": {"kind": "repository", "path": "/fixed/repository"},
            "workspace": "workspace-two",
            "maxBytes": 4_096
        })
    );
    assert_eq!(calls[6].0, "turn.start");
    assert_eq!(
        calls[6].1,
        json!({
            "scope": {"kind": "repository", "path": "/fixed/repository"},
            "workspace": "workspace-three",
            "message": "private prompt text",
            "operationId": "operation-7"
        })
    );

    for audit_index in [1, 3, 5, 7] {
        assert_eq!(calls[audit_index].0, "audit.record");
        assert_eq!(
            calls[audit_index].1.pointer("/details/repositoryPath"),
            Some(&json!("/fixed/repository"))
        );
        assert_eq!(
            calls[audit_index].1.get("outcome"),
            Some(&json!("succeeded"))
        );
    }
    assert_eq!(calls[7].1.get("source"), Some(&json!("mcp")));
    assert_eq!(calls[7].1.get("action"), Some(&json!(WORKSPACES_SEND)));
    assert_eq!(
        calls[7].1.get("workspaceId"),
        Some(&json!("workspace-three"))
    );
    assert_eq!(calls[7].1.get("operationId"), Some(&json!("operation-7")));
    assert!(!calls[7].1.to_string().contains("private prompt text"));
}

#[tokio::test]
async fn generates_send_operation_id_and_reuses_it_for_audit() {
    let daemon = RecordingDaemon::default();
    let dispatcher = Dispatcher::new(daemon.clone(), PathBuf::from("/repo"));

    dispatcher
        .workspaces_send(WorkspaceSendInput {
            workspace: "workspace".into(),
            message: "message".into(),
            operation_id: None,
        })
        .await;

    let calls = daemon.calls();
    let operation_id = calls[0].1["operationId"].as_str().unwrap();
    Uuid::parse_str(operation_id).unwrap();
    assert_eq!(calls[1].1["operationId"], operation_id);
    assert!(!calls[1].1.to_string().contains("message"));
}

#[tokio::test]
async fn returns_structured_errors_and_audits_only_the_safe_code() {
    let daemon = RecordingDaemon::default();
    daemon.fail(
        "workspace.get",
        DaemonFailure::new("WORKSPACE_NOT_FOUND", "workspace does not exist"),
    );
    let dispatcher = Dispatcher::new(daemon.clone(), PathBuf::from("/repo"));

    let result = dispatcher
        .workspaces_status(WorkspaceStatusInput {
            workspace: "missing".into(),
        })
        .await;

    assert_eq!(result.is_error, Some(true));
    assert_eq!(
        result.structured_content,
        Some(json!({
            "error": {
                "code": "WORKSPACE_NOT_FOUND",
                "message": "workspace does not exist"
            }
        }))
    );
    assert_eq!(result.content.len(), 1);

    let calls = daemon.calls();
    assert_eq!(calls[1].0, "audit.record");
    assert_eq!(calls[1].1["outcome"], "failed");
    assert_eq!(calls[1].1["details"]["error"], "WORKSPACE_NOT_FOUND");
    assert!(!calls[1].1.to_string().contains("workspace does not exist"));
}

#[tokio::test]
async fn audit_failure_does_not_replace_a_successful_tool_result() {
    let daemon = RecordingDaemon::default();
    daemon.fail(
        "audit.record",
        DaemonFailure::new("INTERNAL", "audit unavailable"),
    );
    let dispatcher = Dispatcher::new(daemon, PathBuf::from("/repo"));

    let result = dispatcher
        .workspaces_list(WorkspacesListInput::default())
        .await;

    assert_eq!(result.is_error, Some(false));
    assert_eq!(result.structured_content, Some(json!([])));
}
