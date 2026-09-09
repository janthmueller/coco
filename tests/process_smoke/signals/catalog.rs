use super::*;

pub(super) async fn verify(paths: &TestPaths, repository: &Path) -> Result<()> {
    let directory = catalog_dir(paths);
    fs::create_dir_all(&directory)?;
    let first = directory.join("review.requested@1.json");
    let second = directory.join("review.requested@2.json");
    let schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "description": "Request a review", "type": "object",
        "properties": {"pr": {"type": "integer"}},
        "required": ["pr"], "additionalProperties": false
    });
    for path in [&first, &second] {
        fs::write(path, serde_json::to_vec(&schema)?)?;
    }
    let mut reader = client(paths, repository, false).await?;
    let original = reader.call("signals.types", json!({})).await?;
    assert_eq!(original.as_array().map(Vec::len), Some(2));
    assert!(
        original
            .as_array()
            .unwrap()
            .iter()
            .all(|definition| definition["emitAllowed"] == false)
    );
    // Reloading an identical schema through the other executable preserves the snapshot.
    let mut command = Command::new(env!("CARGO_BIN_EXE_coco"));
    paths.apply(&mut command);
    command
        .current_dir(repository)
        .args(["mcp", "serve", "--repository", ".", "--signal-catalog"])
        .arg(&directory);
    let mut reloaded = McpClient::spawn(&mut command).await?;
    assert_eq!(reloaded.call("signals.types", json!({})).await?, original);
    reloaded.stop().await?;

    let mut changed = schema.clone();
    changed["properties"]["pr"]["type"] = json!("string");
    fs::write(&first, serde_json::to_vec(&changed)?)?;
    let added = directory.join("added@1.json");
    fs::write(&added, "true")?;
    rejected_start(paths, repository, None, "SIGNAL_VERSION_CONFLICT").await?;
    assert_eq!(
        reader.call("signals.types", json!({})).await?,
        original,
        "file changes altered a running snapshot"
    );
    assert_eq!(
        retained_types(paths, repository).await?,
        original,
        "a failed load partially registered definitions"
    );
    fs::remove_file(&added)?;
    fs::write(&first, serde_json::to_vec_pretty(&schema)?)?;

    fs::remove_file(&second)?;
    assert_eq!(reader.call("signals.types", json!({})).await?, original);
    let mut reduced = client(paths, repository, false).await?;
    assert_eq!(
        reduced
            .call("signals.types", json!({}))
            .await?
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    reduced.stop().await?;
    rejected_start(
        paths,
        repository,
        Some("review.requested@2"),
        "no matching schema",
    )
    .await?;
    assert_eq!(
        retained_types(paths, repository).await?,
        original,
        "removing a file erased a historical definition"
    );
    fs::write(&second, serde_json::to_vec(&schema)?)?;
    reader.stop().await?;
    Ok(())
}

async fn retained_types(paths: &TestPaths, repository: &Path) -> Result<Value> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_coco-mcp"));
    paths.apply(&mut command);
    command.arg("--repository").arg(repository);
    let mut reader = McpClient::spawn(&mut command).await?;
    let definitions = reader.call("signals.types", json!({})).await?;
    reader.stop().await?;
    Ok(definitions)
}

async fn rejected_start(
    paths: &TestPaths,
    repository: &Path,
    grant: Option<&str>,
    expected: &str,
) -> Result<()> {
    let mut command = catalog_command(paths, repository);
    if let Some(grant) = grant {
        command.args(["--allow-emit", grant]);
    }
    let result = timeout(
        Duration::from_secs(10),
        command.stdin(Stdio::null()).kill_on_drop(true).output(),
    )
    .await??;
    ensure!(
        !result.status.success(),
        "invalid catalog started successfully"
    );
    ensure!(
        String::from_utf8_lossy(&result.stderr).contains(expected),
        "unexpected catalog error: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    ensure!(
        result.stdout.is_empty(),
        "failed startup served partial MCP output"
    );
    Ok(())
}
