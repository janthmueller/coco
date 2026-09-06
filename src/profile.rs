use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::domain::ProfileSnapshot;

pub const DEFAULT_PROFILE_NAME: &str = "default";
pub const MAX_PROFILE_NAME_BYTES: usize = 64;
pub const MAX_PROFILE_BYTES: usize = 1024 * 1024;

/// A runtime profile resolution. `thread_config` can contain credentials or
/// other private configuration and must not be persisted. Persist only the
/// deliberately redacted `snapshot`.
#[derive(Clone, PartialEq)]
pub struct LoadedProfile {
    pub thread_config: Value,
    pub snapshot: ProfileSnapshot,
}

impl std::fmt::Debug for LoadedProfile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LoadedProfile")
            .field("thread_config", &"<redacted>")
            .field("snapshot", &self.snapshot)
            .finish()
    }
}

#[derive(Debug, Error)]
pub enum ProfileError {
    #[error("invalid profile name {name:?}: {reason}")]
    InvalidName { name: String, reason: &'static str },

    #[error("CODEX_HOME must be an absolute path, received {0}")]
    RelativeCodexHome(PathBuf),

    #[error("profile file for {name:?} does not exist at {path}")]
    NotFound { name: String, path: PathBuf },

    #[error("profile source is not a regular file: {0}")]
    NotAFile(PathBuf),

    #[error("profile source at {path} exceeds the {limit}-byte limit")]
    TooLarge { path: PathBuf, limit: usize },

    #[error("could not read profile source at {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("could not parse profile source at {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("profile source at {path} is not valid UTF-8: {source}")]
    Utf8 {
        path: PathBuf,
        #[source]
        source: std::str::Utf8Error,
    },

    #[error("profile source at {path} cannot be represented as JSON: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("profile source at {0} did not decode to a top-level JSON object")]
    NotAnObject(PathBuf),
}

/// Accepts a deliberately small filename-safe profile-name alphabet. This is
/// stricter than merely removing path separators: it also rules out hidden
/// files, option-looking names, platform-specific separators, and ambiguous
/// Unicode normalization.
pub fn validate_profile_name(name: &str) -> Result<(), ProfileError> {
    if name.is_empty() {
        return Err(invalid_name(name, "name must not be empty"));
    }
    if name.len() > MAX_PROFILE_NAME_BYTES {
        return Err(invalid_name(name, "name is longer than 64 bytes"));
    }

    let mut bytes = name.bytes();
    let first = bytes
        .next()
        .expect("the empty profile name was rejected above");
    if !first.is_ascii_alphanumeric() {
        return Err(invalid_name(
            name,
            "name must start with an ASCII letter or digit",
        ));
    }
    if !bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')) {
        return Err(invalid_name(
            name,
            "name may contain only ASCII letters, digits, '-' and '_'",
        ));
    }
    Ok(())
}

/// Resolves a Codex profile before any repository mutation is attempted.
///
/// `default` deliberately supplies an empty per-thread overlay: the app-server
/// keeps using the base `$CODEX_HOME/config.toml` that it loaded itself. Any
/// other safe name loads the complete `$CODEX_HOME/<name>.config.toml` file,
/// matching `codex --profile <name>`, and converts it into the JSON object
/// accepted by `thread/start.config`.
pub fn load_profile(name: &str, codex_home: &Path) -> Result<LoadedProfile, ProfileError> {
    validate_profile_name(name)?;
    if !codex_home.is_absolute() {
        return Err(ProfileError::RelativeCodexHome(codex_home.to_path_buf()));
    }

    if name == DEFAULT_PROFILE_NAME {
        let thread_config = Value::Object(Map::new());
        return Ok(LoadedProfile {
            snapshot: ProfileSnapshot {
                name: name.to_owned(),
                source_path: None,
                source_hash: sha256_hex(&[]),
                model_override: None,
                effective_settings: Value::Object(Map::new()),
            },
            thread_config,
        });
    }

    let source_path = profile_path(codex_home, name)?;
    let bytes = read_bounded(&source_path, name)?;
    let text = std::str::from_utf8(&bytes).map_err(|source| ProfileError::Utf8 {
        path: source_path.clone(),
        source,
    })?;
    let parsed = toml::from_str::<toml::Value>(text).map_err(|source| ProfileError::Parse {
        path: source_path.clone(),
        source,
    })?;
    let thread_config = serde_json::to_value(parsed).map_err(|source| ProfileError::Json {
        path: source_path.clone(),
        source,
    })?;
    if !thread_config.is_object() {
        // A TOML document is currently always a table. Keep this guard local
        // so a future parser behavior change cannot send a non-object config.
        return Err(ProfileError::NotAnObject(source_path));
    }

    let effective_settings = non_secret_effective_settings(&thread_config);
    let profile_bytes =
        serde_json::to_vec(&thread_config).map_err(|source| ProfileError::Json {
            path: source_path.clone(),
            source,
        })?;
    Ok(LoadedProfile {
        snapshot: ProfileSnapshot {
            name: name.to_owned(),
            source_path: Some(source_path),
            source_hash: sha256_hex(&profile_bytes),
            model_override: None,
            effective_settings,
        },
        thread_config,
    })
}

