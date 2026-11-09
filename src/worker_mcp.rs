//! Bounded, credential-free worker recipes. Only the reference is durable.
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::Read;

use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::domain::setup::{WorkerMcpReference, worker_slot_key};

pub(crate) const MAX_RECIPE_BYTES: usize = 64 * 1024;

#[derive(Debug, Error)]
#[error("worker MCP binding is unavailable or invalid: {0}")]
pub(crate) struct WorkerMcpError(pub(crate) &'static str);

// Deliberately no Debug/Serialize: command/args must not enter diagnostics.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Recipe {
    mcp_servers: BTreeMap<String, Server>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Server {
    command: String,
    args: Vec<String>,
    enabled_tools: Vec<String>,
    default_tools_approval_mode: ApprovalMode,
    required: bool,
}

#[derive(Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
enum ApprovalMode {
    Auto,
    Prompt,
    Writes,
    Approve,
}

pub(crate) struct LoadedWorkerMcp {
    recipe: Recipe,
}

impl LoadedWorkerMcp {
    pub(crate) fn server_names(&self) -> Vec<String> {
        self.recipe.mcp_servers.keys().cloned().collect()
    }

    /// Reject profile collisions instead of relying on Codex's recursive merge.
    pub(crate) fn apply(&self, config: &mut Value) -> Result<(), WorkerMcpError> {
        let object = config
            .as_object_mut()
            .ok_or(WorkerMcpError("invalid profile"))?;
        if object.keys().any(|key| key.starts_with("mcp_servers.")) {
            return Err(WorkerMcpError("profile has dotted MCP overrides"));
        }
        let servers = object.entry("mcp_servers").or_insert_with(|| json!({}));
        let servers = servers
            .as_object_mut()
            .ok_or(WorkerMcpError("invalid profile MCP map"))?;
        if self.recipe.mcp_servers.keys().any(|key| {
            servers
                .keys()
                .any(|existing| worker_slot_key(existing) == worker_slot_key(key))
        }) {
            return Err(WorkerMcpError(
                "server position conflicts with the execution profile",
            ));
        }
        for (name, server) in &self.recipe.mcp_servers {
            servers.insert(
                name.clone(),
                json!({
                    "command": server.command, "args": server.args,
                    "enabled_tools": server.enabled_tools,
                    "default_tools_approval_mode": server.default_tools_approval_mode,
                    "required": server.required,
                }),
            );
        }
        Ok(())
    }
}

pub(crate) fn load(reference: &WorkerMcpReference) -> Result<LoadedWorkerMcp, WorkerMcpError> {
    validate_reference(reference)?;
    let path = &reference.path;
    let before =
        fs::symlink_metadata(path).map_err(|_| WorkerMcpError("source missing or unreadable"))?;
    if !before.is_file() || fs::canonicalize(path).ok().as_ref() != Some(path) {
        return Err(WorkerMcpError(
            "source must be a canonical regular file without symlinks",
        ));
    }
    let file = open_source(reference)?;
    let opened = file
        .metadata()
        .map_err(|_| WorkerMcpError("source unreadable"))?;
    check_file(&opened)?;
    let mut bytes = Vec::new();
    file.take((MAX_RECIPE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| WorkerMcpError("source unreadable"))?;
    if bytes.len() > MAX_RECIPE_BYTES {
        return Err(WorkerMcpError("source exceeds 64 KiB"));
    }
    let after =
        fs::symlink_metadata(path).map_err(|_| WorkerMcpError("source changed during read"))?;
    check_file(&after)?;
    if !after.is_file()
        || !same_file(&before, &opened)
        || !same_file(&opened, &after)
        || fs::canonicalize(path).ok().as_ref() != Some(path)
    {
        return Err(WorkerMcpError("source changed during read"));
    }
    if hex::encode(Sha256::digest(&bytes)) != reference.sha256 {
        return Err(WorkerMcpError("source SHA-256 mismatch"));
    }
    // Do not include serde diagnostics: they can quote command/argument values.
    let recipe: Recipe =
        serde_json::from_slice(&bytes).map_err(|_| WorkerMcpError("invalid recipe schema"))?;
    validate_recipe(&recipe)?;
    Ok(LoadedWorkerMcp { recipe })
}

pub(crate) fn validate_reference(reference: &WorkerMcpReference) -> Result<(), WorkerMcpError> {
    if !reference.path.is_absolute()
        || reference.path.to_str().is_none_or(|s| s.len() > 4096)
        || reference.sha256.len() != 64
        || !reference
            .sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(WorkerMcpError(
            "expected an absolute UTF-8 path and lowercase SHA-256",
        ));
    }
    Ok(())
}

fn open_source(reference: &WorkerMcpReference) -> Result<File, WorkerMcpError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    options
        .open(&reference.path)
        .map_err(|_| WorkerMcpError("source cannot be opened safely"))
}

fn check_file(metadata: &fs::Metadata) -> Result<(), WorkerMcpError> {
    if !metadata.is_file() || metadata.len() > MAX_RECIPE_BYTES as u64 {
        return Err(WorkerMcpError(
            "source must be a regular file of at most 64 KiB",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(WorkerMcpError("source must be private (mode 0600 or 0400)"));
        }
    }
    Ok(())
}

fn same_file(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        left.dev() == right.dev()
            && left.ino() == right.ino()
            && left.len() == right.len()
            && left.modified().ok() == right.modified().ok()
    }
    #[cfg(not(unix))]
    {
        left.len() == right.len() && left.modified().ok() == right.modified().ok()
    }
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
}

fn validate_recipe(recipe: &Recipe) -> Result<(), WorkerMcpError> {
    if recipe.mcp_servers.is_empty() || recipe.mcp_servers.len() > 16 {
        return Err(WorkerMcpError("expected 1 to 16 servers"));
    }
    if recipe
        .mcp_servers
        .keys()
        .map(|key| worker_slot_key(key))
        .collect::<BTreeSet<_>>()
        .len()
        != recipe.mcp_servers.len()
    {
        return Err(WorkerMcpError(
            "server positions have ambiguous tool namespaces",
        ));
    }
    for (name, server) in &recipe.mcp_servers {
        if !identifier(name)
            || server.command.is_empty()
            || server.command.len() > 4096
            || server.command.contains(['\0', '\n', '\r'])
            || server.args.len() > 128
            || server
                .args
                .iter()
                .any(|arg| arg.len() > 4096 || arg.contains('\0'))
            || server.enabled_tools.is_empty()
            || server.enabled_tools.len() > 128
            || server.enabled_tools.iter().any(|tool| {
                tool.is_empty()
                    || tool.len() > 128
                    || !tool
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
            })
        {
            return Err(WorkerMcpError(
                "recipe exceeds field bounds or has an invalid identifier",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
