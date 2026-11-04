use std::cmp::Ordering;
use std::io;
use std::time::Duration;

use anyhow::Result;

use crate::protocol::{
    AccountQuotaGetParams, AccountQuotaResult, RepositoryScope, WorkspaceGetParams,
    WorkspaceStatusResult, WorkspaceUsageGetParams, WorkspaceUsageItem, WorkspaceUsageListParams,
};
use crate::rpc::RpcClient;

mod collection;
mod filter;
mod sorting;

use super::follow::{FollowOutput, stdout_supports_live_updates, wait_or_interrupt};
use super::output::{
    render_follow_status, render_status_for_stdout, render_workspace_status_list_for_stdout,
};
pub(super) use collection::CollectionQuery;
pub(super) use filter::StateSelector;
pub(super) use sorting::{SortKey, sort_workspace_collection};
const FOLLOW_POLL_INTERVAL: Duration = Duration::from_millis(500);
const COLLECTION_POLL_INTERVAL: Duration = Duration::from_secs(1);
const SPINNER_FRAME_INTERVAL: Duration = Duration::from_millis(125);
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

#[derive(Debug, Clone, Copy)]
pub(super) struct StatusProjection {
    pub(super) resources: bool,
    pub(super) usage: bool,
    pub(super) quota: bool,
    pub(super) clients: bool,
}

pub(super) fn natural_cmp(left: &str, right: &str) -> Ordering {
    let left = left.as_bytes();
    let right = right.as_bytes();
    let mut left_index = 0;
    let mut right_index = 0;

    while left_index < left.len() && right_index < right.len() {
        if left[left_index].is_ascii_digit() && right[right_index].is_ascii_digit() {
            let left_end = digit_run_end(left, left_index);
            let right_end = digit_run_end(right, right_index);
            let left_significant = significant_digits(left, left_index, left_end);
            let right_significant = significant_digits(right, right_index, right_end);
            let order = left_significant
                .len()
                .cmp(&right_significant.len())
                .then_with(|| left_significant.cmp(right_significant))
                .then_with(|| (left_end - left_index).cmp(&(right_end - right_index)));
            if order != Ordering::Equal {
                return order;
            }
            left_index = left_end;
            right_index = right_end;
            continue;
        }

        let order = left[left_index].cmp(&right[right_index]);
        if order != Ordering::Equal {
            return order;
        }
        left_index += 1;
        right_index += 1;
    }

    left.len().cmp(&right.len())
}

fn digit_run_end(value: &[u8], start: usize) -> usize {
    let mut end = start;
    while end < value.len() && value[end].is_ascii_digit() {
        end += 1;
    }
    end
}

fn significant_digits(value: &[u8], start: usize, end: usize) -> &[u8] {
    let mut significant = start;
    while significant + 1 < end && value[significant] == b'0' {
        significant += 1;
    }
    &value[significant..end]
}

pub(super) async fn follow_status(
    client: &RpcClient,
    scope: RepositoryScope,
    workspace: &str,
    projection: StatusProjection,
) -> Result<()> {
    let interactive = stdout_supports_live_updates();
    let mut output = FollowOutput::new(io::stdout(), interactive)?;
    let mut previous_log_frame: Option<String> = None;
    let mut spinner_index = 0_usize;
    loop {
        let response = client
            .request(WorkspaceGetParams {
                scope: scope.clone(),
                workspace: workspace.to_owned(),
                include_resources: projection.resources,
                include_clients: projection.clients,
            })
            .await?;
        let usage = workspace_usage(client, &response, projection.usage).await?;
        let quota = account_quota(client, projection.quota).await?;
        if interactive {
            for _ in 0..spinner_frames_per_poll() {
                let frame = render_follow_status(
                    &response,
                    SPINNER[spinner_index % SPINNER.len()],
                    projection.resources,
                    usage.as_ref(),
                    quota.as_ref(),
                );
                output.write_frame(&frame)?;
                spinner_index += 1;
                if wait_or_interrupt(SPINNER_FRAME_INTERVAL).await {
                    return Ok(());
                }
            }
        } else {
            let frame = render_status_for_stdout(
                &response,
                projection.resources,
                usage.as_ref(),
                quota.as_ref(),
            );
            if previous_log_frame.as_deref() != Some(&frame) {
                output.write_frame(&frame)?;
                previous_log_frame = Some(frame);
            }
            if wait_or_interrupt(FOLLOW_POLL_INTERVAL).await {
                return Ok(());
            }
        }
    }
}

