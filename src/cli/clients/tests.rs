use super::*;

#[cfg(unix)]
mod native;

fn context() -> TmuxContext {
    TmuxContext::parse("/tmp/coco-tmux.sock,123,0", "%7").unwrap()
}

#[test]
fn context_requires_bounded_exact_tmux_identifiers() {
    for (server, pane) in [
        ("", "%1"),
        ("relative,123,0", "%1"),
        ("/tmp/socket,unknown,0", "%1"),
        ("/tmp/socket,123,no-session", "%1"),
        ("/tmp/socket,123,0", "-t"),
        ("/tmp/socket,123,0", "%1\n"),
        ("/tmp/socket\n,123,0", "%1"),
    ] {
        assert!(TmuxContext::parse(server, pane).is_none());
    }
    assert!(TmuxContext::parse(&format!("/{},123,0", "x".repeat(1200)), "%1").is_none());
    let parsed = TmuxContext::parse("/tmp/socket,with,commas,123,0", "%1").unwrap();
    assert_eq!(parsed.socket, "/tmp/socket,with,commas");
}

#[test]
fn location_is_structured_and_server_scoped_without_exposing_socket_paths() {
    let first =
        parse_tmux_location("/tmp/coco-tmux.sock\t123\t42\t%7\tdev\t2\t1\n", &context()).unwrap();
    assert_eq!(first.label.as_deref(), Some("dev:2.1"));
    assert_eq!(first.locator, "%7");
    assert_eq!(first.scope.len(), 64);
    let second =
        parse_tmux_location("/tmp/coco-tmux.sock\t123\t43\t%7\tdev\t2\t1\n", &context()).unwrap();
    assert_ne!(first.scope, second.scope);
    let encoded = serde_json::to_string(&first).unwrap();
    assert!(!encoded.contains("/tmp/"));
    assert!(!encoded.contains("\"pid\""));
}

#[test]
fn invalid_location_output_is_ignored() {
    for output in [
        "/other/socket\t123\t42\t%7\tdev\t2\t1\n",
        "/tmp/coco-tmux.sock\t124\t42\t%7\tdev\t2\t1\n",
        "/tmp/coco-tmux.sock\t123\t42\t%8\tdev\t2\t1\n",
        "/tmp/coco-tmux.sock\t123\t42\t%7\tdev\tX\t1\n",
        "/tmp/coco-tmux.sock\t123\t42\t%7\t\x1b[31mdev\t2\t1\n",
        "/tmp/coco-tmux.sock\t123\t42\t%7\tdev\t2\t1\nextra",
    ] {
        assert!(
            parse_tmux_location(output, &context()).is_none(),
            "{output:?}"
        );
    }
    assert!(parse_tmux_location(&"x".repeat(2049), &context()).is_none());
}

#[tokio::test]
async fn missing_tmux_is_nonfatal() {
    assert!(
        inspect_tmux(Command::new("/nonexistent/coco-tmux"), &context())
            .await
            .is_none()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn broken_or_slow_tmux_is_nonfatal_and_bounded() {
    let mut broken = Command::new("sh");
    broken.args(["-c", "exit 1"]);
    assert!(inspect_tmux(broken, &context()).await.is_none());
    let mut slow = Command::new("sh");
    slow.args(["-c", "exec sleep 10"]);
    let started = std::time::Instant::now();
    assert!(inspect_tmux(slow, &context()).await.is_none());
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[cfg(unix)]
#[tokio::test]
async fn successful_probe_uses_literal_arguments_and_validates_its_output() {
    let mut command = Command::new("sh");
    command.args([
        "-c",
        "test \"$1\" = -N && test \"$2\" = -S && test \"$3\" = /tmp/coco-tmux.sock && test \"$7\" = '%7' || exit 1; printf '/tmp/coco-tmux.sock\\t123\\t42\\t%%7\\tdev\\t2\\t1\\n'",
        "tmux-probe",
    ]);
    let result = inspect_tmux(command, &context()).await.unwrap();
    assert_eq!(result.label.as_deref(), Some("dev:2.1"));
}
