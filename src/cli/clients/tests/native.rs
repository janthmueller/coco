use super::*;
use std::path::PathBuf;
use std::process::Command as SyncCommand;

struct TmuxServer {
    socket: PathBuf,
}

impl Drop for TmuxServer {
    fn drop(&mut self) {
        let _ = SyncCommand::new("tmux")
            .arg("-N")
            .arg("-S")
            .arg(&self.socket)
            .arg("kill-server")
            .env_remove("TMUX")
            .output();
    }
}

impl TmuxServer {
    fn start(socket: PathBuf) -> Self {
        let server = Self { socket };
        let output = SyncCommand::new("tmux")
            .arg("-f")
            .arg("/dev/null")
            .arg("-S")
            .arg(&server.socket)
            .args(["new-session", "-d", "-s", "coco-test", "sleep 60"])
            .env_remove("TMUX")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        server
    }

    fn context(&self) -> TmuxContext {
        let output = SyncCommand::new("tmux")
            .arg("-N")
            .arg("-S")
            .arg(&self.socket)
            .args([
                "display-message",
                "-p",
                "#{socket_path},#{pid},#{session_id}\t#{pane_id}",
            ])
            .env_remove("TMUX")
            .output()
            .unwrap();
        assert!(output.status.success());
        let output = String::from_utf8(output.stdout).unwrap();
        let (server, pane) = output.trim_end().split_once('\t').unwrap();
        // TMUX stores the numeric session ID without its format-output '$'.
        let (socket_pid, session) = server.rsplit_once(',').unwrap();
        TmuxContext::parse(
            &format!("{socket_pid},{}", session.trim_start_matches('$')),
            pane,
        )
        .unwrap()
    }
}

#[tokio::test]
#[ignore = "requires installed tmux; uses only two isolated temporary servers"]
async fn native_tmux_location_is_detected_without_touching_user_sessions() {
    let temp = tempfile::tempdir().unwrap();
    let first = TmuxServer::start(temp.path().join("first.sock"));
    let second = TmuxServer::start(temp.path().join("second.sock"));
    let one = inspect_tmux(Command::new("tmux"), &first.context())
        .await
        .unwrap();
    let two = inspect_tmux(Command::new("tmux"), &second.context())
        .await
        .unwrap();
    assert_eq!(one.locator, two.locator);
    assert_eq!(one.label.as_deref(), Some("coco-test:0.0"));
    assert_ne!(one.scope, two.scope);
    let output = SyncCommand::new("tmux")
        .arg("-N")
        .arg("-S")
        .arg(&first.socket)
        .args(["rename-session", "-t", "coco-test", "renamed"])
        .env_remove("TMUX")
        .output()
        .unwrap();
    assert!(output.status.success());
    let renamed = inspect_tmux(Command::new("tmux"), &first.context())
        .await
        .unwrap();
    assert_eq!(one.scope, renamed.scope);
    assert_eq!(one.locator, renamed.locator);
    assert_eq!(renamed.label.as_deref(), Some("renamed:0.0"));
}
