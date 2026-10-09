use super::support::*;
use super::*;

struct HeldJump {
    child: Option<Child>,
    marker: PathBuf,
}

impl Drop for HeldJump {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.marker);
    }
}

impl HeldJump {
    fn start(paths: &TestPaths, repository: &Path, name: &str, broken_tmux: bool) -> Result<Self> {
        let marker = paths.home.join(format!("hold-{name}"));
        fs::create_dir_all(&paths.home)?;
        fs::write(&marker, "hold")?;
        let mut command = Command::new(env!("CARGO_BIN_EXE_coco"));
        paths.apply(&mut command);
        command
            .args(["jump", WORKSPACE_NAME])
            .current_dir(repository)
            .env("COCO_TEST_JUMP_HOLD_PATH", &marker)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if broken_tmux {
            command
                .env(
                    "TMUX",
                    format!("{},123,0", paths.home.join("absent-tmux.sock").display()),
                )
                .env("TMUX_PANE", "%7");
        }
        Ok(Self {
            child: Some(command.spawn()?),
            marker,
        })
    }

    async fn finish(mut self) -> Result<()> {
        fs::remove_file(&self.marker)?;
        let output = timeout(
            PROCESS_TIMEOUT,
            self.child.take().unwrap().wait_with_output(),
        )
        .await??;
        ensure!(
            output.status.success(),
            "jump failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    }
}

async fn wait_for_clients(paths: &TestPaths, repository: &Path, count: usize) -> Result<Value> {
    timeout(PROCESS_TIMEOUT, async {
        loop {
            let status = workspace_status(paths, repository).await?;
            if status["clients"].as_array().map(Vec::len) == Some(count) {
                return Ok(status);
            }
            sleep(POLL_INTERVAL).await;
        }
    })
    .await
    .context("client presence did not reach the expected count")?
}

pub(super) async fn exercise(paths: &TestPaths, repository: &Path) -> Result<()> {
    assert!(
        workspace_status(paths, repository).await?["clients"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let first = HeldJump::start(paths, repository, "first", false)?;
    wait_for_clients(paths, repository, 1).await?;
    let second = HeldJump::start(paths, repository, "second", true)?;
    let status = wait_for_clients(paths, repository, 2).await?;
    assert_eq!(status["workspace"]["phase"], "active");
    for client in status["clients"].as_array().unwrap() {
        assert_eq!(client["metadata"]["kind"], "native_tui");
        assert!(client["metadata"].get("integration").is_none());
        assert!(client.get("leaseId").is_none());
    }
    let plain = run_cli(paths, repository, &["status"]).await?;
    assert!(!String::from_utf8_lossy(&plain.stdout).contains("CLIENTS"));
    for arguments in [
        vec!["status", "--clients"],
        vec!["status", "-at", "--clients"],
        vec!["status", WORKSPACE_NAME, "--clients"],
    ] {
        let output = run_cli(paths, repository, &arguments).await?;
        assert!(String::from_utf8_lossy(&output.stdout).contains("TUI ×2"));
    }
    let followed =
        run_cli_until_interrupt(paths, repository, &["status", "-af", "--clients"]).await?;
    assert!(String::from_utf8_lossy(&followed.stdout).contains("TUI ×2"));
    let collection = cli_json(&run_cli(paths, repository, &["status", "-a", "--json"]).await?)?;
    assert_eq!(collection["workspaces"][0]["clients"], status["clients"]);
    first.finish().await?;
    let remaining = wait_for_clients(paths, repository, 1).await?;
    assert_eq!(remaining["workspace"]["phase"], "active");
    second.finish().await?;
    wait_for_clients(paths, repository, 0).await?;
    Ok(())
}
