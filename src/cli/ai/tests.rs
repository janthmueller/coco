use super::*;
use crate::cli::args::{AiApproval, Cli, Command as CliCommand};
use crate::protocol::ControlRepository;
use clap::Parser;

mod native;

fn arguments(extra: &[&str]) -> AiArgs {
    let cli = Cli::try_parse_from(["coco", "ai"].into_iter().chain(extra.iter().copied())).unwrap();
    let CliCommand::Ai(args) = cli.command else {
        panic!("AI command");
    };
    args
}

fn fixture() -> (tempfile::TempDir, CocoPaths, ControlSessionPreparation) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let home = root.join("codex home");
    std::fs::create_dir(&home).unwrap();
    let paths = CocoPaths {
        data_dir: root.join("data"),
        database_path: root.join("data/db"),
        socket_path: root.join("control socket"),
        codex_endpoint_path: root.join("endpoint"),
        codex_token_path: root.join("token"),
        worktrees_dir: root.join("worktrees"),
        codex_home: home.clone(),
        hooks_path: root.join("hooks"),
    };
    let prepared = ControlSessionPreparation {
        working_directory: root.into(),
        default_repository_id: Some("repo-a".into()),
        repositories: vec![ControlRepository {
            id: "repo-a".into(),
            name: "frontend".into(),
            path: root.into(),
        }],
        unavailable_repositories: vec![],
        codex_home: home,
        coco_version: env!("CARGO_PKG_VERSION").into(),
        mcp_servers: vec![],
    };
    (temp, paths, prepared)
}

fn settings(values: &[String]) -> serde_json::Value {
    // Each statement must be valid TOML, including quotes, dotted keys and
    // inline tables. Overlaying these is the same shape passed to native CLI.
    let mut output = serde_json::json!({});
    for value in values {
        let table: toml::Table =
            toml::from_str(value).unwrap_or_else(|error| panic!("{value}: {error}"));
        merge(&mut output, serde_json::to_value(table).unwrap());
    }
    output
}

fn merge(target: &mut serde_json::Value, patch: serde_json::Value) {
    match (target, patch) {
        (serde_json::Value::Object(target), serde_json::Value::Object(patch)) => {
            for (key, value) in patch {
                merge(target.entry(key).or_insert(serde_json::Value::Null), value);
            }
        }
        (target, value) => *target = value,
    }
}

#[test]
fn ai_accepts_an_optional_prompt_model_and_explicit_approval_without_scope_flags() {
    let args = arguments(&[]);
    assert!(args.prompt.is_none() && args.model.is_none() && !args.read_only);
    assert_eq!(args.approval, AiApproval::Ask);
    let args = arguments(&[
        "--model",
        "custom-model",
        "--approval",
        "auto",
        "create two workspaces",
    ]);
    assert_eq!(args.model.as_deref(), Some("custom-model"));
    assert_eq!(args.prompt.as_deref(), Some("create two workspaces"));
    assert_eq!(args.approval, AiApproval::Auto);
    assert!(arguments(&["--read-only"]).read_only);
    for args in [
        vec!["--read-only", "--approval", "auto"],
        vec!["--approval", "unknown"],
        vec![" "],
    ] {
        assert!(Cli::try_parse_from(["coco", "ai"].into_iter().chain(args)).is_err());
    }
}

#[test]
fn native_config_uses_scoped_mcp_confirmations_and_removes_execution_escape_hatches() {
    let (_temp, paths, mut prepared) = fixture();
    prepared.mcp_servers = vec![
        crate::protocol::InheritedMcpServer {
            name: "old.server\"x".into(),
            transport: crate::protocol::McpTransportKind::Stdio,
        },
        crate::protocol::InheritedMcpServer {
            name: "old.http".into(),
            transport: crate::protocol::McpTransportKind::StreamableHttp,
        },
    ];
    let values = config::overrides(
        &paths,
        &prepared,
        &arguments(&[]),
        Path::new("/bin/coco 'quoted'"),
    )
    .unwrap();
    let config = settings(&values);
    assert_eq!(config["features"]["shell_tool"], false);
    assert_eq!(config["features"]["code_mode"], false);
    assert_eq!(config["features"]["code_mode_host"], true);
    assert_eq!(config["features"]["plugins"], false);
    assert_eq!(config["features"]["hooks"], false);
    assert_eq!(config["agents"]["enabled"], false);
    assert_eq!(
        config["approval_policy"]["granular"]["sandbox_approval"],
        false
    );
    assert_eq!(
        config["approval_policy"]["granular"]["mcp_elicitations"],
        true
    );
    assert_eq!(config["approvals_reviewer"], "user");
    assert_eq!(config["mcp_servers"]["old.server\"x"]["enabled"], false);
    assert_eq!(
        config["mcp_servers"]["old.server\"x"]["command"],
        "/bin/coco 'quoted'"
    );
    assert_eq!(
        config["mcp_servers"]["old.server\"x"]["args"],
        serde_json::json!(["--version"])
    );
    assert_eq!(config["mcp_servers"]["old.http"]["enabled"], false);
    assert_eq!(
        config["mcp_servers"]["old.http"]["url"],
        "http://127.0.0.1:1/coco-disabled"
    );
    assert!(config["mcp_servers"]["old.http"].get("command").is_none());
    let servers = config["mcp_servers"].as_object().unwrap();
    let server = servers
        .iter()
        .find(|(key, _)| key.starts_with("coco_repo_"))
        .unwrap()
        .1;
    assert_eq!(server["command"], "/bin/coco 'quoted'");
    assert_eq!(
        server["args"][3],
        serde_json::to_value(&prepared.repositories[0].path).unwrap()
    );
    assert_eq!(server["required"], true);
    assert_eq!(server["default_tools_approval_mode"], "prompt");
    assert_eq!(
        server["tools"]["workspaces.list"]["approval_mode"],
        "approve"
    );
    assert_eq!(
        server["tools"]["workspaces.create"]["approval_mode"],
        "approve"
    );
    assert_eq!(
        server["env"]["COCO_SOCKET_PATH"],
        serde_json::to_value(&paths.socket_path).unwrap()
    );
    assert_eq!(
        server["args"].as_array().unwrap().last().unwrap(),
        "--confirm-changes"
    );
}

