use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::Notify;
use tracing::{info, warn};
use uuid::Uuid;

use crate::domain::hooks::{
    GUARD_REQUEST_SCHEMA_VERSION, GuardAction, GuardErrorPolicy, GuardRequest, GuardSummary,
    HOOK_EVENT_SCHEMA_VERSION, HookDispatch, HookEvent, HookEventKind, HookRegistrySummary,
    HookRepository, HookSummary, HookTarget, HookWorkspace, MAX_GUARD_TIMEOUT_SECONDS, MAX_GUARDS,
    MAX_HOOK_ATTEMPTS, MAX_HOOK_COMMAND_BYTES, MAX_HOOK_COMMAND_PARTS, MAX_HOOK_CONFIG_BYTES,
    MAX_HOOK_TIMEOUT_SECONDS, MAX_HOOKS,
};
use crate::domain::signals::valid_signal_name;
use crate::domain::{Repository, Workspace};

mod runner;

pub(crate) use runner::run_dispatcher;

const DEFAULT_TIMEOUT_SECONDS: u64 = 30;
const DEFAULT_MAX_ATTEMPTS: u32 = 3;
const DEFAULT_GUARD_TIMEOUT_SECONDS: u64 = 5;

#[derive(Debug, Error)]
pub(crate) enum HookConfigError {
    #[error("could not inspect hook configuration {path}: {source}")]
    Inspect {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("refusing hook configuration symlink or non-regular file: {0}")]
    UnsafePath(PathBuf),
    #[error("hook configuration {0} must not be writable by group or other users")]
    UnsafePermissions(PathBuf),
    #[error("hook configuration exceeds 64 KiB: {0}")]
    TooLarge(PathBuf),
    #[error("could not read hook configuration {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid hook configuration {path}: {message}")]
    Invalid { path: PathBuf, message: String },
}

#[derive(Debug, Error)]
pub(crate) enum GuardRejection {
    #[error("guard {guard_id:?} denied {action}: {reason}", action = action.as_str())]
    Denied {
        guard_id: String,
        action: GuardAction,
        reason: String,
    },
    #[error(
        "guard {guard_id:?} failed closed for {action}: {reason}",
        action = action.as_str()
    )]
    FailedClosed {
        guard_id: String,
        action: GuardAction,
        reason: String,
    },
}

