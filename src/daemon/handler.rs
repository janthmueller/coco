use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tracing::error;

use crate::coordinator::{Coordinator, CoordinatorError};
use crate::protocol::{
    AuditRecordParams, DaemonMethod, EventListParams, HealthParams, HealthResult, TurnStartParams,
    WorkspaceCreateParams, WorkspaceDiffParams, WorkspaceGetParams, WorkspaceListParams,
};
use crate::rpc::{RpcErrorPayload, RpcHandler};

pub(crate) struct DaemonHandler {
    coordinator: Arc<Coordinator>,
}

impl DaemonHandler {
    pub(crate) fn new(coordinator: Arc<Coordinator>) -> Self {
        Self { coordinator }
    }
}

#[async_trait]
impl RpcHandler for DaemonHandler {
    async fn handle(&self, method: &str, params: Value) -> Result<Value, RpcErrorPayload> {
        let method = DaemonMethod::parse(method).ok_or_else(|| {
            RpcErrorPayload::new(
                "METHOD_NOT_FOUND",
                format!("unknown daemon method {method:?}"),
            )
        })?;
        match method {
            DaemonMethod::Health => {
                decode::<HealthParams>(params)?;
                encode(HealthResult {
                    status: "ok".to_owned(),
                })
            }
            DaemonMethod::RepositoryRegister => {
                execute(self.coordinator.register_repository(decode(params)?))
            }
            DaemonMethod::WorkspaceCreate => execute(
                self.coordinator
                    .create_workspace(decode::<WorkspaceCreateParams>(params)?)
                    .await,
            ),
            DaemonMethod::WorkspaceList => execute(self.coordinator.list_workspaces(decode::<
                WorkspaceListParams,
            >(
                params
            )?)),
            DaemonMethod::WorkspaceGet => execute(self.coordinator.get_workspace(decode::<
                WorkspaceGetParams,
            >(
                params
            )?)),
            DaemonMethod::TurnStart => execute(
                self.coordinator
                    .start_turn(decode::<TurnStartParams>(params)?)
                    .await,
            ),
            DaemonMethod::EventList => execute(
                self.coordinator
                    .list_events(decode::<EventListParams>(params)?),
            ),
            DaemonMethod::WorkspaceDiff => execute(self.coordinator.workspace_diff(decode::<
                WorkspaceDiffParams,
            >(
                params
            )?)),
            DaemonMethod::AuditRecord => execute(self.coordinator.record_audit(decode::<
                AuditRecordParams,
            >(
                params
            )?)),
        }
    }
}

fn decode<T>(params: Value) -> Result<T, RpcErrorPayload>
where
    T: DeserializeOwned,
{
    serde_json::from_value(params).map_err(|source| {
        RpcErrorPayload::new(
            "INVALID_PARAMS",
            format!("invalid request parameters: {source}"),
        )
    })
}

fn execute<T>(result: Result<T, CoordinatorError>) -> Result<Value, RpcErrorPayload>
where
    T: Serialize,
{
    result.map_err(map_coordinator_error).and_then(encode)
}

fn encode<T>(result: T) -> Result<Value, RpcErrorPayload>
where
    T: Serialize,
{
    serde_json::to_value(result).map_err(|source| {
        error!(%source, "daemon result serialization failed");
        RpcErrorPayload::new("INTERNAL", "CoCo could not encode the result")
    })
}

fn map_coordinator_error(source: CoordinatorError) -> RpcErrorPayload {
    let code = source.code();
    let message = match &source {
        CoordinatorError::Worker(error) => {
            error!(%error, "Codex operation failed");
            "Codex could not accept the operation".to_owned()
        }
        CoordinatorError::Store(error) => {
            error!(%error, "persistence operation failed");
            if code == "INTERNAL" {
                "CoCo could not persist the operation".to_owned()
            } else {
                source.to_string()
            }
        }
        _ => source.to_string(),
    };
    RpcErrorPayload::new(code, message)
}
