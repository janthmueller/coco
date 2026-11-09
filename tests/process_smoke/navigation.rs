use super::support::*;
use super::*;

pub(super) async fn verify(
    paths: &TestPaths,
    repository: &Path,
    worktree: &Path,
    native_requests: &Arc<Mutex<Vec<Value>>>,
) -> Result<()> {
    let before = native_requests.lock().unwrap().len();
    let jump_before = fs::read(&paths.jump_args).ok();
    let output = run_cli(paths, repository, &["path", WORKSPACE_NAME]).await?;
    assert_eq!(
        String::from_utf8(output.stdout)?,
        format!("{}\n", worktree.display())
    );
    assert!(output.stderr.is_empty());
    assert_eq!(
        native_requests.lock().unwrap().len(),
        before,
        "path queried Codex"
    );
    assert_eq!(
        fs::read(&paths.jump_args).ok(),
        jump_before,
        "path launched a TUI"
    );
    Ok(())
}
