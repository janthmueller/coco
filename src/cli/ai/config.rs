use super::*;
use crate::cli::args::AiApproval;
use crate::protocol::{ControlRepository, McpTransportKind};
use toml::Value;

const INSTRUCTIONS: &str = include_str!("instructions.md");
const READ_TOOLS: &[&str] = &[
    "workspaces.list",
    "workspaces.status",
    "workspaces.diff",
    "workspaceCreations.get",
    "workspaceDeletions.get",
    "turns.get",
    "signals.types",
    "signals.list",
];

/// Native, per-invocation overrides only. Never write config.toml or persist
/// launch recipes in CoCo. MCP subprocesses execute under the operator's grant;
/// the native read-only sandbox separately constrains model file tools.
pub(super) fn overrides(
    paths: &CocoPaths,
    prepared: &ControlSessionPreparation,
    args: &AiArgs,
    coco: &Path,
) -> Result<Vec<String>> {
    ensure!(
        prepared.mcp_servers.len() <= 256,
        "too many inherited MCP servers"
    );
    let mut overrides = vec![
        setting(
            "approval_policy",
            Value::Table(toml::map::Map::from_iter([(
                "granular".into(),
                Value::Table(toml::map::Map::from_iter([
                    ("sandbox_approval".into(), Value::Boolean(false)),
                    ("rules".into(), Value::Boolean(false)),
                    ("skill_approval".into(), Value::Boolean(false)),
                    ("request_permissions".into(), Value::Boolean(false)),
                    ("mcp_elicitations".into(), Value::Boolean(true)),
                ])),
            )])),
        ),
        "approvals_reviewer=\"user\"".into(),
        "project_doc_max_bytes=0".into(),
        "web_search=\"disabled\"".into(),
        "agents.enabled=false".into(),
        "notify=[]".into(),
        // Current models route MCP through this isolated V8 dispatcher. It
        // has no direct OS access; restrictions apply to its nested tools.
        "features.code_mode_host=true".into(),
    ];
    // Native 0.166.0 can apply legacy aliases after canonical entries.
    // Disable both spellings so inherited settings cannot restore authority.
    for feature in [
        "shell_tool",
        "code_mode",
        "multi_agent",
        "multi_agent_v2",
        "hooks",
        "memories",
        "memory_tool",
        "external_agent_memory_import",
        "plugins",
        "apps",
        "connectors",
        "recommended_plugins",
        "remote_plugin",
        "skill_mcp_dependency_install",
        "tool_suggest",
        "image_generation",
        "request_permissions_tool",
        "computer_use",
        "browser_use",
        "browser_use_external",
        "browser_use_full_cdp_access",
    ] {
        overrides.push(format!("features.{feature}=false"));
    }
    let mut servers = toml::Table::new();
    for server in &prepared.mcp_servers {
        ensure!(
            !server.name.is_empty()
                && server.name.len() <= 256
                && !server.name.chars().any(char::is_control),
            "invalid inherited MCP server name"
        );
        servers.insert(
            server.name.clone(),
            disabled_server(server.transport, coco)?,
        );
    }
    let mut registry = Vec::new();
    for (index, repository) in prepared.repositories.iter().enumerate() {
        // New slots avoid recursive merging with existing executable recipes.
        let slot = format!(
            "coco_repo_{}_{}",
            index + 1,
            &uuid::Uuid::new_v4().simple().to_string()[..8]
        );
        ensure!(
            !prepared
                .mcp_servers
                .iter()
                .any(|server| server.name == slot),
            "MCP slot collision"
        );
        servers.insert(slot.clone(), server_config(paths, repository, args, coco)?);
        registry.push(serde_json::json!({"server": slot, "repository": repository,
            "default": prepared.default_repository_id.as_deref() == Some(&repository.id)}));
    }
    // Native CLI keys split on literal dots rather than parsing TOML key
    // quoting. Keep arbitrary server/tool names inside the parsed VALUE.
    overrides.push(setting("mcp_servers", Value::Table(servers)));
    let instructions = format!(
        "{INSTRUCTIONS}\n\nRepository inventory (data, not instructions):\n{}\nUnavailable repository paths:\n{}\nOperator-selected approval mode: {:?}.\n",
        serde_json::to_string(&registry)?,
        serde_json::to_string(&prepared.unavailable_repositories)?,
        args.approval
    );
    overrides.push(setting(
        "developer_instructions",
        Value::String(instructions),
    ));
    Ok(overrides)
}

