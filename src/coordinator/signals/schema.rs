use crate::coordinator::CoordinatorError;
use crate::domain::signals::{MAX_SIGNAL_BYTES, valid_signal_name};
use jsonschema::{Draft, Validator};
use serde_json::Value;

pub(super) fn validate_name(name: &str) -> Result<(), CoordinatorError> {
    if !valid_signal_name(name) {
        return invalid(
            "signal name must start with a lowercase letter and contain at most 64 lowercase letters, digits, dots, underscores or hyphens",
        );
    }
    Ok(())
}

pub(super) fn validate_version(version: u32) -> Result<(), CoordinatorError> {
    if version == 0 {
        return invalid("signal version must be positive");
    }
    Ok(())
}

pub(super) fn validate_json(value: &Value) -> Result<(), CoordinatorError> {
    if serde_json::to_vec(value).map_or(true, |bytes| bytes.len() > MAX_SIGNAL_BYTES) {
        return invalid("signal payload/schema must be at most 16 KiB");
    }
    visit(value, 0, &mut 0, false)
}

pub(super) fn compile(schema: &Value) -> Result<Validator, CoordinatorError> {
    validate_json(schema)?;
    visit(schema, 0, &mut 0, true)?;
    if schema
        .get("$schema")
        .is_some_and(|draft| draft != "https://json-schema.org/draft/2020-12/schema")
    {
        return invalid(
            "signal schemas use JSON Schema draft 2020-12; omit $schema or use its official URI",
        );
    }
    jsonschema::options()
        .with_draft(Draft::Draft202012)
        .build(schema)
        .map_err(|_| {
            CoordinatorError::InvalidParams("invalid inline JSON Schema (draft 2020-12)".into())
        })
}

fn visit(
    value: &Value,
    depth: usize,
    nodes: &mut usize,
    schema: bool,
) -> Result<(), CoordinatorError> {
    *nodes += 1;
    if depth > 32 || *nodes > 2048 {
        return invalid("signal JSON exceeds 32 levels or 2048 values");
    }
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if schema && matches!(key.as_str(), "$ref" | "$dynamicRef" | "$recursiveRef") {
                    return invalid(
                        "signal schemas must be inline; schema references are not supported",
                    );
                }
                visit(value, depth + 1, nodes, schema)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                visit(value, depth + 1, nodes, schema)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn invalid<T>(message: &str) -> Result<T, CoordinatorError> {
    Err(CoordinatorError::InvalidParams(message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validates_inline_schema_and_rejects_external_or_recursive_resolution() {
        let validator = compile(&json!({"type": "object", "properties": {"pr": {"type": "integer"}}, "required": ["pr"], "additionalProperties": false})).unwrap();
        assert!(validator.is_valid(&json!({"pr": 12})));
        assert!(!validator.is_valid(&json!({"pr": "12"})));
        assert!(!validator.is_valid(&json!({"pr": 12, "extra": true})));
        for reference in ["#", "file:///private", "https://example.invalid/schema"] {
            assert!(compile(&json!({"$ref": reference})).is_err());
        }
        assert!(compile(&json!({"type": "unknown"})).is_err());
        assert!(compile(&json!({"$schema": "https://example.invalid/schema"})).is_err());
    }

    #[test]
    fn bounds_values_and_validates_names() {
        assert!(validate_json(&json!("x".repeat(MAX_SIGNAL_BYTES))).is_err());
        let mut deep = Value::Null;
        for _ in 0..34 {
            deep = json!([deep]);
        }
        assert!(validate_json(&deep).is_err());
        assert!(validate_name("review.requested").is_ok());
        for name in ["", "UPPER", "has space", "0starts"] {
            assert!(validate_name(name).is_err());
        }
    }

    #[test]
    fn standard_shapes_reject_wrong_types_required_fields_and_extra_properties() {
        let validator = compile(&json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": {
                "pr": {"type": "integer", "minimum": 1},
                "outcome": {"enum": ["ready", "blocked"]},
                "checks": {"type": "array", "items": {"type": "object", "properties": {"passed": {"type": "boolean"}}, "required": ["passed"], "additionalProperties": false}}
            },
            "required": ["pr", "outcome", "checks"], "additionalProperties": false
        })).unwrap();
        let valid = json!({"pr": 42, "outcome": "ready", "checks": [{"passed": true}]});
        assert!(validator.is_valid(&valid));
        for (pointer, value) in [
            ("/pr", json!("42")),
            ("/pr", json!(0)),
            ("/outcome", json!("unknown")),
            ("/checks/0/passed", json!("true")),
        ] {
            let mut invalid = valid.clone();
            *invalid.pointer_mut(pointer).unwrap() = value;
            let error = validator.validate(&invalid).unwrap_err();
            assert_eq!(error.instance_path().to_string(), pointer);
        }
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove("pr");
        assert!(!validator.is_valid(&missing));
        let mut extra = valid;
        extra["extra"] = json!(true);
        assert!(!validator.is_valid(&extra));
        // Draft 2020-12 format annotations do not implicitly enable assertions.
        assert!(
            compile(&json!({"type": "string", "format": "email"}))
                .unwrap()
                .is_valid(&json!("not an email"))
        );
    }
}
