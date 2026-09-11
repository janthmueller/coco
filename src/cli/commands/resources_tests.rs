use std::collections::HashSet;

use super::*;

fn set_args() -> LimitsSetArgs {
    LimitsSetArgs {
        workspace: Some("review/api".to_owned()),
        global: false,
        memory_high: None,
        memory_max: None,
        cpu_max: None,
        cpu_weight: None,
        tasks_max: None,
        clear: Vec::new(),
        json: false,
    }
}

#[test]
fn limit_patch_requires_a_change_and_rejects_set_clear_conflicts() {
    assert_eq!(
        limits_patch(&set_args()).unwrap_err().to_string(),
        "set at least one limit or use --clear <FIELD>"
    );

    let mut conflict = set_args();
    conflict.memory_max = Some(512);
    conflict.clear.push(LimitField::MemoryMax);
    assert_eq!(
        limits_patch(&conflict).unwrap_err().to_string(),
        "--memory-max conflicts with --clear memory-max"
    );
}

#[test]
fn limit_patch_can_set_and_clear_independent_fields() {
    let mut args = set_args();
    args.memory_max = Some(512);
    args.cpu_max = Some(750);
    args.clear = vec![LimitField::MemoryHigh, LimitField::TasksMax];
    let patch = limits_patch(&args).unwrap();
    assert_eq!(patch.memory_high_bytes, Some(ResourcePolicyUpdate::Clear));
    assert_eq!(patch.memory_max_bytes, Some(ResourcePolicyUpdate::Set(512)));
    assert_eq!(
        patch.cpu_max_millicores,
        Some(ResourcePolicyUpdate::Set(750))
    );
    assert_eq!(patch.tasks_max, Some(ResourcePolicyUpdate::Clear));
    assert_eq!(patch.cpu_weight, None);

    let unique = args.clear.iter().copied().collect::<HashSet<_>>();
    assert_eq!(unique.len(), 2);
}
