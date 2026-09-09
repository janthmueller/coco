use super::*;
use serde_json::json;

fn write(directory: &Path, name: &str, value: &Value) {
    fs::write(directory.join(name), serde_json::to_vec(value).unwrap()).unwrap();
}

#[test]
fn files_are_plain_standard_schemas_with_stable_names_and_sorted_versions() {
    let directory = tempfile::tempdir().unwrap();
    let schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "description": "Ask for review",
        "type": "object", "properties": {"pr": {"type": "integer", "minimum": 1}},
        "required": ["pr"], "additionalProperties": false
    });
    write(directory.path(), "review.requested@10.json", &schema);
    write(directory.path(), "review.requested@2.json", &schema);
    write(directory.path(), "free.payload@1.json", &json!(true));
    fs::write(directory.path().join("README.md"), "ignored").unwrap();
    fs::create_dir(directory.path().join("nested")).unwrap();
    write(
        &directory.path().join("nested"),
        "not.loaded@1.json",
        &json!(true),
    );
    let definitions = load(directory.path(), "repo").unwrap();
    assert_eq!(definitions.len(), 3);
    assert_eq!(definitions[0].name, "free.payload");
    assert_eq!(definitions[0].description, "free.payload");
    assert_eq!(definitions[0].payload_schema, Some(json!(true)));
    assert_eq!(definitions[1].version, 2);
    assert_eq!(definitions[2].version, 10);
    assert_eq!(definitions[1].payload_schema, Some(schema));
    assert_eq!(definitions[1].description, "Ask for review");
    assert!(
        definitions
            .iter()
            .all(|definition| definition.repository_id == "repo")
    );
}

#[test]
fn invalid_files_fail_the_whole_collection_without_dumping_their_contents() {
    for (filename, value) in [
        ("missing-version.json", json!(true)),
        ("test@0.json", json!(true)),
        ("test@01.json", json!(true)),
        ("test@-1.json", json!(true)),
        ("test@4294967296.json", json!(true)),
        ("UPPER@1.json", json!(true)),
        ("test@1.json", json!({"type": "PRIVATE_INVALID_TYPE"})),
        ("test@1.json", json!({"description": ""})),
        ("test@1.json", json!({"description": "x".repeat(1025)})),
        (
            "test@1.json",
            json!({"$schema": "https://json-schema.org/draft-07/schema#"}),
        ),
        ("test@1.json", json!({"$ref": "file:///private"})),
    ] {
        let directory = tempfile::tempdir().unwrap();
        write(directory.path(), "valid@1.json", &json!(true));
        write(directory.path(), filename, &value);
        let error = load(directory.path(), "repo").unwrap_err().to_string();
        assert!(error.contains(filename), "filename missing: {error}");
        assert!(!error.contains("PRIVATE_INVALID_TYPE"));
    }
}

#[test]
fn unreadable_non_files_and_oversized_collections_fail_closed() {
    let directory = tempfile::tempdir().unwrap();
    assert!(load(Path::new("relative"), "repo").is_err());
    assert!(load(&directory.path().join("absent"), "repo").is_err());
    let file = directory.path().join("test@1.json");
    fs::write(&file, "not JSON").unwrap();
    assert!(load(directory.path(), "repo").is_err());
    fs::write(&file, " ".repeat(MAX_SIGNAL_BYTES + 1)).unwrap();
    assert!(load(directory.path(), "repo").is_err());
    fs::remove_file(&file).unwrap();
    fs::create_dir(&file).unwrap();
    assert!(load(directory.path(), "repo").is_err());
    fs::remove_dir(&file).unwrap();
    for version in 1..=129 {
        write(
            directory.path(),
            &format!("test@{version}.json"),
            &json!(true),
        );
    }
    assert!(load(directory.path(), "repo").is_err());
}

#[cfg(unix)]
#[test]
fn symlinks_do_not_import_files_outside_the_selected_collection() {
    let directory = tempfile::tempdir().unwrap();
    let external = tempfile::NamedTempFile::new().unwrap();
    fs::write(external.path(), "true").unwrap();
    std::os::unix::fs::symlink(external.path(), directory.path().join("test@1.json")).unwrap();
    assert!(load(directory.path(), "repo").is_err());
}
