use super::*;

#[tokio::test]
#[ignore = "requires selected Codex; isolated synthetic legacy history, no model calls"]
async fn native_summary_reports_legacy_mode_without_a_full_transcript_replay() -> Result<()> {
    require_explicit_opt_in()?;
    let binary = env::var_os(CODEX_BINARY_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| "codex".into());
    let temporary = tempfile::tempdir()?;
    let paths = TestPaths::new(temporary.path());
    for directory in [&paths.home, &paths.codex_home, &paths.data_dir] {
        fs::create_dir_all(directory)?;
    }
    verify_codex_version(&binary, &paths).await?;
    let repository = temporary.path().join("repository");
    prepare_repository(&repository)?;
    let thread = "0192a058-0000-7000-8000-000000000456";
    let size = write_legacy_fixture(&paths, &repository, thread)?;
    let mut server =
        RealAppServer::spawn(&paths, &binary, &repository, "turn-legacy-summary").await?;
    let params = json!({"threadId": thread, "includeTurns": false});
    server.request("thread/read", params.clone()).await?;
    #[cfg(target_os = "linux")]
    let before = native_read_bytes(server.child.id().context("native child missing")?)?;
    let summary = server.request("thread/read", params).await?;
    assert_eq!(summary["thread"]["id"], thread);
    assert_eq!(summary["thread"]["historyMode"], "legacy");
    assert_eq!(summary["thread"]["status"]["type"], "notLoaded");
    ensure!(
        summary["thread"]["turns"]
            .as_array()
            .is_some_and(Vec::is_empty)
    );
    #[cfg(target_os = "linux")]
    {
        let after = native_read_bytes(server.child.id().context("native child missing")?)?;
        ensure!(
            after.saturating_sub(before) < size as u64,
            "summary replayed the full legacy transcript"
        );
    }
    #[cfg(not(target_os = "linux"))]
    let _ = size;
    server.stop().await
}

fn write_legacy_fixture(paths: &TestPaths, repository: &Path, thread: &str) -> Result<usize> {
    let directory = paths.codex_home.join("sessions/2026/10/08");
    fs::create_dir_all(&directory)?;
    let records = [
        json!({"timestamp": "2026-10-08T12:00:00Z", "type": "session_meta", "payload": {
            "id": thread, "session_id": thread, "timestamp": "2026-10-08T12:00:00Z", "cwd": repository,
            "originator": "codex", "cli_version": "0.0.0", "source": "cli", "model_provider": "openai" }}),
        json!({"timestamp": "2026-10-08T12:00:01Z", "type": "response_item", "payload": {
            "type": "message", "role": "user", "content": [{"type": "input_text", "text": "legacy metadata fixture"}] }}),
        json!({"timestamp": "2026-10-08T12:00:01Z", "type": "event_msg", "payload": {
            "type": "user_message", "message": "legacy metadata fixture", "kind": "plain" }}),
        json!({"timestamp": "2026-10-08T12:00:02Z", "type": "response_item", "payload": {
            "type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "x".repeat(4*1024*1024)}] }}),
    ];
    let contents = records
        .iter()
        .map(|record| format!("{record}\n"))
        .collect::<String>();
    fs::write(
        directory.join(format!("rollout-2026-10-08T12-00-00-{thread}.jsonl")),
        &contents,
    )?;
    Ok(contents.len())
}

#[cfg(target_os = "linux")]
fn native_read_bytes(pid: u32) -> Result<u64> {
    fs::read_to_string(format!("/proc/{pid}/io"))?
        .lines()
        .find_map(|line| line.strip_prefix("rchar: "))
        .context("native I/O counter missing")?
        .parse()
        .context("invalid native I/O counter")
}