pub fn profile_path(codex_home: &Path, name: &str) -> Result<PathBuf, ProfileError> {
    validate_profile_name(name)?;
    if !codex_home.is_absolute() {
        return Err(ProfileError::RelativeCodexHome(codex_home.to_path_buf()));
    }
    Ok(codex_home.join(format!("{name}.config.toml")))
}

/// Replaces configured hints with the non-secret settings the App Server says
/// it actually applied to the newly created thread.
pub fn with_effective_thread_settings(
    mut snapshot: ProfileSnapshot,
    thread_start_response: &Value,
) -> ProfileSnapshot {
    const SAFE_RESPONSE_KEYS: &[&str] = &[
        "activePermissionProfile",
        "approvalPolicy",
        "approvalsReviewer",
        "model",
        "modelProvider",
        "reasoningEffort",
        "sandbox",
        "serviceTier",
    ];

    let mut effective = Map::new();
    if let Some(response) = thread_start_response.as_object() {
        for key in SAFE_RESPONSE_KEYS {
            if let Some(value) = response.get(*key) {
                effective.insert((*key).to_owned(), value.clone());
            }
        }
    }
    snapshot.effective_settings = Value::Object(effective);
    snapshot
}

fn invalid_name(name: &str, reason: &'static str) -> ProfileError {
    ProfileError::InvalidName {
        name: name.to_owned(),
        reason,
    }
}

