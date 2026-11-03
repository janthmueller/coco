use std::str::FromStr;

use anyhow::{Result, ensure};

use crate::domain::WorkspacePhase;
use crate::protocol::WorkspaceListItem;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::cli) struct StateSelector(WorkspacePhase);

impl FromStr for StateSelector {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        let alias = match value {
            "working" => "active",
            "ready" => "idle",
            "unloaded" => "not_loaded",
            "needs-approval" => "waiting_for_approval",
            "needs-input" => "waiting_for_input",
            "codex-error" => "system_error",
            "preparing" => "provisioning",
            value => value,
        };
        WorkspacePhase::parse(&alias.replace('-', "_")).map(Self).ok_or_else(|| format!(
            "unknown state {value:?}; use needs-approval, needs-input, codex-error, unavailable, failed, working, preparing, starting, closing, reopening, deleting, ready, completed, prepared or unloaded"
        ))
    }
}

#[derive(Debug)]
pub(super) struct StateFilter {
    include: Vec<WorkspacePhase>,
    exclude: Vec<WorkspacePhase>,
}

impl StateFilter {
    pub(super) fn new(include: &[StateSelector], exclude: &[StateSelector]) -> Result<Self> {
        ensure!(
            !include
                .iter()
                .any(|state| state.0 == WorkspacePhase::Closed),
            "status shows open workspaces; use `coco list --closed` to list closed workspaces"
        );
        Ok(Self {
            include: include.iter().map(|state| state.0).collect(),
            exclude: exclude.iter().map(|state| state.0).collect(),
        })
    }

    pub(super) fn is_active(&self) -> bool {
        !self.include.is_empty() || !self.exclude.is_empty()
    }

    pub(super) fn phases(&self) -> Option<Vec<String>> {
        // Only positive selection becomes a daemon phase request. A complement
        // would accidentally opt into the daemon's explicit closed-phase scope.
        (!self.include.is_empty()).then(|| {
            self.include
                .iter()
                .map(|phase| phase.as_str().to_owned())
                .collect()
        })
    }

    pub(super) fn apply(&self, rows: &mut Vec<WorkspaceListItem>) {
        rows.retain(|row| {
            row.workspace.phase != WorkspacePhase::Closed
                && (self.include.is_empty() || self.include.contains(&row.workspace.phase))
                && !self.exclude.contains(&row.workspace.phase)
        });
    }
}