#[test]
fn each_repository_has_its_own_endpoint_and_inventory_without_an_all_flag() {
    let (_temp, paths, mut prepared) = fixture();
    prepared.repositories.push(ControlRepository {
        id: "repo-b".into(),
        name: "backend".into(),
        path: PathBuf::from("/other/repo $x; [quoted]"),
    });
    let config = settings(
        &config::overrides(&paths, &prepared, &arguments(&[]), Path::new("/bin/coco")).unwrap(),
    );
    let servers = config["mcp_servers"].as_object().unwrap();
    assert_eq!(servers.len(), 2);
    for repo in &prepared.repositories {
        assert!(
            servers
                .values()
                .any(|server| server["args"][3] == serde_json::to_value(&repo.path).unwrap())
        );
    }
    let instructions = config["developer_instructions"].as_str().unwrap();
    assert!(instructions.contains("frontend") && instructions.contains("backend"));
    assert!(instructions.contains("\"default\":true"));
    assert!(instructions.contains("Before a mutating call show a short readable review"));
}

#[test]
fn automatic_approval_is_explicit_and_read_only_removes_every_mutation_grant() {
    let (_temp, paths, prepared) = fixture();
    for (extra, expected_mode, writes) in [
        (vec![], "prompt", true),
        (vec!["--approval", "auto"], "approve", true),
        (vec!["--read-only"], "prompt", false),
    ] {
        let config = settings(
            &config::overrides(
                &paths,
                &prepared,
                &arguments(&extra),
                Path::new("/bin/coco"),
            )
            .unwrap(),
        );
        let server = config["mcp_servers"]
            .as_object()
            .unwrap()
            .values()
            .next()
            .unwrap();
        assert_eq!(server["default_tools_approval_mode"], expected_mode);
        let args = server["args"].as_array().unwrap();
        for flag in [
            "--allow-create",
            "--allow-send",
            "--allow-interrupt",
            "--allow-delete",
            "--allow-close",
            "--allow-reopen",
        ] {
            assert_eq!(args.iter().any(|arg| arg == flag), writes);
        }
        for feature in ["apps", "connectors", "memories", "memory_tool"] {
            assert_eq!(config["features"][feature], false, "{feature}: {extra:?}");
        }
    }
}

#[test]
fn launch_keeps_prompt_and_paths_as_literal_arguments_and_isolates_the_native_session() {
    let (_temp, paths, prepared) = fixture();
    let args = arguments(&[
        "--model",
        "native-model",
        "--",
        "-c dangerous=true; $(not executed)",
    ]);
    let command = launch_command(
        &paths,
        &prepared,
        &args,
        Path::new("/bin/codex"),
        Path::new("/bin/coco"),
    )
    .unwrap();
    let command = command.as_std();
    let values = command
        .get_args()
        .map(|value| value.to_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(&values[..3], ["--no-daemon", "--sandbox", "read-only"]);
    assert_eq!(
        &values[values.len() - 4..],
        [
            "--model",
            "native-model",
            "--",
            "-c dangerous=true; $(not executed)"
        ]
    );
    assert!(
        !values
            .iter()
            .any(|arg| *arg == "--worktree" || *arg == "--remote")
    );
    let canonical_home = prepared.codex_home.canonicalize().unwrap();
    assert!(
        command
            .get_envs()
            .any(|(key, value)| key == "CODEX_HOME" && value == Some(canonical_home.as_os_str()))
    );
}

#[test]
fn mismatched_or_unavailable_preparation_fails_closed() {
    let (_temp, paths, mut prepared) = fixture();
    validate_preparation(&paths, &prepared).unwrap();
    prepared.coco_version = "old".into();
    assert!(validate_preparation(&paths, &prepared).is_err());
    prepared.coco_version = env!("CARGO_PKG_VERSION").into();
    prepared.codex_home = paths.data_dir.clone();
    assert!(validate_preparation(&paths, &prepared).is_err());
}