fn read_bounded(path: &Path, name: &str) -> Result<Vec<u8>, ProfileError> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            return Err(ProfileError::NotFound {
                name: name.to_owned(),
                path: path.to_path_buf(),
            });
        }
        Err(source) => {
            return Err(ProfileError::Read {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    let metadata = file.metadata().map_err(|source| ProfileError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    if !metadata.is_file() {
        return Err(ProfileError::NotAFile(path.to_path_buf()));
    }
    if metadata.len() > MAX_PROFILE_BYTES as u64 {
        return Err(ProfileError::TooLarge {
            path: path.to_path_buf(),
            limit: MAX_PROFILE_BYTES,
        });
    }

    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_PROFILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|source| ProfileError::Read {
            path: path.to_path_buf(),
            source,
        })?;
    if bytes.len() > MAX_PROFILE_BYTES {
        return Err(ProfileError::TooLarge {
            path: path.to_path_buf(),
            limit: MAX_PROFILE_BYTES,
        });
    }
    Ok(bytes)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn non_secret_effective_settings(config: &Value) -> Value {
    const SAFE_KEYS: &[&str] = &[
        "approval_policy",
        "model",
        "model_provider",
        "model_reasoning_effort",
        "model_reasoning_summary",
        "model_verbosity",
        "personality",
        "sandbox_mode",
        "service_tier",
        "web_search",
    ];
    const INSTRUCTION_KEYS: &[&str] = &[
        "base_instructions",
        "developer_instructions",
        "instructions",
    ];

    let Some(config) = config.as_object() else {
        return Value::Object(Map::new());
    };
    let mut safe = Map::new();
    for key in SAFE_KEYS {
        if let Some(value) = config.get(*key).filter(|value| is_safe_scalar(value)) {
            safe.insert((*key).to_owned(), value.clone());
        }
    }
    for key in INSTRUCTION_KEYS {
        if config.contains_key(*key) {
            safe.insert(format!("{key}_configured"), Value::Bool(true));
        }
    }
    Value::Object(safe)
}

fn is_safe_scalar(value: &Value) -> bool {
    matches!(
        value,
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_)
    )
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use serde_json::json;
    use tempfile::tempdir;

    use super::{
        DEFAULT_PROFILE_NAME, ProfileError, load_profile, profile_path, validate_profile_name,
        with_effective_thread_settings,
    };

    #[test]
    fn validates_filename_safe_profile_names() {
        for valid in [DEFAULT_PROFILE_NAME, "backend", "gpt_5-high", "Worker42"] {
            validate_profile_name(valid).unwrap();
        }
        for invalid in [
            "",
            ".",
            "..",
            "../admin",
            "/absolute",
            "nested/name",
            r"nested\name",
            "-option",
            ".hidden",
            "two words",
            "unicode-é",
        ] {
            assert!(
                matches!(
                    validate_profile_name(invalid),
                    Err(ProfileError::InvalidName { .. })
                ),
                "{invalid:?} should be rejected"
            );
        }
    }

    #[test]
    fn default_uses_the_base_config_without_an_overlay() {
        let home = tempdir().unwrap();
        let loaded = load_profile(DEFAULT_PROFILE_NAME, home.path()).unwrap();

        assert_eq!(loaded.thread_config, json!({}));
        assert_eq!(loaded.snapshot.name, DEFAULT_PROFILE_NAME);
        assert_eq!(loaded.snapshot.source_path, None);
        assert_eq!(
            loaded.snapshot.source_hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(loaded.snapshot.effective_settings, json!({}));
    }

    #[test]
    fn loads_the_complete_named_codex_profile_as_thread_config() {
        let home = tempdir().unwrap();
        let path = profile_path(home.path(), "backend").unwrap();
        assert_eq!(path, home.path().join("backend.config.toml"));
        let source = concat!(
            "model = \"gpt-5.6-codex\"\n",
            "model_reasoning_effort = \"high\"\n",
            "sandbox_mode = \"workspace-write\"\n",
            "instructions = \"private operating context\"\n",
            "api_key = \"super-secret\"\n",
            "[mcp_servers.internal]\n",
            "command = \"internal-server\"\n",
            "[mcp_servers.internal.env]\n",
            "TOKEN = \"also-secret\"\n",
        );
        fs::write(&path, source).unwrap();

        let loaded = load_profile("backend", home.path()).unwrap();

        assert_eq!(loaded.snapshot.source_path.as_deref(), Some(path.as_path()));
        assert_eq!(loaded.snapshot.source_hash.len(), 64);
        assert_eq!(loaded.thread_config["model"], "gpt-5.6-codex");
        assert_eq!(loaded.thread_config["api_key"], "super-secret");
        assert_eq!(
            loaded.thread_config["mcp_servers"]["internal"]["env"]["TOKEN"],
            "also-secret"
        );
        assert_eq!(
            loaded.snapshot.effective_settings,
            json!({
                "instructions_configured": true,
                "model": "gpt-5.6-codex",
                "model_reasoning_effort": "high",
                "sandbox_mode": "workspace-write",
            })
        );
        let persisted = serde_json::to_string(&loaded.snapshot).unwrap();
        assert!(!persisted.contains("super-secret"));
        assert!(!persisted.contains("also-secret"));
        assert!(!persisted.contains("private operating context"));
    }

    #[test]
    fn rejects_invalid_toml_with_its_source_path() {
        let home = tempdir().unwrap();
        let path = profile_path(home.path(), "broken").unwrap();
        fs::write(&path, "model = [").unwrap();

        match load_profile("broken", home.path()) {
            Err(ProfileError::Parse {
                path: error_path, ..
            }) => assert_eq!(error_path, path),
            other => panic!("expected parse error, got {other:?}"),
        }
    }

    #[test]
    fn does_not_treat_a_legacy_profile_table_as_a_named_profile_file() {
        let home = tempdir().unwrap();
        let path = profile_path(home.path(), "missing").unwrap();
        fs::write(
            home.path().join("config.toml"),
            "[profiles.missing]\nmodel = \"gpt-5\"\n",
        )
        .unwrap();

        let error = load_profile("missing", home.path()).unwrap_err();
        assert!(matches!(
            &error,
            ProfileError::NotFound { name, path: error_path }
                if name == "missing" && error_path == &path
        ));
        assert_eq!(
            error.to_string(),
            format!(
                "profile file for \"missing\" does not exist at {}",
                path.display()
            )
        );
    }

    #[test]
    fn snapshots_only_known_effective_thread_settings() {
        let loaded = load_profile(DEFAULT_PROFILE_NAME, Path::new("/tmp")).unwrap();
        let snapshot = with_effective_thread_settings(
            loaded.snapshot,
            &json!({
                "model": "gpt-5.6-codex",
                "approvalPolicy": "on-request",
                "activePermissionProfile": {"id": ":workspace"},
                "runtimeWorkspaceRoots": ["/secret/path"],
                "token": "must-not-persist",
            }),
        );

        assert_eq!(
            snapshot.effective_settings,
            json!({
                "activePermissionProfile": {"id": ":workspace"},
                "approvalPolicy": "on-request",
                "model": "gpt-5.6-codex",
            })
        );
    }
}
