use super::*;

pub(super) async fn verify(paths: &TestPaths, repository: &Path, signals: &Value) -> Result<()> {
    let id = signals[0]["workspaceId"]
        .as_str()
        .context("signal sender missing")?;
    let root = repository.parent().context("repository parent missing")?;
    let first_repo = root.join("repository");
    let explicit_path = repository.to_str().context("repository path invalid")?;
    for spelling in ["list", "ls"] {
        for (cwd, arguments) in [
            (
                repository,
                vec!["signal", spelling, WORKSPACE_NAME, "--json"],
            ),
            (
                root,
                vec![explicit_path, "signal", spelling, WORKSPACE_NAME, "--json"],
            ),
            (root, vec!["signal", spelling, id, "--json"]),
            (root, vec!["signal", spelling, "-a", "--json"]),
        ] {
            let page = cli_json(&run_cli(paths, cwd, &arguments).await?)?;
            assert_eq!(page["signals"], *signals, "wrong scope for {arguments:?}");
        }
        let local = cli_json(
            &run_cli(
                paths,
                &first_repo,
                &["signal", spelling, WORKSPACE_NAME, "--json"],
            )
            .await?,
        )?;
        assert_eq!(
            local["signals"],
            json!([]),
            "local read leaked another repository's history"
        );
        let ambiguous = capture_cli(
            paths,
            root,
            &["signal", spelling, WORKSPACE_NAME, "-g"],
            None,
        )
        .await?;
        ensure!(
            !ambiguous.status.success(),
            "global duplicate name resolved"
        );
        let error = String::from_utf8_lossy(&ambiguous.stderr);
        ensure!(
            error.contains("WORKSPACE_REFERENCE_AMBIGUOUS"),
            "wrong ambiguity error: {error}"
        );
        ensure!(
            error.contains(explicit_path)
                && error.contains(first_repo.to_str().context("path invalid")?),
            "ambiguity omitted repository choices: {error}"
        );
    }

    let local_cursor = cli_json(
        &run_cli(
            paths,
            repository,
            &["signal", "ls", WORKSPACE_NAME, "--json"],
        )
        .await?,
    )?;
    let cursor = local_cursor["nextCursor"]
        .as_str()
        .context("cursor missing")?;
    let mismatched = capture_cli(
        paths,
        root,
        &["signal", "ls", "-a", "--after", cursor],
        None,
    )
    .await?;
    ensure!(!mismatched.status.success());
    ensure!(String::from_utf8_lossy(&mismatched.stderr).contains("SIGNAL_CURSOR_INVALID"));
    Ok(())
}
