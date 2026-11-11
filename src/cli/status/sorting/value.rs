use std::cmp::Ordering;

use crate::domain::WorkspacePhase;
use crate::domain::runtime::{WorkspaceRuntimeResources, WorkspaceRuntimeState};
use crate::protocol::{WorkspaceListItem, WorkspaceUsageItem};

use super::super::natural_cmp;
use super::SortField;

pub(super) enum SortValue {
    Text(String),
    Model(String, Option<String>),
    Integer(u64),
    Number(f64),
}

impl SortValue {
    pub(super) fn compare(&self, right: &Self) -> Ordering {
        match (self, right) {
            (Self::Text(left), Self::Text(right)) => natural_cmp(left, right),
            (Self::Model(left, effort), Self::Model(right, other)) => {
                natural_cmp(left, right).then_with(|| effort.as_deref().cmp(&other.as_deref()))
            }
            (Self::Integer(left), Self::Integer(right)) => left.cmp(right),
            (Self::Number(left), Self::Number(right)) => left.total_cmp(right),
            _ => unreachable!("one sort field always has the same value type"),
        }
    }
}

pub(super) fn read(
    row: &WorkspaceListItem,
    field: SortField,
    usage: Option<&WorkspaceUsageItem>,
) -> Option<SortValue> {
    Some(match field {
        SortField::State => SortValue::Integer(state_order(row.workspace.phase)),
        SortField::Name => SortValue::Text(row.workspace.name.clone()),
        SortField::Repo => SortValue::Text(row.repository.root_path.to_string_lossy().into_owned()),
        SortField::Recent => {
            let time = row.workspace.thread_runtime.as_ref()?.recency_at_ms?;
            chrono::DateTime::from_timestamp_millis(time)?;
            SortValue::Integer(u64::try_from(time).ok()?)
        }
        SortField::Branch => SortValue::Text(
            row.workspace
                .branch_name
                .clone()
                .unwrap_or_else(|| "detached".to_owned()),
        ),
        SortField::Model => {
            let runtime = row.workspace.thread_runtime.as_ref()?;
            let model = runtime.model.as_deref()?.trim();
            if model.is_empty() {
                return None;
            }
            let effort = runtime
                .reasoning_effort
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned);
            SortValue::Model(model.to_owned(), effort)
        }
        SortField::Memory => {
            let runtime = running(row)?;
            SortValue::Integer(
                runtime
                    .memory_current_bytes
                    .or(runtime.resident_memory_bytes)?,
            )
        }
        SortField::Procs => SortValue::Integer(u64::from(running(row)?.process_count?)),
        SortField::Cpu => {
            let value = running(row)?.cpu_percent?;
            if !value.is_finite() || value < 0.0 {
                return None;
            }
            SortValue::Number(value + 0.0)
        }
        SortField::Tokens => {
            SortValue::Integer(usage?.tokens.as_ref()?.checkpoint.total.total_tokens)
        }
        SortField::Context => {
            let tokens = &usage?.tokens.as_ref()?.checkpoint;
            let window = tokens.model_context_window.filter(|window| *window > 0)?;
            SortValue::Number(tokens.last.total_tokens as f64 / window as f64)
        }
    })
}

fn running(row: &WorkspaceListItem) -> Option<&WorkspaceRuntimeResources> {
    row.runtime_resources
        .as_ref()
        .filter(|runtime| runtime.state == WorkspaceRuntimeState::Running)
}

pub(in crate::cli::status) const fn state_order(phase: WorkspacePhase) -> u64 {
    match phase {
        WorkspacePhase::WaitingForApproval => 0,
        WorkspacePhase::WaitingForInput => 1,
        WorkspacePhase::SystemError => 2,
        WorkspacePhase::Unavailable => 3,
        WorkspacePhase::Failed => 4,
        WorkspacePhase::Active => 5,
        WorkspacePhase::Provisioning => 6,
        WorkspacePhase::Starting => 7,
        WorkspacePhase::Closing => 8,
        WorkspacePhase::Reopening => 9,
        WorkspacePhase::Deleting => 10,
        WorkspacePhase::Idle => 11,
        WorkspacePhase::Completed => 12,
        WorkspacePhase::Prepared => 13,
        WorkspacePhase::NotLoaded => 14,
        WorkspacePhase::Closed => 15,
    }
}
