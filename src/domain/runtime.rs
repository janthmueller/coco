use serde::{Deserialize, Serialize};
use thiserror::Error;

pub(crate) const WORKSPACE_RESOURCE_POLICY_SCHEMA_VERSION: u32 = 1;
pub(crate) const MAX_CPU_MILLICORES: u32 = 1_024_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkspaceRuntimeBackend {
    ExecServer,
}

/// Lifecycle of the executor process owned by one workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkspaceRuntimeState {
    Inactive,
    Running,
    Exited,
}

/// Boundary covered by one resource observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkspaceResourceScope {
    ProcessTree,
    RootProcess,
    CgroupV2,
}

/// Durable resource intent for one workspace execution boundary.
///
/// The vocabulary describes behavior CoCo can ask a runtime backend to
/// provide. It deliberately does not contain systemd property names so a
/// future native backend can advertise only the subset it implements with the
/// same semantics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspaceResourcePolicy {
    pub(crate) schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) memory_high_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) memory_max_bytes: Option<u64>,
    /// Maximum CPU capacity in thousandths of one logical CPU. A value of
    /// 1,000 therefore permits one fully used core.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) cpu_max_millicores: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) cpu_weight: Option<u16>,
    /// Maximum kernel tasks/threads in this execution boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) tasks_max: Option<u64>,
}

impl Default for WorkspaceResourcePolicy {
    fn default() -> Self {
        Self {
            schema_version: WORKSPACE_RESOURCE_POLICY_SCHEMA_VERSION,
            memory_high_bytes: None,
            memory_max_bytes: None,
            cpu_max_millicores: None,
            cpu_weight: None,
            tasks_max: None,
        }
    }
}

impl WorkspaceResourcePolicy {
    pub(crate) const fn is_empty(&self) -> bool {
        self.memory_high_bytes.is_none()
            && self.memory_max_bytes.is_none()
            && self.cpu_max_millicores.is_none()
            && self.cpu_weight.is_none()
            && self.tasks_max.is_none()
    }

