use super::*;

#[test]
fn versions_accept_alpha_and_apple_git_but_not_arbitrary_output() {
    assert_eq!(
        parse_version("cocod 0.1.0-alpha.10\n", "cocod "),
        Some("0.1.0-alpha.10".into())
    );
    assert_eq!(
        parse_version("git version 2.50.1 (Apple Git-155)\n", "git version "),
        Some("2.50.1".into())
    );
    for invalid in [
        "",
        "token",
        "1...",
        "0.1.0\nsecret=value",
        "0.1.secret",
        "\u{1b}[1m0.1.0",
    ] {
        assert!(parse_version(&format!("codex-cli {invalid}"), "codex-cli ").is_none());
    }
}

#[cfg(unix)]
fn script(source: &str) -> (tempfile::TempDir, Command) {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("probe");
    std::fs::write(&path, format!("#!/bin/sh\n{source}\n")).unwrap();
    // Execute the existing shell, not a freshly written executable: another
    // concurrent spawn can transiently inherit its writable descriptor.
    let mut command = Command::new("/bin/sh");
    command.arg(path);
    (temporary, command)
}

#[cfg(unix)]
#[tokio::test]
async fn successful_output_is_returned_from_a_non_executable_fixture() {
    use std::os::unix::fs::PermissionsExt;
    let (temporary, command) = script("printf 'probe-output'");
    assert_eq!(
        std::fs::metadata(temporary.path().join("probe"))
            .unwrap()
            .permissions()
            .mode()
            & 0o111,
        0
    );
    assert_eq!(capture_command(command).await, Ok("probe-output".into()));
}

#[tokio::test]
async fn a_missing_executable_remains_unavailable() {
    let temporary = tempfile::tempdir().unwrap();
    let command = Command::new(temporary.path().join("missing"));
    assert_eq!(capture_command(command).await, Err(ProbeError::Unavailable));
}

#[cfg(unix)]
#[tokio::test]
async fn dropping_a_probe_before_polling_wait_still_reaps_it() {
    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", "exec sleep 3600"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let child = command.spawn().unwrap();
    let pid = child.id().unwrap();
    drop(ProbeChild(Some(child)));
    assert_reaped(pid).await;
}

#[cfg(unix)]
#[tokio::test]
async fn oversized_output_terminates_instead_of_retaining_unbounded_data() {
    let (_temporary, command) =
        script("while :; do printf 'long-output-line-long-output-line\\n'; done");
    assert_eq!(
        capture_command(command).await,
        Err(ProbeError::InvalidOutput)
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_stalled_subprocess_times_out_and_is_reaped() {
    let (temporary, command) = script(
        "printf '%s' \"$$\" > \"$0.pid.tmp\"\nmv \"$0.pid.tmp\" \"$0.pid\"\nexec sleep 3600",
    );
    let started = tokio::time::Instant::now();
    assert_eq!(capture_command(command).await, Err(ProbeError::TimedOut));
    assert!(started.elapsed() < PROBE_TIMEOUT + Duration::from_secs(2));
    let pid: u32 = std::fs::read_to_string(temporary.path().join("probe.pid"))
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        !Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .unwrap()
            .success()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn cancelling_a_probe_kills_its_owned_subprocess() {
    let (temporary, command) = script(
        "printf '%s' \"$$\" > \"$0.pid.tmp\"\nmv \"$0.pid.tmp\" \"$0.pid\"\nexec sleep 3600",
    );
    let task = tokio::spawn(capture_command(command));
    let pid_path = temporary.path().join("probe.pid");
    timeout(Duration::from_secs(2), async {
        while !pid_path.exists() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let pid: u32 = std::fs::read_to_string(pid_path).unwrap().parse().unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_reaped(pid).await;
}

#[cfg(unix)]
async fn assert_reaped(pid: u32) {
    timeout(Duration::from_secs(2), async {
        loop {
            if !Command::new("kill")
                .args(["-0", &pid.to_string()])
                .stderr(std::process::Stdio::null())
                .status()
                .await
                .unwrap()
                .success()
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("owned diagnostic child was not killed and reaped");
}
