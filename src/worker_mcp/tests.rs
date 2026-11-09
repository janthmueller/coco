use super::*;

fn source(directory: &std::path::Path, value: &Value) -> WorkerMcpReference {
    let path = directory.join("worker.json");
    let bytes = serde_json::to_vec(value).unwrap();
    fs::write(&path, &bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    WorkerMcpReference {
        path,
        sha256: hex::encode(Sha256::digest(&bytes)),
    }
}

fn recipe() -> Value {
    json!({"mcp_servers": {"worker": {
        "command": "/fixture/worker", "args": ["--grant", "run-42"],
        "enabled_tools": ["run.submit"], "default_tools_approval_mode": "auto", "required": true
    }}})
}

#[test]
fn normalized_server_aliases_are_rejected_in_profiles_and_recipes() {
    let temp = tempfile::tempdir().unwrap();
    let mut value = recipe();
    let server = value["mcp_servers"]
        .as_object_mut()
        .unwrap()
        .remove("worker")
        .unwrap();
    value["mcp_servers"]["run_worker"] = server.clone();
    let loaded = load(&source(temp.path(), &value)).unwrap();
    assert!(
        loaded
            .apply(&mut json!({"mcp_servers": {"RUN-WORKER": {"command": "stale"}}}))
            .is_err()
    );
    value["mcp_servers"]["run-worker"] = server;
    assert!(load(&source(temp.path(), &value)).is_err());
}

#[test]
fn exact_bytes_and_strict_schema_are_checked_without_quoting_private_values() {
    let temp = tempfile::tempdir().unwrap();
    let reference = source(temp.path(), &recipe());
    let loaded = load(&reference).unwrap();
    let mut config = json!({"model": "fixture", "mcp_servers": {"other": {"command": "other"}}});
    loaded.apply(&mut config).unwrap();
    assert_eq!(
        config["mcp_servers"]["worker"],
        recipe()["mcp_servers"]["worker"]
    );
    assert!(loaded.apply(&mut config).is_err());
    let mut bytes = fs::read(&reference.path).unwrap();
    bytes.push(b'\n');
    fs::write(&reference.path, bytes).unwrap();
    assert!(
        load(&reference)
            .err()
            .unwrap()
            .to_string()
            .contains("SHA-256")
    );
    for field in ["env", "env_vars", "url", "bearer_token_env_var", "unknown"] {
        let mut value = recipe();
        value["mcp_servers"]["worker"][field] = json!("DO_NOT_LEAK_FIXTURE");
        let reference = source(temp.path(), &value);
        let error = load(&reference).err().unwrap().to_string();
        assert!(!error.contains("DO_NOT_LEAK_FIXTURE"));
        assert!(!error.contains("run-42"));
    }
}

#[test]
fn missing_invalid_and_oversized_sources_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let mut reference = source(temp.path(), &recipe());
    for bytes in [vec![0xff], vec![b' '; MAX_RECIPE_BYTES + 1], b"{}".to_vec()] {
        fs::write(&reference.path, &bytes).unwrap();
        reference.sha256 = hex::encode(Sha256::digest(&bytes));
        assert!(load(&reference).is_err());
    }
    fs::remove_file(&reference.path).unwrap();
    assert!(load(&reference).is_err());
    reference.path = "relative.json".into();
    assert!(load(&reference).is_err());
    reference.path = temp.path().join("worker.json");
    reference.sha256 = "A".repeat(64);
    assert!(load(&reference).is_err());
}

#[cfg(unix)]
#[test]
fn symlink_substitution_and_public_permissions_are_rejected() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let temp = tempfile::tempdir().unwrap();
    let reference = source(temp.path(), &recipe());
    fs::set_permissions(&reference.path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(load(&reference).is_err());
    fs::set_permissions(&reference.path, fs::Permissions::from_mode(0o600)).unwrap();
    let target = temp.path().join("same-bytes.json");
    fs::rename(&reference.path, &target).unwrap();
    symlink(&target, &reference.path).unwrap();
    assert!(load(&reference).is_err());
}