    pub(crate) fn validate(&self) -> Result<(), WorkspaceResourcePolicyError> {
        if self.schema_version != WORKSPACE_RESOURCE_POLICY_SCHEMA_VERSION {
            return Err(WorkspaceResourcePolicyError::UnsupportedSchema(
                self.schema_version,
            ));
        }
        for (field, value) in [
            ("memoryHighBytes", self.memory_high_bytes),
            ("memoryMaxBytes", self.memory_max_bytes),
        ] {
            if value == Some(0) {
                return Err(WorkspaceResourcePolicyError::ZeroValue(field));
            }
        }
        if let (Some(high), Some(max)) = (self.memory_high_bytes, self.memory_max_bytes)
            && high > max
        {
            return Err(WorkspaceResourcePolicyError::MemoryOrder { high, max });
        }
        if let Some(value) = self.cpu_max_millicores
            && !(1..=MAX_CPU_MILLICORES).contains(&value)
        {
            return Err(WorkspaceResourcePolicyError::CpuMaximum(value));
        }
        if let Some(value) = self.cpu_weight
            && !(1..=10_000).contains(&value)
        {
            return Err(WorkspaceResourcePolicyError::CpuWeight(value));
        }
        if self.tasks_max == Some(0) {
            return Err(WorkspaceResourcePolicyError::ZeroValue("tasksMax"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspaceResourcePolicySnapshot {
    pub(crate) revision: u64,
    pub(crate) policy: WorkspaceResourcePolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkspaceResourceControllerBackend {
    None,
    SystemdCgroupV2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspaceResourceCapabilities {
    pub(crate) backend: WorkspaceResourceControllerBackend,
    pub(crate) dynamic_updates: bool,
    pub(crate) memory_high: bool,
    pub(crate) memory_max: bool,
    pub(crate) cpu_max: bool,
    pub(crate) cpu_weight: bool,
    pub(crate) tasks_max: bool,
}

impl WorkspaceResourceCapabilities {
    pub(crate) const fn unavailable() -> Self {
        Self {
            backend: WorkspaceResourceControllerBackend::None,
            dynamic_updates: false,
            memory_high: false,
            memory_max: false,
            cpu_max: false,
            cpu_weight: false,
            tasks_max: false,
        }
    }

    pub(crate) fn unsupported_fields(&self, policy: &WorkspaceResourcePolicy) -> Vec<&'static str> {
        let mut fields = Vec::new();
        if policy.memory_high_bytes.is_some() && !self.memory_high {
            fields.push("memoryHighBytes");
        }
        if policy.memory_max_bytes.is_some() && !self.memory_max {
            fields.push("memoryMaxBytes");
        }
        if policy.cpu_max_millicores.is_some() && !self.cpu_max {
            fields.push("cpuMaxMillicores");
        }
        if policy.cpu_weight.is_some() && !self.cpu_weight {
            fields.push("cpuWeight");
        }
        if policy.tasks_max.is_some() && !self.tasks_max {
            fields.push("tasksMax");
        }
        fields
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspaceResourceControllerStatus {
    pub(crate) capabilities: WorkspaceResourceCapabilities,
    pub(crate) runtime_state: WorkspaceRuntimeState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) applied_policy: Option<WorkspaceResourcePolicySnapshot>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub(crate) enum WorkspaceResourcePolicyError {
    #[error("unsupported workspace resource policy schema version {0}")]
    UnsupportedSchema(u32),
    #[error("{0} must be greater than zero")]
    ZeroValue(&'static str),
    #[error("memoryHighBytes ({high}) must not exceed memoryMaxBytes ({max})")]
    MemoryOrder { high: u64, max: u64 },
    #[error("cpuMaxMillicores must be between 1 and {MAX_CPU_MILLICORES}, got {0}")]
    CpuMaximum(u32),
    #[error("cpuWeight must be between 1 and 10000, got {0}")]
    CpuWeight(u16),
}

/// Cumulative controller events observed for a cgroup-v2 workspace runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspaceResourceEvents {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) memory_high: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) memory_max: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) memory_oom: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) memory_oom_kill: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) pids_max: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) cpu_throttled_periods: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) cpu_throttled_usec: Option<u64>,
}

/// Ephemeral resource observation for a workspace-owned execution process.
///
/// This is never persisted. Optional measurements stay absent when the host
/// cannot attribute them without overstating the available evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspaceRuntimeResources {
    pub(crate) backend: WorkspaceRuntimeBackend,
    pub(crate) state: WorkspaceRuntimeState,
    pub(crate) scope: WorkspaceResourceScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) process_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) process_count: Option<u32>,
    /// Kernel task/thread count. This is distinct from `process_count` and is
    /// the quantity constrained by cgroup `pids.max`/systemd `TasksMax`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) task_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) resident_memory_bytes: Option<u64>,
    /// Total memory charged by cgroup v2. It is neither RSS nor PSS.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) memory_current_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) cpu_percent: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) cpu_usage_usec: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) cgroup_unit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) events: Option<WorkspaceResourceEvents>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) sampled_at_ms: Option<i64>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn resource_policy_validation_preserves_portable_semantics() {
        let policy = WorkspaceResourcePolicy {
            memory_high_bytes: Some(256),
            memory_max_bytes: Some(128),
            ..WorkspaceResourcePolicy::default()
        };
        assert!(matches!(
            policy.validate(),
            Err(WorkspaceResourcePolicyError::MemoryOrder {
                high: 256,
                max: 128
            })
        ));

        let policy = WorkspaceResourcePolicy {
            cpu_weight: Some(10_001),
            ..WorkspaceResourcePolicy::default()
        };
        assert_eq!(
            policy.validate(),
            Err(WorkspaceResourcePolicyError::CpuWeight(10_001))
        );

        let policy = WorkspaceResourcePolicy {
            tasks_max: Some(0),
            ..WorkspaceResourcePolicy::default()
        };
        assert_eq!(
            policy.validate(),
            Err(WorkspaceResourcePolicyError::ZeroValue("tasksMax"))
        );

        let valid = WorkspaceResourcePolicy {
            memory_high_bytes: Some(128),
            memory_max_bytes: Some(256),
            cpu_max_millicores: Some(1_500),
            cpu_weight: Some(200),
            tasks_max: Some(64),
            ..WorkspaceResourcePolicy::default()
        };
        assert_eq!(valid.validate(), Ok(()));
    }

    #[test]
    fn cgroup_observation_keeps_memory_and_controller_semantics_explicit() {
        let resources = WorkspaceRuntimeResources {
            backend: WorkspaceRuntimeBackend::ExecServer,
            state: WorkspaceRuntimeState::Running,
            scope: WorkspaceResourceScope::CgroupV2,
            process_id: Some(42),
            process_count: Some(2),
            task_count: Some(17),
            resident_memory_bytes: None,
            memory_current_bytes: Some(25_165_824),
            cpu_percent: Some(50.0),
            cpu_usage_usec: Some(500_000),
            cgroup_unit: Some("opaque.scope".to_owned()),
            events: Some(WorkspaceResourceEvents {
                memory_high: Some(3),
                memory_max: Some(1),
                memory_oom: Some(1),
                memory_oom_kill: Some(0),
                pids_max: Some(4),
                cpu_throttled_periods: Some(2),
                cpu_throttled_usec: Some(3_000),
            }),
            sampled_at_ms: Some(1),
        };
        assert_eq!(
            serde_json::to_value(resources).unwrap(),
            json!({
                "backend": "exec_server",
                "state": "running",
                "scope": "cgroup_v2",
                "processId": 42,
                "processCount": 2,
                "taskCount": 17,
                "memoryCurrentBytes": 25_165_824,
                "cpuPercent": 50.0,
                "cpuUsageUsec": 500_000,
                "cgroupUnit": "opaque.scope",
                "events": {
                    "memoryHigh": 3,
                    "memoryMax": 1,
                    "memoryOom": 1,
                    "memoryOomKill": 0,
                    "pidsMax": 4,
                    "cpuThrottledPeriods": 2,
                    "cpuThrottledUsec": 3_000,
                },
                "sampledAtMs": 1,
            })
        );
    }
}
