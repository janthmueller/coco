use super::*;
use crate::mcp::tests::RecordingDaemon;

#[test]
fn interrupt_grant_is_independent_of_send_and_defaults_to_read_only() {
    for send in [false, true] {
        for interrupt in [false, true] {
            let server = McpServer::new(
                PathBuf::from("/repo"),
                false,
                send,
                interrupt,
                PathBuf::from("/tmp/unused-turn-test.sock"),
                vec![],
            );
            assert!(server.get_tool("turns.get").is_some());
            assert_eq!(server.get_tool("turns.interrupt").is_some(), interrupt);
            assert_eq!(server.tool_router.has_route("turns.interrupt"), interrupt);
            assert_eq!(server.get_tool(WORKSPACES_SEND).is_some(), send);
        }
    }
}

#[test]
fn inputs_require_both_identities_and_cannot_override_repository_scope() {
    let valid = json!({"workspace": "feat/work", "operationId": "client-key"});
    assert!(serde_json::from_value::<TurnInput>(valid.clone()).is_ok());
    for input in [
        json!({"workspace": "feat/work"}),
        json!({"operationId": "client-key"}),
        json!({"workspace": "feat/work", "operationId": "client-key", "repository": "/other"}),
        json!({"workspace": "feat/work", "operationId": "client-key", "turnId": "newer-turn"}),
    ] {
        assert!(serde_json::from_value::<TurnInput>(input).is_err());
    }
    let schema = schemars::schema_for!(TurnInput);
    let schema = serde_json::to_value(schema).unwrap();
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["required"], json!(["workspace", "operationId"]));
}

#[tokio::test]
async fn tools_forward_exact_repository_workspace_and_client_key_and_audit_safely() {
    let daemon = RecordingDaemon::default();
    let dispatcher = Dispatcher::new(daemon.clone(), PathBuf::from("/fixed/repo"));
    for interrupt in [false, true] {
        let input = TurnInput {
            workspace: "feat/work".into(),
            operation_id: "operation-test".into(),
        };
        let result = if interrupt {
            dispatcher
                .turns_interrupt(TurnInterruptInput {
                    workspace: input.workspace,
                    operation_id: input.operation_id,
                    mode: TurnInterruptMode::Strict,
                })
                .await
        } else {
            dispatcher.turns_get(input).await
        };
        assert_eq!(result.is_error, Some(false));
        assert!(result.structured_content.is_some());
    }
    let calls = daemon.calls();
    assert_eq!(calls.len(), 4);
    for (index, method, tool) in [
        (0, "turn.get", "turns.get"),
        (2, "turn.interrupt", "turns.interrupt"),
    ] {
        assert_eq!(calls[index].0, method);
        let mut expected = json!({"scope": {"kind": "repository", "path": "/fixed/repo"},
            "workspace": "feat/work", "operationId": "operation-test"});
        if tool == "turns.interrupt" {
            expected["mode"] = json!("strict");
        }
        assert_eq!(calls[index].1, expected);
        assert_eq!(calls[index + 1].0, "audit.record");
        assert_eq!(calls[index + 1].1["action"], tool);
        assert_eq!(calls[index + 1].1["operationId"], "operation-test");
    }
}

#[test]
fn interrupt_mode_defaults_to_strict_and_schema_advertises_only_explicit_modes() {
    for (mode, expected) in [
        (None, TurnInterruptMode::Strict),
        (Some("strict"), TurnInterruptMode::Strict),
        (Some("bestEffort"), TurnInterruptMode::BestEffort),
    ] {
        let mut input = json!({"workspace": "feat/work", "operationId": "client-key"});
        if let Some(mode) = mode {
            input["mode"] = json!(mode);
        }
        assert_eq!(
            serde_json::from_value::<TurnInterruptInput>(input)
                .unwrap()
                .mode,
            expected
        );
    }
    for mode in [
        json!(null),
        json!("native"),
        json!("best_effort"),
        json!(false),
    ] {
        assert!(
            serde_json::from_value::<TurnInterruptInput>(
                json!({"workspace": "w", "operationId": "op", "mode": mode})
            )
            .is_err()
        );
    }
    let schema = serde_json::to_value(schemars::schema_for!(TurnInterruptInput)).unwrap();
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["required"], json!(["workspace", "operationId"]));
    assert_eq!(
        schema["properties"]["mode"]["enum"],
        json!(["strict", "bestEffort"])
    );
    assert_eq!(schema["properties"]["mode"]["default"], "strict");
    assert!(
        serde_json::from_value::<TurnInput>(
            json!({"workspace": "w", "operationId": "op", "mode": "bestEffort"})
        )
        .is_err()
    );
}

#[tokio::test]
async fn best_effort_mode_is_forwarded_without_broadening_repository_scope() {
    let daemon = RecordingDaemon::default();
    let dispatcher = Dispatcher::new(daemon.clone(), PathBuf::from("/fixed/repo"));
    dispatcher
        .turns_interrupt(TurnInterruptInput {
            workspace: "w".into(),
            operation_id: "op".into(),
            mode: TurnInterruptMode::BestEffort,
        })
        .await;
    let calls = daemon.calls();
    assert_eq!(calls[0].0, "turn.interrupt");
    assert_eq!(
        calls[0].1,
        json!({"scope": {"kind": "repository", "path": "/fixed/repo"}, "workspace": "w", "operationId": "op", "mode": "bestEffort"})
    );
}

#[tokio::test]
async fn unknown_operation_has_a_structured_code_without_leaking_native_error_details() {
    let daemon = RecordingDaemon::default();
    daemon.fail(
        "turn.get",
        DaemonFailure::new(
            "TURN_OPERATION_NOT_FOUND",
            "turn operation not found in this workspace",
        ),
    );
    let dispatcher = Dispatcher::new(daemon.clone(), PathBuf::from("/repo"));
    let result = dispatcher
        .turns_get(TurnInput {
            workspace: "feat/work".into(),
            operation_id: "unknown".into(),
        })
        .await;
    assert_eq!(result.is_error, Some(true));
    assert_eq!(
        result.structured_content.unwrap()["error"]["code"],
        "TURN_OPERATION_NOT_FOUND"
    );
    let calls = daemon.calls();
    assert_eq!(calls[1].1["details"]["error"], "TURN_OPERATION_NOT_FOUND");
    assert!(!calls[1].1.to_string().contains("turn operation not found"));
}
