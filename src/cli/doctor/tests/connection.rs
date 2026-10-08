use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::watch;

use super::*;
use crate::protocol::DoctorResult;
use crate::rpc::{RpcErrorPayload, RpcHandler, RpcServer};

struct Stub {
    response: Result<Value, RpcErrorPayload>,
    stall: bool,
}

#[async_trait]
impl RpcHandler for Stub {
    async fn handle(&self, method: &str, _params: Value) -> Result<Value, RpcErrorPayload> {
        assert_eq!(method, "doctor");
        if self.stall {
            std::future::pending::<()>().await;
        }
        self.response.clone()
    }
}

fn paths(root: &Path) -> CocoPaths {
    CocoPaths {
        data_dir: root.join("data"),
        database_path: root.join("state.db"),
        socket_path: root.join("cocod.sock"),
        codex_endpoint_path: root.join("endpoint.json"),
        codex_token_path: root.join("credentials"),
        worktrees_dir: root.join("worktrees"),
        codex_home: root.join("codex-home"),
        hooks_path: root.join("hooks.json"),
    }
}

fn info(paths: &CocoPaths) -> DoctorDaemonInfo {
    DoctorDaemonInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        pid: 123,
        executable: None,
        codex_binary: None,
        codex_version: Some("0.160.1".into()),
        codex_home: paths.codex_home.clone(),
        database_path: paths.database_path.clone(),
        endpoint_path: paths.codex_endpoint_path.clone(),
        token_path: paths.codex_token_path.clone(),
        execution_mode: "exec-server".into(),
    }
}

async fn inspect(stub: Stub, paths: &CocoPaths) -> DoctorReport {
    let (shutdown, receiver) = watch::channel(false);
    let server = RpcServer::bind(&paths.socket_path, Arc::new(stub))
        .await
        .unwrap();
    let task = tokio::spawn(server.run(receiver));
    let mut report = report(Vec::new());
    collect_live(
        &RpcClient::new(&paths.socket_path),
        paths,
        None,
        None,
        &mut report,
        Instant::now() + std::time::Duration::from_millis(100),
    )
    .await;
    shutdown.send(true).unwrap();
    task.await.unwrap().unwrap();
    report
}

#[tokio::test]
async fn old_coordinators_get_an_actionable_update_error_without_leaking_raw_messages() {
    let temporary = tempfile::tempdir().unwrap();
    let paths = paths(temporary.path());
    let report = inspect(
        Stub {
            response: Err(RpcErrorPayload::new(
                "METHOD_NOT_FOUND",
                "sensitive-untrusted-error",
            )),
            stall: false,
        },
        &paths,
    )
    .await;
    assert!(!report.complete);
    assert_eq!(report.checks[0].status, DiagnosticStatus::Error);
    assert!(
        report.checks[0]
            .hint
            .as_ref()
            .unwrap()
            .contains("Update coco and cocod")
    );
    assert!(
        !serde_json::to_string(&report)
            .unwrap()
            .contains("sensitive-")
    );
}

#[tokio::test]
async fn stalled_coordinator_reports_do_not_wait_indefinitely() {
    let temporary = tempfile::tempdir().unwrap();
    let paths = paths(temporary.path());
    let report = inspect(
        Stub {
            response: Ok(Value::Null),
            stall: true,
        },
        &paths,
    )
    .await;
    assert!(!report.complete);
    assert_eq!(
        report.checks[0].message,
        "Coordinator diagnostics timed out"
    );
}

#[tokio::test]
async fn live_metadata_detects_stale_versions_and_different_codex_homes() {
    let temporary = tempfile::tempdir().unwrap();
    let paths = paths(temporary.path());
    let mut daemon = info(&paths);
    daemon.version = "0.0.1-alpha.1".into();
    daemon.codex_home = temporary.path().join("other-codex-home");
    let response = DoctorResult {
        daemon,
        checks: Vec::new(),
        complete: true,
    };
    let report = inspect(
        Stub {
            response: Ok(serde_json::to_value(response).unwrap()),
            stall: false,
        },
        &paths,
    )
    .await;
    assert!(report.complete);
    assert!(
        report
            .checks
            .iter()
            .any(|check| check.id == "installation.live"
                && check.status == DiagnosticStatus::Warning)
    );
    assert!(
        report
            .checks
            .iter()
            .any(|check| check.id == "installation.paths"
                && check.status == DiagnosticStatus::Warning)
    );
}

#[test]
fn unchanged_metadata_and_native_codex_versions_do_not_get_spurious_warnings() {
    let temporary = tempfile::tempdir().unwrap();
    let paths = paths(temporary.path());
    let mut report = report(Vec::new());
    installation_consistency(
        &paths,
        Some(env!("CARGO_PKG_VERSION")),
        Some("0.160.1"),
        &info(&paths),
        &mut report,
    );
    assert!(
        report
            .checks
            .iter()
            .all(|check| check.status == DiagnosticStatus::Ok)
    );
}
