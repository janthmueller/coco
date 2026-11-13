use super::*;

#[test]
fn rename_original_path_is_not_misclassified_as_an_untracked_record() {
    let sha = "a".repeat(40);
    let status = format!(
        "# branch.oid {sha}\0# branch.head main\02 R. N... 100644 100644 100644 {sha} {sha} R100 new\0? old\0? real-untracked\0"
    );
    let (_, _, changes) = parse_status(status.as_bytes()).unwrap();
    assert_eq!(
        changes,
        GitChangeSummary {
            tracked_files: 1,
            untracked_files: 1
        }
    );
    assert!(parse_status(b"? missing-headers\0").is_err());
    assert!(parse_branches("refs/heads/main\0broken").is_err());
}
