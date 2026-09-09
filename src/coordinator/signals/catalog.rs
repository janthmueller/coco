use std::fs;
use std::io::Read;
use std::path::Path;

use chrono::Utc;
use serde_json::Value;

use super::schema;
use crate::coordinator::CoordinatorError;
use crate::domain::signals::{MAX_SIGNAL_BYTES, MAX_SIGNAL_TYPES, SignalType};

#[cfg(test)]
mod tests;

pub(super) fn load(
    directory: &Path,
    repository_id: &str,
) -> Result<Vec<SignalType>, CoordinatorError> {
    if !directory.is_absolute() {
        return Err(invalid("signal catalog directory must be absolute"));
    }
    let entries = fs::read_dir(directory)
        .map_err(|_| invalid("could not read the selected signal catalog directory"))?;
    let mut definitions = Vec::new();
    for (index, entry) in entries.enumerate() {
        if index >= 1024 {
            return Err(invalid("signal catalog exceeds 1024 directory entries"));
        }
        let entry = entry.map_err(|_| invalid("could not read signal catalog entry"))?;
        let path = entry.path();
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        if definitions.len() >= MAX_SIGNAL_TYPES {
            return Err(invalid("signal catalog exceeds 128 schema files"));
        }
        definitions.push(read_definition(&path, repository_id).map_err(|error| {
            invalid(format!("signal catalog file {}: {error}", path.display()))
        })?);
    }
    definitions
        .sort_by(|left, right| (&left.name, left.version).cmp(&(&right.name, right.version)));
    Ok(definitions)
}

fn read_definition(path: &Path, repository_id: &str) -> Result<SignalType, CoordinatorError> {
    let stem = path
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or_else(|| invalid("schema filename must be UTF-8"))?;
    let (name, version) = stem
        .split_once('@')
        .ok_or_else(|| invalid("schema filename must be NAME@VERSION.json"))?;
    schema::validate_name(name)?;
    let version_number = version
        .parse::<u32>()
        .ok()
        .filter(|number| *number > 0 && number.to_string() == version)
        .ok_or_else(|| {
            invalid("schema filename version must be a positive integer, without leading zeros")
        })?;
    let metadata =
        fs::symlink_metadata(path).map_err(|_| invalid("could not inspect schema file"))?;
    if !metadata.is_file() {
        return Err(invalid(
            "schema must be a regular file, not a symlink or directory",
        ));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|_| invalid("could not open schema file"))?
        .take((MAX_SIGNAL_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid("could not read schema file"))?;
    if bytes.len() > MAX_SIGNAL_BYTES {
        return Err(invalid("schema file must be at most 16 KiB"));
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| invalid("schema file is not valid JSON"))?;
    schema::compile(&value)?;
    let description = match value.get("description") {
        None => name,
        Some(Value::String(description))
            if !description.trim().is_empty() && description.len() <= 1024 =>
        {
            description
        }
        Some(_) => return Err(invalid("schema description must contain 1–1024 bytes")),
    };
    Ok(SignalType {
        repository_id: repository_id.to_owned(),
        name: name.to_owned(),
        version: version_number,
        description: description.to_owned(),
        payload_schema: Some(value),
        registered_at_ms: Utc::now().timestamp_millis(),
    })
}

fn invalid(message: impl Into<String>) -> CoordinatorError {
    CoordinatorError::InvalidParams(message.into())
}
