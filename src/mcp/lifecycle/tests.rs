use super::*;

#[test]
fn lifecycle_grants_are_independent_and_lookup_stays_read_only() {
    for bits in 0..64 {
        let permissions = ServePermissions {
            allow_create: bits & 1 != 0,
            allow_send: bits & 2 != 0,
            allow_interrupt: bits & 4 != 0,
            allow_delete: bits & 8 != 0,
            allow_close: bits & 16 != 0,
            allow_reopen: bits & 32 != 0,
            ..Default::default()
        };
        let server = McpServer::new(
            "/repo".into(),
            permissions,
            "/tmp/unused.sock".into(),
            vec![],
        );
        for (name, granted) in [
            ("workspaces.create", permissions.allow_create),
            ("workspaces.send", permissions.allow_send),
            ("turns.interrupt", permissions.allow_interrupt),
            ("workspaces.delete", permissions.allow_delete),
            ("workspaces.close", permissions.allow_close),
            ("workspaces.reopen", permissions.allow_reopen),
        ] {
            assert_eq!(server.get_tool(name).is_some(), granted, "{name}");
            assert_eq!(server.tool_router.has_route(name), granted, "{name}");
        }
        assert!(server.get_tool("workspaceDeletions.get").is_some());
    }
}

#[test]
fn delete_inputs_require_a_key_and_never_accept_scope_force_or_confirmation() {
    let input: DeleteInput =
        serde_json::from_value(json!({"workspace":"fix/login", "operationId":"delete-key"}))
            .unwrap();
    assert!(!input.keep_thread && !input.keep_branch);
    assert!(!input.discard_changes && !input.discard_unretained_commits && !input.dry_run);
    for bad in [
        json!({"workspace":"fix/login"}),
        json!({"workspace":"fix/login", "operationId":"k", "repository":"/other"}),
        json!({"workspace":"fix/login", "operationId":"k", "force":true}),
        json!({"workspace":"fix/login", "operationId":"k", "confirm":true}),
        json!({"workspace":"fix/login", "operationId":"k", "deleteThread":true}),
    ] {
        assert!(serde_json::from_value::<DeleteInput>(bad).is_err());
    }
    let schema = serde_json::to_value(schemars::schema_for!(DeleteInput)).unwrap();
    assert_eq!(schema["additionalProperties"], false);
}
