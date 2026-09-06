use anyhow::Result;
use serde_json::{Value, json};

pub(super) fn phase_label(phase: &str) -> &'static str {
    match phase {
        "provisioning" => "Preparing worktree",
        "starting" => "Starting Codex",
        "active" => "Working",
        "waiting_for_approval" => "Waiting for approval",
        "waiting_for_input" => "Waiting for input",
        "idle" => "Ready",
        "not_loaded" => "Codex thread is unloaded",
        "system_error" => "Codex system error",
        "unavailable" => "Status unavailable",
        "completed" => "Completed",
        "failed" => "Failed",
        _ => "Unknown",
    }
}

pub(super) fn versioned(value: Value) -> Value {
    match value {
        Value::Object(mut object) => {
            object.insert("schemaVersion".into(), Value::from(5));
            Value::Object(object)
        }
        value => json!({ "schemaVersion": 5, "result": value }),
    }
}

pub(super) fn versioned_array(key: &str, value: Value) -> Value {
    let mut object = serde_json::Map::new();
    object.insert("schemaVersion".to_owned(), Value::from(5));
    object.insert(key.to_owned(), value);
    Value::Object(object)
}

pub(super) fn print_json(value: Value) -> Result<()> {
    println!("{}", serde_json::to_string(&value)?);
    Ok(())
}

pub(super) fn print_human(value: &Value) {
    if let Some(workspace) = value.get("workspace") {
        print_human(workspace);
        if let Some(turn_id) = value.get("turnId").and_then(Value::as_str) {
            println!("turn: {turn_id}");
        }
        return;
    }
    if let Some(object) = value.as_object() {
        for key in [
            "id",
            "name",
            "phase",
            "rootPath",
            "worktreePath",
            "branchName",
            "baseSha",
            "codexThreadId",
            "profile",
        ] {
            if let Some(entry) = object.get(key) {
                println!("{}: {}", human_key(key), compact(entry));
            }
        }
    } else {
        println!("{}", compact(value));
    }
}

pub(super) fn print_workspace_list(value: &Value, include_repository: bool) {
    let Some(workspaces) = value.as_array() else {
        println!("No workspaces.");
        return;
    };
    if workspaces.is_empty() {
        println!("No workspaces.");
        return;
    }
    if include_repository {
        println!("ID\tNAME\tREPOSITORY\tPHASE\tBRANCH");
    } else {
        println!("ID\tNAME\tPHASE\tBRANCH");
    }
    for workspace in workspaces {
        if include_repository {
            println!(
                "{}\t{}\t{}\t{}\t{}",
                text(workspace, "id"),
                text(workspace, "name"),
                workspace
                    .pointer("/repository/rootPath")
                    .map(compact)
                    .unwrap_or_else(|| "-".into()),
                text(workspace, "phase"),
                text(workspace, "branchName")
            );
        } else {
            println!(
                "{}\t{}\t{}\t{}",
                text(workspace, "id"),
                text(workspace, "name"),
                text(workspace, "phase"),
                text(workspace, "branchName")
            );
        }
    }
}

pub(super) fn print_repository_list(value: &Value) {
    let Some(repositories) = value.as_array() else {
        println!("No repositories.");
        return;
    };
    if repositories.is_empty() {
        println!("No repositories.");
        return;
    }
    println!("ID\tNAME\tPATH");
    for repository in repositories {
        println!(
            "{}\t{}\t{}",
            text(repository, "id"),
            text(repository, "displayName"),
            text(repository, "rootPath")
        );
    }
}

pub(super) fn print_model_list(value: &Value) {
    let Some(models) = value.as_array() else {
        println!("No models available.");
        return;
    };
    if models.is_empty() {
        println!("No models available.");
        return;
    }
    println!("MODEL\tNAME\tDEFAULT\tREASONING");
    for model in models {
        let reasoning = model
            .get("supportedReasoningEfforts")
            .and_then(Value::as_array)
            .map(|efforts| {
                efforts
                    .iter()
                    .filter_map(|effort| effort.get("reasoningEffort").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .filter(|efforts| !efforts.is_empty())
            .unwrap_or_else(|| "-".to_owned());
        println!(
            "{}\t{}\t{}\t{}",
            text(model, "model"),
            text(model, "displayName"),
            if model
                .get("isDefault")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                "yes"
            } else {
                ""
            },
            reasoning,
        );
    }
}

pub(super) fn print_status(value: &Value) {
    let workspace = value.get("workspace").unwrap_or(value);
    let name = workspace
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("workspace");
    let phase = workspace
        .get("phase")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    println!("{name}: {}", phase_label(phase));
    if let Some(worktree) = workspace.get("worktreePath").and_then(Value::as_str) {
        println!("worktree: {worktree}");
    }
    if let Some(message) = workspace.get("lastErrorMessage").and_then(Value::as_str) {
        println!("error: {message}");
    }
    let decisions = value
        .get("openDecisions")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    print_decision_hints(decisions);
    if decisions.is_empty() && matches!(phase, "waiting_for_approval" | "waiting_for_input") {
        println!("blocked: this Codex request is not supported by coco decide");
    }
}

pub(super) fn print_decision_hints(decisions: &[Value]) {
    for decision in decisions {
        let id = decision
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let kind = match decision.get("kind").and_then(Value::as_str) {
            Some("command_approval") => "command approval",
            Some("file_change_approval") => "file-change approval",
            Some("user_input") => "question from Codex",
            _ => "Codex request",
        };
        match decision.get("state").and_then(Value::as_str) {
            Some("pending") => {
                println!("decision: {id} ({kind})");
                println!("next: coco decide {id}");
            }
            Some("submitted") => println!("decision: {id} (response sent; waiting for Codex)"),
            _ => {}
        }
    }
}

pub(super) fn print_diff(value: &Value) {
    let patch = value.get("patch").and_then(Value::as_str).unwrap_or("");
    if !patch.is_empty() {
        print!("{patch}");
        if !patch.ends_with('\n') {
            println!();
        }
    }
    let untracked = value
        .get("untrackedPaths")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    if !untracked.is_empty() {
        println!("Untracked:");
        for path in untracked {
            println!("{}", compact(path));
        }
    }
    if patch.is_empty() && untracked.is_empty() {
        println!("No changes.");
    }
}

fn text(value: &Value, key: &str) -> String {
    value.get(key).map(compact).unwrap_or_else(|| "-".into())
}

fn compact(value: &Value) -> String {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| value.to_string())
}

fn human_key(value: &str) -> String {
    let mut output = String::new();
    for (index, character) in value.chars().enumerate() {
        if index > 0 && character.is_uppercase() {
            output.push('_');
        }
        output.extend(character.to_lowercase());
    }
    output
}
