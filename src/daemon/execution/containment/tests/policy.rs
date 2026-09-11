use super::super::*;

#[test]
fn portable_policy_maps_to_complete_systemd_properties() {
    let policy = WorkspaceResourcePolicy {
        memory_high_bytes: Some(256),
        memory_max_bytes: Some(512),
        cpu_max_millicores: Some(1_505),
        cpu_weight: Some(321),
        tasks_max: Some(128),
        ..WorkspaceResourcePolicy::default()
    };
    assert_eq!(
        configured_policy_properties(&policy),
        [
            "MemoryHigh=256",
            "MemoryMax=512",
            "CPUQuota=150.5%",
            "CPUWeight=321",
            "TasksMax=128",
        ]
    );
    assert_eq!(
        complete_policy_properties(&WorkspaceResourcePolicy::default()),
        [
            "MemoryHigh=",
            "MemoryMax=",
            "CPUQuota=",
            "CPUWeight=100",
            "TasksMax=",
        ]
    );
    assert_eq!(cpu_quota_percent(1), "0.1%");
    assert_eq!(cpu_quota_percent(1_000), "100%");
    assert!(cpu_ratio_matches(150_000, 100_000, 1_500));
    assert!(!cpu_ratio_matches(100_000, 100_000, 1_500));
}

#[tokio::test]
#[ignore = "requires a cgroup-v2 systemd user manager"]
async fn live_scope_applies_updates_and_resets_resource_policy() {
    let directory = tempfile::tempdir().unwrap();
    let backend = SystemdBackend::detect(directory.path()).await.unwrap();
    backend.cleanup_stale_scopes().await.unwrap();
    let containment = WorkspaceContainment {
        backend: ContainmentBackend::Systemd(backend),
    };
    let initial = WorkspaceResourcePolicy {
        memory_high_bytes: Some(256 * 1024 * 1024),
        memory_max_bytes: Some(512 * 1024 * 1024),
        cpu_max_millicores: Some(1_505),
        cpu_weight: Some(321),
        tasks_max: Some(128),
        ..WorkspaceResourcePolicy::default()
    };
    let pending = containment
        .prepare_with_policy(
            "live-policy-test",
            WorkspaceResourcePolicySnapshot {
                revision: 1,
                policy: initial,
            },
        )
        .unwrap();
    let mut command = pending.command(Path::new("/bin/sh"), &["-c", "sleep 30 & wait"]);
    command
        .current_dir(directory.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut child = command.spawn().unwrap();
    let supervisor_pid = child.id().unwrap();

    let result: Result<(), String> = async {
        let active = activate_scope(&pending, supervisor_pid).await?;
        exercise_live_policy(&active).await?;
        active.stop().await.map_err(|error| error.to_string())?;
        Ok(())
    }
    .await;

    let cleanup = pending.cleanup().await;
    timeout(Duration::from_secs(5), child.wait())
        .await
        .expect("systemd-run supervisor did not exit")
        .expect("could not wait for systemd-run supervisor");
    cleanup.expect("could not clean up disposable policy scope");
    result.expect("live resource policy contract failed");
}

async fn activate_scope(
    pending: &PendingContainment,
    supervisor_pid: u32,
) -> Result<ActiveContainment, String> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        match pending.clone().activate(supervisor_pid).await {
            Ok(active) => return Ok(active),
            Err(_error) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(error) => return Err(format!("test scope did not become active: {error}")),
        }
    }
}

async fn exercise_live_policy(active: &ActiveContainment) -> Result<(), String> {
    let path = active
        .cgroup_path()
        .ok_or_else(|| "live systemd scope had no cgroup path".to_owned())?;
    let updated = WorkspaceResourcePolicy {
        memory_high_bytes: Some(128 * 1024 * 1024),
        memory_max_bytes: Some(256 * 1024 * 1024),
        cpu_max_millicores: Some(750),
        cpu_weight: Some(456),
        tasks_max: Some(96),
        ..WorkspaceResourcePolicy::default()
    };
    active
        .apply_policy(&updated)
        .await
        .map_err(|error| error.to_string())?;
    assert_unsafe_memory_limit_is_rejected(active, path, &updated).await?;

    let cleared = WorkspaceResourcePolicy {
        cpu_max_millicores: updated.cpu_max_millicores,
        ..WorkspaceResourcePolicy::default()
    };
    active
        .apply_policy(&cleared)
        .await
        .map_err(|error| error.to_string())?;
    assert_non_cpu_limits_are_cleared(path)?;

    if !matches!(
        active
            .apply_policy(&WorkspaceResourcePolicy::default())
            .await,
        Err(ContainmentError::CpuMaximumRemovalRequiresRestart)
    ) {
        return Err("active CPU maximum removal did not require a restart".to_owned());
    }
    Ok(())
}

async fn assert_unsafe_memory_limit_is_rejected(
    active: &ActiveContainment,
    path: &Path,
    policy: &WorkspaceResourcePolicy,
) -> Result<(), String> {
    let current =
        read_cgroup_u64(&path.join("memory.current")).map_err(|error| error.to_string())?;
    let unsafe_policy = WorkspaceResourcePolicy {
        memory_max_bytes: Some(current.saturating_sub(1)),
        ..policy.clone()
    };
    if matches!(
        active.apply_policy(&unsafe_policy).await,
        Err(ContainmentError::MemoryMaximumBelowCurrent { .. })
    ) {
        Ok(())
    } else {
        Err("unsafe memory maximum was not rejected".to_owned())
    }
}

fn assert_non_cpu_limits_are_cleared(path: &Path) -> Result<(), String> {
    let read = |name: &str| read_cgroup_value(&path.join(name)).map_err(|error| error.to_string());
    let memory_high = read("memory.high")?;
    let memory_max = read("memory.max")?;
    let cpu_max = read("cpu.max")?;
    let cpu_weight = read("cpu.weight")?;
    let tasks_max = read("pids.max")?;
    if memory_high == "max"
        && memory_max == "max"
        && !cpu_max.starts_with("max ")
        && cpu_weight == "100"
        && tasks_max != "96"
    {
        Ok(())
    } else {
        Err(format!(
            "partially cleared policy values: memory.high={memory_high}, memory.max={memory_max}, cpu.max={cpu_max}, cpu.weight={cpu_weight}, pids.max={tasks_max}"
        ))
    }
}
