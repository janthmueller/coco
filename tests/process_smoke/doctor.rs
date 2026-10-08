use super::support::*;
use super::*;

pub(super) async fn verify_live(
    paths: &TestPaths,
    repository: &Path,
    token: &str,
    observed: &Arc<Mutex<Vec<Value>>>,
) -> Result<()> {
    let before = observed.lock().unwrap().len();
    let output = capture_cli(paths, repository, &["doctor", "--json"], None).await?;
    let report = cli_json(&output)?;
    ensure!(output.status.success(), "doctor failed: {report}");
    ensure!(report["schemaVersion"] == 14 && report["complete"] == true);
    ensure!(report["daemon"]["pid"].as_u64().is_some());
    ensure!(
        report["checks"]
            .as_array()
            .context("missing doctor checks")?
            .iter()
            .any(|check| check["id"] == "state" && check["status"] == "ok")
    );
    ensure!(
        output.stderr.is_empty(),
        "doctor duplicated its diagnostic output on stderr"
    );
    ensure!(
        !String::from_utf8_lossy(&output.stdout).contains(token),
        "doctor leaked a capability token"
    );
    let requests = observed.lock().unwrap();
    ensure!(
        requests.len() - before == 2,
        "doctor did not read both model pages exactly once"
    );
    ensure!(
        requests[before..]
            .iter()
            .all(|request| request["method"] == "model/list"),
        "doctor issued a mutating Codex request"
    );
    Ok(())
}

#[tokio::test]
async fn doctor_without_a_daemon_reports_errors_without_creating_state() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let paths = TestPaths::new(temporary.path());
    write_fake_codex(&paths.fake_codex)?;
    let output = capture_cli(&paths, temporary.path(), &["doctor", "--json"], None).await?;
    ensure!(output.status.code() == Some(1));
    ensure!(
        output.stderr.is_empty(),
        "doctor printed a redundant error after JSON"
    );
    let report = cli_json(&output)?;
    ensure!(report["complete"] == false);
    ensure!(
        report["checks"]
            .as_array()
            .context("missing checks")?
            .iter()
            .any(|check| check["id"] == "daemon" && check["status"] == "error")
    );
    for path in [
        &paths.data_dir,
        &paths.database,
        &paths.socket,
        &paths.worktrees,
        &paths.codex_home,
        &paths.codex_args,
        &paths.jump_args,
    ] {
        ensure!(!path.exists(), "doctor created {}", path.display());
    }
    let output = capture_cli(&paths, temporary.path(), &["doctor"], None).await?;
    ensure!(output.status.code() == Some(1) && output.stderr.is_empty());
    let human = String::from_utf8_lossy(&output.stdout);
    ensure!(human.contains("Start cocod") && human.contains("Workspace checks"));
    ensure!(!human.contains('\u{1b}') && !human.contains("selection"));
    Ok(())
}