pub(super) async fn follow_status_collection(
    client: &RpcClient,
    scope: RepositoryScope,
    projection: StatusProjection,
    tree: bool,
    query: CollectionQuery,
) -> Result<()> {
    let include_repository = matches!(scope, RepositoryScope::AllRepositories);
    let mut output = FollowOutput::new(io::stdout(), stdout_supports_live_updates())?;
    let mut previous_frame: Option<String> = None;
    loop {
        let snapshot = query.read(client, scope.clone(), projection, false).await?;
        let frame = render_workspace_status_list_for_stdout(
            &snapshot.workspaces,
            include_repository,
            projection.resources,
            snapshot.usage.as_deref(),
            snapshot.quota.as_ref(),
            tree,
            query.filtered(),
        );
        if previous_frame.as_deref() != Some(&frame) {
            output.write_frame(&frame)?;
            previous_frame = Some(frame);
        }
        if wait_or_interrupt(COLLECTION_POLL_INTERVAL).await {
            return Ok(());
        }
    }
}

pub(super) async fn account_quota(
    client: &RpcClient,
    include_quota: bool,
) -> Result<Option<AccountQuotaResult>> {
    if !include_quota {
        return Ok(None);
    }
    Ok(Some(client.request(AccountQuotaGetParams {}).await?))
}

pub(super) async fn workspace_usage(
    client: &RpcClient,
    status: &WorkspaceStatusResult,
    include_usage: bool,
) -> Result<Option<WorkspaceUsageItem>> {
    if !include_usage {
        return Ok(None);
    }
    Ok(Some(
        client
            .request(WorkspaceUsageGetParams {
                scope: RepositoryScope::AllRepositories,
                workspace: status.workspace.id.clone(),
            })
            .await?,
    ))
}

pub(super) async fn workspace_usage_collection(
    client: &RpcClient,
    scope: RepositoryScope,
    include_usage: bool,
) -> Result<Option<Vec<WorkspaceUsageItem>>> {
    if !include_usage {
        return Ok(None);
    }
    Ok(Some(
        client.request(WorkspaceUsageListParams { scope }).await?,
    ))
}

const fn spinner_frames_per_poll() -> usize {
    (FOLLOW_POLL_INTERVAL.as_millis() / SPINNER_FRAME_INTERVAL.as_millis()) as usize
}

#[cfg(test)]
#[path = "status/tests.rs"]
mod query_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::WorkspacePhase;

    #[test]
    fn natural_names_keep_slash_groups_and_order_numeric_suffixes() {
        let mut names = [
            "frontend/w10",
            "ops",
            "backend/api",
            "frontend/w2",
            "frontend/w1",
            "backend/worker",
        ];
        names.sort_by(|left, right| natural_cmp(left, right));
        assert_eq!(
            names,
            [
                "backend/api",
                "backend/worker",
                "frontend/w1",
                "frontend/w2",
                "frontend/w10",
                "ops",
            ]
        );
    }

    #[test]
    fn state_order_puts_actionable_work_before_activity_and_idle_states() {
        use super::sorting::state_order as state_priority;
        assert!(
            state_priority(WorkspacePhase::WaitingForInput)
                < state_priority(WorkspacePhase::Failed)
        );
        assert!(state_priority(WorkspacePhase::Failed) < state_priority(WorkspacePhase::Active));
        assert!(state_priority(WorkspacePhase::Active) < state_priority(WorkspacePhase::Idle));
        assert!(state_priority(WorkspacePhase::Idle) < state_priority(WorkspacePhase::Prepared));
        assert!(state_priority(WorkspacePhase::Prepared) < state_priority(WorkspacePhase::Closed));
    }
}
