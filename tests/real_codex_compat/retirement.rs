use super::*;

pub(super) async fn verify_workspace_retirement(
    paths: &TestPaths,
    codex_binary: &Path,
    repository: &Path,
    source_status: &Value,
) -> Result<()> {
    let (worktree, branch) = source_binding(source_status)?;
    retire_context_fork(paths, codex_binary, repository).await?;
    close_source(paths, codex_binary, repository, source_status, &worktree).await?;
    reopen_and_reclose_source(paths, codex_binary, repository, source_status, &worktree).await?;
    delete_source(paths, codex_binary, repository, &branch).await
}

fn source_binding(source_status: &Value) -> Result<(PathBuf, String)> {
    let worktree = PathBuf::from(
        source_status
            .pointer("/workspace/worktreePath")
            .and_then(Value::as_str)
            .context("source workspace had no worktree path")?,
    );
    let branch = source_status
        .pointer("/workspace/branchName")
        .and_then(Value::as_str)
        .context("source workspace had no branch")?
        .to_owned();
    Ok((worktree, branch))
}

async fn close_source(
    paths: &TestPaths,
    codex_binary: &Path,
    repository: &Path,
    source_status: &Value,
    worktree: &Path,
) -> Result<()> {
    run_cli(
        paths,
        codex_binary,
        repository,
        &["close", WORKSPACE_NAME, "--archive-thread"],
    )
    .await?;
    assert_workspace_executor_stopped(source_status)?;
    ensure!(
        !worktree.exists(),
        "native archive close retained the worktree"
    );
    let hidden = run_cli(paths, codex_binary, repository, &["list", "--json"]).await?;
    let hidden: Value = serde_json::from_slice(&hidden.stdout)?;
    ensure!(
        hidden["workspaces"]
            .as_array()
            .is_some_and(|items| items.iter().all(|item| item["name"] != WORKSPACE_NAME)),
        "normal list did not hide the retired source workspace: {hidden}"
    );
    let closed = run_cli(
        paths,
        codex_binary,
        repository,
        &["list", "--closed", "--json"],
    )
    .await?;
    let closed: Value = serde_json::from_slice(&closed.stdout)?;
    ensure!(
        closed["workspaces"]
            .as_array()
            .is_some_and(|items| items.iter().any(|item| item["name"] == WORKSPACE_NAME)),
        "closed list did not expose the retired source workspace: {closed}"
    );
    Ok(())
}

async fn reopen_and_reclose_source(
    paths: &TestPaths,
    codex_binary: &Path,
    repository: &Path,
    source_status: &Value,
    worktree: &Path,
) -> Result<()> {
    run_cli(paths, codex_binary, repository, &["reopen", WORKSPACE_NAME]).await?;
    ensure!(
        worktree.is_dir(),
        "reopen did not restore the exact worktree"
    );
    attach_workspace(paths, repository)
        .await
        .context("reopened workspace could not be attached")?;
    let reopened = workspace_status(paths, codex_binary, repository).await?;
    ensure!(
        reopened.pointer("/workspace/codexThreadId")
            == source_status.pointer("/workspace/codexThreadId"),
        "reopen changed the native Codex thread identity: {reopened}"
    );
    ensure!(
        reopened.pointer("/runtimeResources/state") == Some(&json!("running")),
        "reopen did not re-establish its workspace executor: {reopened}"
    );

    run_cli(
        paths,
        codex_binary,
        repository,
        &["close", WORKSPACE_NAME, "--archive-thread"],
    )
    .await?;
    assert_workspace_executor_stopped(&reopened)?;
    Ok(())
}

async fn delete_source(
    paths: &TestPaths,
    codex_binary: &Path,
    repository: &Path,
    branch: &str,
) -> Result<()> {
    run_cli(
        paths,
        codex_binary,
        repository,
        &[
            "delete",
            WORKSPACE_NAME,
            "--delete-thread",
            "--delete-branch",
            "--yes",
        ],
    )
    .await?;
    let branch_status = Command::new("git")
        .args([
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .current_dir(repository)
        .status()
        .await?;
    ensure!(
        !branch_status.success(),
        "permanent deletion retained the explicitly selected branch"
    );
    Ok(())
}

async fn retire_context_fork(
    paths: &TestPaths,
    codex_binary: &Path,
    repository: &Path,
) -> Result<()> {
    let active = workspace_status_for(paths, codex_binary, repository, FORK_WORKSPACE_NAME).await?;
    run_cli(
        paths,
        codex_binary,
        repository,
        &["close", FORK_WORKSPACE_NAME, "--archive-thread"],
    )
    .await?;
    assert_workspace_executor_stopped(&active)?;
    run_cli(
        paths,
        codex_binary,
        repository,
        &[
            "delete",
            FORK_WORKSPACE_NAME,
            "--delete-thread",
            "--delete-branch",
            "--yes",
        ],
    )
    .await?;
    Ok(())
}
