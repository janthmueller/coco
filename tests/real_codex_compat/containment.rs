use std::fs;

use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use tokio::process::Command;
use tokio::time::timeout;

use super::{COMPATIBILITY_TIMEOUT, TestPaths};

pub(super) async fn verify_scope_hierarchy(unit: &str) -> Result<()> {
    let output = timeout(
        COMPATIBILITY_TIMEOUT,
        Command::new("systemctl")
            .args([
                "--user",
                "--no-pager",
                "show",
                "--property=ControlGroup",
                "--value",
                unit,
            ])
            .output(),
    )
    .await
    .context("timed out while inspecting the workspace systemd scope")??;
    ensure!(
        output.status.success(),
        "could not inspect workspace systemd scope: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let control_group = String::from_utf8_lossy(&output.stdout);
    let instance = unit
        .strip_prefix("coco")
        .and_then(|value| {
            value
                .split_once("-workspace-")
                .map(|(instance, _)| instance)
        })
        .context("workspace scope had no opaque CoCo instance component")?;
    let expected_suffix =
        format!("/app-coco{instance}.slice/app-coco{instance}-workspaces.slice/{unit}");
    ensure!(
        control_group.trim().ends_with(&expected_suffix),
        "workspace scope was outside its instance workspace pool: {control_group}"
    );
    Ok(())
}

pub(super) async fn verify_scope_cleanup(paths: &TestPaths) -> Result<()> {
    let canonical = fs::canonicalize(&paths.data_dir)?;
    let digest = Sha256::digest(canonical.as_os_str().as_encoded_bytes());
    let pattern = format!("coco{}-workspace-*.scope", hex::encode(&digest[..16]));
    let output = timeout(
        COMPATIBILITY_TIMEOUT,
        Command::new("systemctl")
            .args([
                "--user",
                "--no-pager",
                "--no-legend",
                "--plain",
                "list-units",
                "--all",
                "--type=scope",
                &pattern,
            ])
            .output(),
    )
    .await
    .context("timed out while checking workspace scope cleanup")??;
    ensure!(
        output.status.success(),
        "could not check workspace scope cleanup: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    ensure!(
        output.stdout.iter().all(u8::is_ascii_whitespace),
        "workspace scopes remained after daemon shutdown: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    Ok(())
}
