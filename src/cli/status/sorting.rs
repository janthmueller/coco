use std::cmp::Ordering;
use std::collections::HashMap;
use std::str::FromStr;

use anyhow::{Result, ensure};
use clap::ValueEnum;

use crate::protocol::{WorkspaceListItem, WorkspaceUsageItem};

mod value;
use value::SortValue;
#[cfg(test)]
pub(super) use value::state_order;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(in crate::cli) enum SortField {
    State,
    #[value(alias = "workspace")]
    Name,
    #[value(alias = "repository")]
    Repo,
    Model,
    Branch,
    Memory,
    Cpu,
    Procs,
    Tokens,
    Context,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::cli) struct SortKey {
    pub(in crate::cli) field: SortField,
    descending: bool,
}

impl FromStr for SortKey {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        let (descending, name) = value
            .strip_prefix('-')
            .map_or((false, value), |name| (true, name));
        Ok(Self {
            field: <SortField as ValueEnum>::from_str(name, false)?,
            descending,
        })
    }
}

#[derive(Debug)]
pub(super) struct SortPlan {
    keys: Vec<SortKey>,
}

impl SortPlan {
    pub(super) fn new(keys: &[SortKey], resources: bool, usage: bool) -> Result<Self> {
        let mut ordered = Vec::new();
        for key in keys {
            ensure!(
                !ordered
                    .iter()
                    .any(|previous: &SortKey| previous.field == key.field),
                "each sort field may appear only once"
            );
            ensure!(
                !matches!(
                    key.field,
                    SortField::Memory | SortField::Cpu | SortField::Procs
                ) || resources,
                "resource sort keys require --resources (-r)"
            );
            ensure!(
                !matches!(key.field, SortField::Tokens | SortField::Context) || usage,
                "usage sort keys require --usage (-u)"
            );
            ordered.push(*key);
        }
        for field in [SortField::Repo, SortField::Name] {
            if !ordered.iter().any(|key| key.field == field) {
                ordered.push(SortKey {
                    field,
                    descending: false,
                });
            }
        }
        Ok(Self { keys: ordered })
    }

    pub(super) fn apply(
        &self,
        rows: &mut [WorkspaceListItem],
        usage: Option<&[WorkspaceUsageItem]>,
    ) {
        let usage = usage
            .unwrap_or_default()
            .iter()
            .map(|item| (item.workspace.id.as_str(), item))
            .collect::<HashMap<_, _>>();
        // Cache owned raw values once, not formatted strings or model labels on
        // every comparison. IDs alone are the final deterministic tie-breaker.
        let values = rows
            .iter()
            .map(|row| {
                let usage = usage.get(row.workspace.id.as_str()).copied();
                let fields = self
                    .keys
                    .iter()
                    .map(|key| value::read(row, key.field, usage))
                    .collect::<Vec<_>>();
                (row.workspace.id.clone(), fields)
            })
            .collect::<HashMap<_, _>>();
        rows.sort_by(|left, right| {
            let left_values = &values[&left.workspace.id];
            let right_values = &values[&right.workspace.id];
            self.keys
                .iter()
                .enumerate()
                .map(|(index, key)| {
                    compare(
                        left_values[index].as_ref(),
                        right_values[index].as_ref(),
                        key.descending,
                    )
                })
                .find(|order| *order != Ordering::Equal)
                .unwrap_or_else(|| left.workspace.id.cmp(&right.workspace.id))
        });
    }
}

fn compare(left: Option<&SortValue>, right: Option<&SortValue>, descending: bool) -> Ordering {
    match (left, right) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(left), Some(right)) if descending => right.compare(left),
        (Some(left), Some(right)) => left.compare(right),
    }
}

pub(in crate::cli) fn sort_workspace_collection(rows: &mut [WorkspaceListItem]) {
    SortPlan::new(&[], false, false)
        .expect("default sort is valid")
        .apply(rows, None);
}
