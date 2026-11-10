use super::*;

#[tokio::test]
#[ignore = "requires COCO_RUN_REAL_CODEX_COMPAT=1 and Codex 0.166.0; feature inspection only, no model"]
async fn installed_codex_cannot_reenable_apps_or_memories_through_legacy_aliases() -> Result<()> {
    ensure!(
        std::env::var("COCO_RUN_REAL_CODEX_COMPAT").as_deref() == Ok("1"),
        "set COCO_RUN_REAL_CODEX_COMPAT=1 for this native configuration proof"
    );
    let (temp, paths, prepared) = fixture();
    let program = std::env::var_os("COCO_CODEX_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| "codex".into());
    let program = resolve_executable(&program).context("native Codex is unavailable")?;
    let mut version = Command::new(&program);
    version.arg("--version");
    let version = probe(version, temp.path(), &prepared.codex_home).await?;
    ensure!(
        parse_version(&version, "codex-cli ").as_deref() == Some(CONTROL_CODEX_VERSION),
        "this proof requires Codex {CONTROL_CODEX_VERSION}"
    );
    let original = "check_for_update_on_startup=false\n[features]\napps=false\nconnectors=true\nmemories=false\nmemory_tool=true\n";
    let config_file = prepared.codex_home.join("config.toml");
    std::fs::write(&config_file, original)?;
    let mut inherited = Command::new(&program);
    inherited.args(["features", "list"]);
    let inherited = probe(inherited, temp.path(), &prepared.codex_home).await?;
    for feature in ["apps", "memories"] {
        let actual = resolved_feature(&inherited, feature);
        ensure!(
            actual == Some(true),
            "fixture did not reproduce inherited {feature}: {actual:?}"
        );
    }
    for extra in [vec![], vec!["--approval", "auto"], vec!["--read-only"]] {
        // Exercise the actual launcher arguments, not a synthetic TOML merge.
        // `features list` does not start a conversation or an MCP subprocess.
        let mut command = launch_command(
            &paths,
            &prepared,
            &arguments(&extra),
            &program,
            Path::new("/unused-coco-feature-probe"),
        )?;
        command.args(["features", "list"]);
        let output = probe(command, temp.path(), &prepared.codex_home).await?;
        for feature in ["apps", "memories", "shell_tool", "hooks", "plugins"] {
            let actual = resolved_feature(&output, feature);
            ensure!(
                actual == Some(false),
                "{feature} was not disabled in {extra:?}: {actual:?}"
            );
        }
    }
    ensure!(
        std::fs::read_to_string(config_file)? == original,
        "feature inspection changed the inherited configuration"
    );
    Ok(())
}

async fn probe(mut command: Command, root: &Path, codex_home: &Path) -> Result<String> {
    command
        .current_dir(root)
        .env("HOME", root)
        .env("CODEX_HOME", codex_home)
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CACHE_HOME", root.join("cache"));
    capture_command(command)
        .await
        .map_err(|error| anyhow::anyhow!("isolated native feature probe failed: {error:?}"))
}

fn resolved_feature(output: &str, name: &str) -> Option<bool> {
    output.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        (fields.next() == Some(name))
            .then(|| match fields.next_back()? {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            })
            .flatten()
    })
}
