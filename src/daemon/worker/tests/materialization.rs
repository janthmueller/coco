use super::super::*;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

fn empty_rollout(path: &Path) -> Value {
    json!({"code": -32603, "message": format!(
        "failed to read thread: thread-store internal error: failed to read session metadata {}: rollout at {} is empty",
        path.display(), path.display()
    )})
}

async fn worker(responses: Vec<Value>) -> (CodexWorker, tokio::task::JoinHandle<()>) {
    let (client, _events, stream) = CodexClient::test_io_pair(64 * 1024).await;
    let task = tokio::spawn(async move {
        let (reader, mut writer) = tokio::io::split(stream);
        let mut lines = BufReader::new(reader).lines();
        for response in responses {
            let request: Value =
                serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
            assert_eq!(request["method"], "thread/read");
            assert_eq!(
                request["params"],
                json!({"threadId": "thread-1", "includeTurns": false})
            );
            let mut frame = response;
            frame["id"] = request["id"].clone();
            writer
                .write_all(format!("{frame}\n").as_bytes())
                .await
                .unwrap();
        }
    });
    (CodexWorker::new(client, None), task)
}

#[tokio::test]
async fn fresh_empty_rollout_is_pending_and_can_later_be_adopted() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("rollout-2026-11-08-thread-1.jsonl");
    std::fs::write(&path, "").unwrap();
    let (worker, server) = worker(vec![
        json!({"error": empty_rollout(&path)}),
        json!({"result": {"thread": {
            "id": "thread-1", "cwd": "/worktree", "path": path,
            "status": {"type": "idle"}
        }}}),
    ])
    .await;
    assert!(
        worker
            .find_materialized_thread("thread-1", Path::new("/worktree"))
            .await
            .unwrap()
            .is_none()
    );
    std::fs::write(&path, "{\"type\":\"session_meta\"}\n").unwrap();
    let thread = worker
        .find_materialized_thread("thread-1", Path::new("/worktree"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(thread.id, "thread-1");
    server.await.unwrap();
}

#[tokio::test]
async fn empty_rollout_error_does_not_make_a_bound_thread_look_deleted() {
    let path = Path::new("/sessions/rollout-2026-11-08-thread-1.jsonl");
    let (worker, server) = worker(vec![json!({"error": empty_rollout(path)})]).await;
    assert!(worker.locate_thread("thread-1").await.is_err());
    server.await.unwrap();
}

#[tokio::test]
async fn fresh_adoption_preserves_other_storage_and_identity_errors() {
    let path = Path::new("/sessions/rollout-2026-11-08-thread-1.jsonl");
    let mut wrong_code = empty_rollout(path);
    wrong_code["code"] = json!(-32600);
    let mut structured = empty_rollout(path);
    structured["data"] = json!({"type": "otherStorageFailure"});
    let mut mismatched_paths = empty_rollout(path);
    mismatched_paths["message"] = json!(format!(
        "failed to read thread: thread-store internal error: failed to read session metadata {}: rollout at /other/rollout-2026-11-08-thread-1.jsonl is empty",
        path.display()
    ));
    for error in [
        wrong_code,
        structured,
        mismatched_paths,
        empty_rollout(Path::new("sessions/rollout-2026-11-08-thread-1.jsonl")),
        empty_rollout(Path::new("/sessions/rollout-2026-11-08-other-thread.jsonl")),
        json!({"code": -32603, "message": "permission denied: rollout is empty"}),
        json!({"code": -32603, "message": "failed to read thread: database is corrupt"}),
    ] {
        let (worker, server) = worker(vec![json!({"error": error})]).await;
        assert!(
            worker
                .find_materialized_thread("thread-1", Path::new("/worktree"))
                .await
                .is_err()
        );
        server.await.unwrap();
    }
}