impl GuardRejection {
    pub(crate) const fn code(&self) -> &'static str {
        match self {
            Self::Denied { .. } => "GUARD_DENIED",
            Self::FailedClosed { .. } => "GUARD_FAILED_CLOSED",
        }
    }

    pub(crate) fn details(&self) -> (&str, GuardAction, &str) {
        match self {
            Self::Denied {
                guard_id,
                action,
                reason,
            }
            | Self::FailedClosed {
                guard_id,
                action,
                reason,
            } => (guard_id, *action, reason),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HookConfigFile {
    version: u32,
    #[serde(default)]
    hooks: Vec<HookConfigEntry>,
    #[serde(default)]
    guards: Vec<GuardConfigEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HookConfigEntry {
    id: String,
    event: HookEventKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    signal: Option<String>,
    command: Vec<String>,
    #[serde(default = "default_timeout_seconds")]
    timeout_seconds: u64,
    #[serde(default = "default_max_attempts")]
    max_attempts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GuardConfigEntry {
    id: String,
    action: GuardAction,
    command: Vec<String>,
    #[serde(default = "default_guard_timeout_seconds")]
    timeout_seconds: u64,
    on_error: GuardErrorPolicy,
}

const fn default_timeout_seconds() -> u64 {
    DEFAULT_TIMEOUT_SECONDS
}

const fn default_max_attempts() -> u32 {
    DEFAULT_MAX_ATTEMPTS
}

const fn default_guard_timeout_seconds() -> u64 {
    DEFAULT_GUARD_TIMEOUT_SECONDS
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SignalSelector {
    name: String,
    version: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct HookDefinition {
    summary: HookSummary,
    definition_hash: String,
    command: Vec<String>,
    signal: Option<SignalSelector>,
}

#[derive(Debug, Clone)]
pub(crate) struct GuardDefinition {
    summary: GuardSummary,
    command: Vec<String>,
}

impl GuardDefinition {
    pub(crate) fn command(&self) -> &[String] {
        &self.command
    }

    pub(crate) const fn timeout_seconds(&self) -> u64 {
        self.summary.timeout_seconds
    }

    pub(crate) const fn on_error(&self) -> GuardErrorPolicy {
        self.summary.on_error
    }

    pub(crate) fn id(&self) -> &str {
        &self.summary.id
    }
}

#[derive(Debug)]
struct HookSnapshot {
    definitions: BTreeMap<String, HookDefinition>,
    guards: BTreeMap<String, GuardDefinition>,
    working_directory: PathBuf,
}

impl HookSnapshot {
    fn summary(&self) -> HookRegistrySummary {
        HookRegistrySummary {
            hooks: self
                .definitions
                .values()
                .map(|definition| definition.summary.clone())
                .collect(),
            guards: self
                .guards
                .values()
                .map(|definition| definition.summary.clone())
                .collect(),
        }
    }
}

impl HookDefinition {
    pub(crate) fn command(&self) -> &[String] {
        &self.command
    }

    pub(crate) const fn timeout_seconds(&self) -> u64 {
        self.summary.timeout_seconds
    }

    pub(crate) const fn max_attempts(&self) -> u32 {
        self.summary.max_attempts
    }

    fn matches(&self, event: &HookEvent) -> bool {
        if self.summary.event != event.kind {
            return false;
        }
        let Some(selector) = &self.signal else {
            return true;
        };
        event.data.get("name").and_then(Value::as_str) == Some(selector.name.as_str())
            && event.data.get("version").and_then(Value::as_u64)
                == Some(u64::from(selector.version))
    }
}

#[derive(Debug)]
pub(crate) struct HookRegistry {
    config_path: PathBuf,
    snapshot: RwLock<HookSnapshot>,
    wake: Notify,
}

impl HookRegistry {
    pub(crate) fn load(path: &Path) -> Result<Self, HookConfigError> {
        let snapshot = load_snapshot(path)?;
        Ok(Self {
            config_path: path.to_owned(),
            snapshot: RwLock::new(snapshot),
            wake: Notify::new(),
        })
    }

    pub(crate) fn validate(path: &Path) -> Result<HookRegistrySummary, HookConfigError> {
        Ok(load_snapshot(path)?.summary())
    }

    pub(crate) fn reload(&self) -> Result<HookRegistrySummary, HookConfigError> {
        let replacement = load_snapshot(&self.config_path)?;
        let summary = replacement.summary();
        *self
            .snapshot
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = replacement;
        self.notify();
        Ok(summary)
    }

    #[cfg(test)]
    pub(crate) fn empty() -> Self {
        Self {
            config_path: PathBuf::new(),
            snapshot: RwLock::new(empty_snapshot(PathBuf::new())),
            wake: Notify::new(),
        }
    }

    pub(crate) fn summary(&self) -> HookRegistrySummary {
        self.snapshot
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .summary()
    }

    pub(crate) fn definition(&self, id: &str, hash: &str) -> Option<(HookDefinition, PathBuf)> {
        let snapshot = self
            .snapshot
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        snapshot
            .definitions
            .get(id)
            .filter(|definition| definition.definition_hash == hash)
            .cloned()
            .map(|definition| (definition, snapshot.working_directory.clone()))
    }

    pub(crate) fn event(
        &self,
        kind: HookEventKind,
        repository: &Repository,
        workspace: &Workspace,
        data: Value,
    ) -> Option<HookDispatch> {
        let event = HookEvent {
            schema_version: HOOK_EVENT_SCHEMA_VERSION,
            id: Uuid::now_v7().to_string(),
            kind,
            occurred_at_ms: Utc::now().timestamp_millis(),
            repository: hook_repository(repository),
            workspace: hook_workspace(workspace),
            data,
        };
        let snapshot = self
            .snapshot
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let targets = snapshot
            .definitions
            .values()
            .filter(|definition| definition.matches(&event))
            .map(|definition| HookTarget {
                hook_id: definition.summary.id.clone(),
                definition_hash: definition.definition_hash.clone(),
            })
            .collect::<Vec<_>>();
        (!targets.is_empty()).then_some(HookDispatch { event, targets })
    }

    pub(crate) async fn check_guards(
        &self,
        action: GuardAction,
        repository: &Repository,
        workspace: &Workspace,
        data: Value,
    ) -> Result<(), GuardRejection> {
        let (guards, working_directory) = {
            let snapshot = self
                .snapshot
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            (
                snapshot
                    .guards
                    .values()
                    .filter(|definition| definition.summary.action == action)
                    .cloned()
                    .collect::<Vec<_>>(),
                snapshot.working_directory.clone(),
            )
        };
        if guards.is_empty() {
            return Ok(());
        }
        let request = GuardRequest {
            schema_version: GUARD_REQUEST_SCHEMA_VERSION,
            id: Uuid::now_v7().to_string(),
            action,
            requested_at_ms: Utc::now().timestamp_millis(),
            repository: hook_repository(repository),
            workspace: hook_workspace(workspace),
            data,
        };
        let body = serde_json::to_vec(&request).expect("guard request is JSON serializable");
        for guard in guards {
            match runner::run_guard(&guard, &working_directory, &body).await {
                Ok(runner::GuardDecision::Allow) => {
                    info!(
                        guard_id = guard.id(),
                        action = action.as_str(),
                        "guard allowed action"
                    );
                }
                Ok(runner::GuardDecision::Deny(reason)) => {
                    return Err(GuardRejection::Denied {
                        guard_id: guard.id().to_owned(),
                        action,
                        reason,
                    });
                }
                Err(reason) if guard.on_error() == GuardErrorPolicy::Allow => {
                    warn!(
                        guard_id = guard.id(),
                        action = action.as_str(),
                        %reason,
                        "guard failed open"
                    );
                }
                Err(reason) => {
                    return Err(GuardRejection::FailedClosed {
                        guard_id: guard.id().to_owned(),
                        action,
                        reason,
                    });
                }
            }
        }
        Ok(())
    }

    pub(crate) fn notify(&self) {
        self.wake.notify_one();
    }

    pub(crate) async fn notified(&self) {
        self.wake.notified().await;
    }
}

fn load_snapshot(path: &Path) -> Result<HookSnapshot, HookConfigError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            return Ok(empty_snapshot(config_parent(path)));
        }
        Err(source) => {
            return Err(HookConfigError::Inspect {
                path: path.to_owned(),
                source,
            });
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(HookConfigError::UnsafePath(path.to_owned()));
    }
    #[cfg(unix)]
    if metadata.permissions().mode() & 0o022 != 0 {
        return Err(HookConfigError::UnsafePermissions(path.to_owned()));
    }
    if metadata.len() > MAX_HOOK_CONFIG_BYTES {
        return Err(HookConfigError::TooLarge(path.to_owned()));
    }
    let canonical_path = fs::canonicalize(path).map_err(|source| HookConfigError::Inspect {
        path: path.to_owned(),
        source,
    })?;
    let bytes = fs::read(path).map_err(|source| HookConfigError::Read {
        path: path.to_owned(),
        source,
    })?;
    let config: HookConfigFile =
        serde_json::from_slice(&bytes).map_err(|source| HookConfigError::Invalid {
            path: path.to_owned(),
            message: source.to_string(),
        })?;
    validate_config(path, config_parent(&canonical_path), config)
}

fn empty_snapshot(working_directory: PathBuf) -> HookSnapshot {
    HookSnapshot {
        definitions: BTreeMap::new(),
        guards: BTreeMap::new(),
        working_directory,
    }
}

fn hook_repository(repository: &Repository) -> HookRepository {
    HookRepository {
        id: repository.id.clone(),
        name: repository.display_name.clone(),
        path: repository.root_path.clone(),
    }
}

fn hook_workspace(workspace: &Workspace) -> HookWorkspace {
    HookWorkspace {
        id: workspace.id.clone(),
        name: workspace.name.clone(),
        thread_id: workspace.codex_thread_id.clone(),
        worktree_path: workspace.worktree_path.clone(),
        branch_name: workspace.branch_name.clone(),
    }
}

fn validate_config(
    path: &Path,
    working_directory: PathBuf,
    config: HookConfigFile,
) -> Result<HookSnapshot, HookConfigError> {
    if config.version != 1 {
        return invalid(path, "hook configuration version must be 1");
    }
    if config.hooks.len() > MAX_HOOKS {
        return invalid(path, "at most 64 hooks are supported");
    }
    if config.guards.len() > MAX_GUARDS {
        return invalid(path, "at most 64 guards are supported");
    }
    let mut ids = HashSet::with_capacity(config.hooks.len() + config.guards.len());
    let mut definitions = BTreeMap::new();
    for entry in config.hooks {
        validate_hook_id(path, &entry.id)?;
        if !ids.insert(entry.id.clone()) {
            return invalid(path, format!("duplicate hook or guard id {:?}", entry.id));
        }
        validate_command(path, "hook", &entry.id, &entry.command)?;
        if !(1..=MAX_HOOK_TIMEOUT_SECONDS).contains(&entry.timeout_seconds) {
            return invalid(
                path,
                format!("hook {:?} timeoutSeconds must be 1–300", entry.id),
            );
        }
        if !(1..=MAX_HOOK_ATTEMPTS).contains(&entry.max_attempts) {
            return invalid(path, format!("hook {:?} maxAttempts must be 1–5", entry.id));
        }
        let signal = entry
            .signal
            .as_deref()
            .map(|value| parse_signal_selector(path, &entry.id, value))
            .transpose()?;
        if signal.is_some() && entry.event != HookEventKind::SignalEmitted {
            return invalid(
                path,
                format!(
                    "hook {:?} may use signal only with event signal.emitted",
                    entry.id
                ),
            );
        }
        let encoded = serde_json::to_vec(&entry).map_err(|source| HookConfigError::Invalid {
            path: path.to_owned(),
            message: source.to_string(),
        })?;
        let mut hasher = Sha256::new();
        update_path_hash(&mut hasher, &working_directory);
        hasher.update([0]);
        hasher.update(encoded);
        let definition_hash = format!("sha256:{}", hex::encode(hasher.finalize()));
        let summary = HookSummary {
            id: entry.id.clone(),
            event: entry.event,
            signal: entry.signal.clone(),
            timeout_seconds: entry.timeout_seconds,
            max_attempts: entry.max_attempts,
        };
        definitions.insert(
            entry.id.clone(),
            HookDefinition {
                summary,
                definition_hash,
                command: entry.command,
                signal,
            },
        );
    }
    let mut guards = BTreeMap::new();
    for entry in config.guards {
        validate_hook_id(path, &entry.id)?;
        if !ids.insert(entry.id.clone()) {
            return invalid(path, format!("duplicate hook or guard id {:?}", entry.id));
        }
        validate_command(path, "guard", &entry.id, &entry.command)?;
        if !(1..=MAX_GUARD_TIMEOUT_SECONDS).contains(&entry.timeout_seconds) {
            return invalid(
                path,
                format!("guard {:?} timeoutSeconds must be 1–30", entry.id),
            );
        }
        let summary = GuardSummary {
            id: entry.id.clone(),
            action: entry.action,
            timeout_seconds: entry.timeout_seconds,
            on_error: entry.on_error,
        };
        guards.insert(
            entry.id,
            GuardDefinition {
                summary,
                command: entry.command,
            },
        );
    }
    Ok(HookSnapshot {
        definitions,
        guards,
        working_directory,
    })
}

fn validate_hook_id(path: &Path, id: &str) -> Result<(), HookConfigError> {
    let valid = !id.is_empty()
        && id.len() <= 64
        && id.starts_with(|character: char| character.is_ascii_lowercase())
        && id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        });
    if valid {
        Ok(())
    } else {
        invalid(
            path,
            "hook id must start with a lowercase letter and contain at most 64 lowercase letters, digits, dots, underscores, or hyphens",
        )
    }
}

fn validate_command(
    path: &Path,
    kind: &str,
    id: &str,
    command: &[String],
) -> Result<(), HookConfigError> {
    if command.is_empty() || command.len() > MAX_HOOK_COMMAND_PARTS {
        return invalid(
            path,
            format!("{kind} {id:?} command must contain 1–33 parts"),
        );
    }
    let total = command.iter().map(String::len).sum::<usize>();
    if total > MAX_HOOK_COMMAND_BYTES || command.iter().any(String::is_empty) {
        return invalid(
            path,
            format!("{kind} {id:?} command parts must be non-empty and total at most 16 KiB"),
        );
    }
    let executable = Path::new(&command[0]);
    if !executable.is_absolute() {
        return invalid(
            path,
            format!("{kind} {id:?} executable must be an absolute path"),
        );
    }
    let metadata = fs::metadata(executable).map_err(|source| HookConfigError::Invalid {
        path: path.to_owned(),
        message: format!(
            "{kind} {id:?} executable {} is unavailable: {source}",
            executable.display()
        ),
    })?;
    if !metadata.is_file() {
        return invalid(
            path,
            format!(
                "{kind} {id:?} executable is not a regular file: {}",
                executable.display()
            ),
        );
    }
    #[cfg(unix)]
    if metadata.permissions().mode() & 0o111 == 0 {
        return invalid(
            path,
            format!(
                "{kind} {id:?} executable is not marked executable: {}",
                executable.display()
            ),
        );
    }
    Ok(())
}

fn parse_signal_selector(
    path: &Path,
    hook_id: &str,
    value: &str,
) -> Result<SignalSelector, HookConfigError> {
    let Some((name, version)) = value.rsplit_once('@') else {
        return invalid(
            path,
            format!("hook {hook_id:?} signal must use NAME@VERSION"),
        );
    };
    let version = version.parse::<u32>().ok().filter(|value| *value > 0);
    if !valid_signal_name(name)
        || version.is_none()
        || value.ends_with("@0")
        || version.is_some_and(|parsed| format!("{name}@{parsed}") != value)
    {
        return invalid(
            path,
            format!("hook {hook_id:?} signal must use canonical NAME@VERSION"),
        );
    }
    Ok(SignalSelector {
        name: name.to_owned(),
        version: version.expect("checked positive signal version"),
    })
}

fn config_parent(path: &Path) -> PathBuf {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_owned()
}

#[cfg(unix)]
fn update_path_hash(hasher: &mut Sha256, path: &Path) {
    use std::os::unix::ffi::OsStrExt;

    hasher.update(path.as_os_str().as_bytes());
}

#[cfg(not(unix))]
fn update_path_hash(hasher: &mut Sha256, path: &Path) {
    hasher.update(path.to_string_lossy().as_bytes());
}

fn invalid<T>(path: &Path, message: impl Into<String>) -> Result<T, HookConfigError> {
    Err(HookConfigError::Invalid {
        path: path.to_owned(),
        message: message.into(),
    })
}

#[cfg(test)]
mod tests;