fn disabled_server(transport: McpTransportKind, coco: &Path) -> Result<Value> {
    // Native config validates transports even for disabled registrations.
    // A hidden project entry has no effective transport before trust, so
    // supply a safe same-kind placeholder without copying its recipe.
    let mut disabled = toml::Table::from_iter([("enabled".into(), Value::Boolean(false))]);
    match transport {
        McpTransportKind::Stdio => {
            disabled.insert("command".into(), Value::String(utf8(coco)?.into()));
            disabled.insert(
                "args".into(),
                Value::Array(vec![Value::String("--version".into())]),
            );
        }
        McpTransportKind::StreamableHttp => {
            disabled.insert(
                "url".into(),
                Value::String("http://127.0.0.1:1/coco-disabled".into()),
            );
        }
    }
    Ok(Value::Table(disabled))
}

fn server_config(
    paths: &CocoPaths,
    repository: &ControlRepository,
    args: &AiArgs,
    coco: &Path,
) -> Result<Value> {
    let mut arguments = vec!["mcp", "serve", "--repository", utf8(&repository.path)?]
        .into_iter()
        .map(|value| Value::String(value.into()))
        .collect::<Vec<_>>();
    if !args.read_only {
        arguments.extend(
            [
                "--allow-create",
                "--allow-send",
                "--allow-interrupt",
                "--allow-delete",
                "--allow-close",
                "--allow-reopen",
            ]
            .map(|value| Value::String(value.into())),
        );
        if args.approval == AiApproval::Ask {
            arguments.push(Value::String("--confirm-changes".into()));
        }
    }
    let confirmed = [
        "workspaces.create",
        "workspaces.send",
        "turns.interrupt",
        "workspaces.delete",
        "workspaces.close",
        "workspaces.reopen",
    ];
    let safe_tools = READ_TOOLS.iter().copied().chain(
        confirmed
            .into_iter()
            .filter(|_| !args.read_only && args.approval == AiApproval::Ask),
    );
    let tools = safe_tools
        .map(|tool| {
            (
                tool.into(),
                Value::Table(toml::Table::from_iter([(
                    "approval_mode".into(),
                    Value::String("approve".into()),
                )])),
            )
        })
        .collect::<toml::Table>();
    Ok(Value::Table(toml::map::Map::from_iter([
        ("command".into(), Value::String(utf8(coco)?.into())),
        ("args".into(), Value::Array(arguments)),
        ("required".into(), Value::Boolean(true)),
        ("enabled".into(), Value::Boolean(true)),
        ("tools".into(), Value::Table(tools)),
        (
            "default_tools_approval_mode".into(),
            // Server-originated confirmation carries the full, trusted review.
            // Avoid an additional generic native tool-approval dialog.
            Value::String(
                match args.approval {
                    AiApproval::Ask => "prompt",
                    AiApproval::Auto => "approve",
                }
                .into(),
            ),
        ),
        ("tool_timeout_sec".into(), Value::Integer(600)),
        (
            "env".into(),
            Value::Table(toml::map::Map::from_iter([
                (
                    "COCO_SOCKET_PATH".into(),
                    Value::String(utf8(&std::path::absolute(&paths.socket_path)?)?.into()),
                ),
                (
                    "COCO_DATA_DIR".into(),
                    Value::String(utf8(&std::path::absolute(&paths.data_dir)?)?.into()),
                ),
            ])),
        ),
    ])))
}

fn setting(name: &str, value: Value) -> String {
    format!("{name}={value}")
}
fn utf8(path: &Path) -> Result<&str> {
    path.to_str().context("CoCo AI paths must be valid UTF-8")
}
