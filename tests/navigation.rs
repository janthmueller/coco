#![cfg(unix)]

use anyhow::{Result, ensure};
use serde_json::json;

#[path = "navigation/support.rs"]
mod support;

use support::Fixture;

#[tokio::test]
async fn path_prints_one_exact_line_and_uses_only_the_lightweight_lookup() -> Result<()> {
    let fixture = Fixture::new().await?;
    let output = fixture.run(&["path", "fix/login"]).await?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout)?,
        format!("{}\n", fixture.destination.display())
    );
    assert!(output.stderr.is_empty());
    assert_eq!(fixture.calls()[0]["method"], "workspace.path.get");
    assert_eq!(fixture.calls()[0]["params"]["workspace"], "fix/login");
    assert_eq!(
        fixture.calls()[0]["params"]["scope"],
        json!({"kind":"repository", "path":fixture.root.path()})
    );
    Ok(())
}

#[tokio::test]
async fn path_preserves_explicit_repository_and_global_uuid_scopes() -> Result<()> {
    let fixture = Fixture::new().await?;
    for args in [
        vec!["../api", "path", "fix/login"],
        vec!["path", "fix/login", "-g"],
        vec!["-g", "path", "fix/login"],
        vec!["path", "118a6e27-e5d8-47fa-9213-7fa9f7c07fb4"],
        vec!["path", "fix/login", "-g", "--sort", "name"],
        vec!["path", "fix/login", "-g", "--sort", "recent"],
    ] {
        let output = fixture.run(&args).await?;
        ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let calls = fixture.calls();
    assert_eq!(
        calls[0]["params"]["scope"],
        json!({"kind":"repository", "path":fixture.root.path().join("../api")})
    );
    assert!(
        calls[1..]
            .iter()
            .all(|call| call["params"]["scope"] == json!({"kind":"allRepositories"}))
    );
    Ok(())
}

#[tokio::test]
async fn path_errors_never_emit_a_destination_and_missing_input_does_not_query() -> Result<()> {
    let fixture = Fixture::new().await?;
    for args in [
        vec!["path"],
        vec!["--no-input", "path"],
        vec!["-a", "path", "fix/login"],
        vec!["cd", "fix/login"],
    ] {
        let output = fixture.run(&args).await?;
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
    assert!(fixture.calls().is_empty());
    let output = fixture.run(&["path", "missing"]).await?;
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("WORKSPACE_NOT_FOUND"));
    Ok(())
}

#[tokio::test]
async fn shell_initialization_is_offline_and_rejects_repository_scope() -> Result<()> {
    let fixture = Fixture::new().await?;
    for shell in ["bash", "zsh", "fish"] {
        let output = fixture.run(&["shell", "init", shell]).await?;
        ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("function coco")
                || String::from_utf8_lossy(&output.stdout).contains("coco()")
        );
        assert!(output.stderr.is_empty());
        let output = fixture.run(&["../repo", "shell", "init", shell]).await?;
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
    assert!(fixture.calls().is_empty());
    Ok(())
}
