use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use super::support::*;
use super::*;

pub(super) async fn verify(
    paths: &TestPaths,
    repository: &Path,
    worktree: &Path,
    native_requests: &Arc<Mutex<Vec<Value>>>,
) -> Result<()> {
    let before = native_requests.lock().unwrap().len();
    let source = inspect(paths, repository).await?;
    let child = inspect(paths, worktree).await?;
    assert_eq!(source["repositoryId"], child["repositoryId"]);
    assert_eq!(source["rootPath"], json!(fs::canonicalize(repository)?));
    assert_eq!(child["rootPath"], json!(fs::canonicalize(worktree)?));
    assert_eq!(child["branch"], WORKSPACE_NAME);
    for state in [&source, &child] {
        assert_eq!(state["headSha"], state["baseSha"]);
        assert!(state["headSha"].as_str().is_some_and(|sha| sha.len() == 40));
        assert!(state["branches"].as_array().is_some_and(|branches| {
            branches.iter().any(|branch| {
                branch["reference"] == format!("refs/heads/{WORKSPACE_NAME}")
                    && !branch["checkoutPath"].is_null()
            })
        }));
    }
    assert_eq!(
        native_requests.lock().unwrap().len(),
        before,
        "Git inspection unexpectedly queried Codex"
    );
    Ok(())
}

async fn inspect(paths: &TestPaths, path: &Path) -> Result<Value> {
    timeout(PROCESS_TIMEOUT, async {
        let mut stream = UnixStream::connect(&paths.socket).await?;
        let request = json!({
            "id": "git-source-smoke", "method": "repository.git.inspect",
            "params": {
                "path": path, "includeBranches": true,
                "base": {"kind": "revision", "revision": "HEAD"},
            },
        });
        let mut encoded = serde_json::to_vec(&request)?;
        encoded.push(b'\n');
        stream.write_all(&encoded).await?;
        let mut response = String::new();
        BufReader::new(stream)
            .take(8 * 1024 * 1024)
            .read_line(&mut response)
            .await?;
        let response: Value = serde_json::from_str(&response)?;
        assert_eq!(response["id"], "git-source-smoke");
        ensure!(
            response.get("error").is_none(),
            "Git inspection failed: {response}"
        );
        response.get("result").cloned().context("missing Git state")
    })
    .await
    .context("Git inspection timed out")?
}
