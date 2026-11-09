use super::*;
use crate::mcp::tests::RecordingDaemon;

fn input() -> Value {
    json!({"operationId": "setup-key", "name": "feat/work", "worktree": {
        "kind": "newBranch", "base": {"kind": "revision", "revision": "HEAD"}
    }, "changes": "reject", "profile": "default"})
}

#[test]
fn create_grant_is_independent_of_all_existing_mutation_grants() {
    for create in [false, true] {
        for send in [false, true] {
            for interrupt in [false, true] {
                let server = McpServer::new(
                    "/repo".into(),
                    create,
                    send,
                    interrupt,
                    "/tmp/unused-setup.sock".into(),
                    vec![],
                );
                assert_eq!(server.get_tool("workspaces.create").is_some(), create);
                assert_eq!(server.tool_router.has_route("workspaces.create"), create);
                assert!(server.get_tool("workspaceCreations.get").is_some());
                assert_eq!(server.get_tool("workspaces.send").is_some(), send);
                assert_eq!(server.get_tool("turns.interrupt").is_some(), interrupt);
            }
        }
    }
}

#[test]
fn create_schema_requires_explicit_policies_and_cannot_override_scope_or_context() {
    let value = input();
    assert!(serde_json::from_value::<CreateInput>(value.clone()).is_ok());
    for field in ["operationId", "name", "worktree", "changes", "profile"] {
        let mut invalid = value.clone();
        invalid.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<CreateInput>(invalid).is_err(),
            "{field}"
        );
    }
    for field in ["repositoryPath", "context", "temporary", "threadId"] {
        let mut invalid = value.clone();
        invalid[field] = json!("forbidden");
        assert!(
            serde_json::from_value::<CreateInput>(invalid).is_err(),
            "{field}"
        );
    }
    let schema = serde_json::to_value(schemars::schema_for!(CreateInput)).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    assert!(validator.is_valid(&value), "{schema}");
    let mut invalid = value;
    invalid["worktree"]["base"]["unexpected"] = json!(true);
    assert!(!validator.is_valid(&invalid));
}

#[tokio::test]
async fn setup_and_lookup_forward_only_the_fixed_repository_and_audit_no_recipe() {
    let daemon = RecordingDaemon::default();
    let dispatcher = Dispatcher::new(daemon.clone(), "/fixed/repo".into());
    let result = dispatcher
        .create_workspace(serde_json::from_value(input()).unwrap())
        .await;
    assert_eq!(result.is_error, Some(false));
    let result = dispatcher
        .get_creation(CreationGetInput {
            operation_id: "setup-key".into(),
        })
        .await;
    assert_eq!(result.is_error, Some(false));
    let calls = daemon.calls();
    assert_eq!(calls[0].0, "workspace.setup");
    assert_eq!(calls[0].1["repositoryPath"], "/fixed/repo");
    assert_eq!(
        calls[0].1["request"],
        serde_json::to_value(
            serde_json::from_value::<CreateInput>(input())
                .unwrap()
                .request
        )
        .unwrap()
    );
    assert_eq!(calls[2].0, "workspaceCreation.get");
    assert_eq!(
        calls[2].1,
        json!({"repositoryPath": "/fixed/repo", "operationId": "setup-key"})
    );
    assert_eq!(
        calls[1].1["details"],
        json!({"repositoryPath": "/fixed/repo"})
    );
}
