use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tracing::error;

use crate::codex::CodexError;
use crate::coordinator::{Coordinator, CoordinatorError, WorkerError};
use crate::protocol::{
    AuditRecordParams, DaemonMethod, DecisionGetParams, DecisionRespondParams, EventListParams,
    HealthParams, HealthResult, HookDeliveryListParams, HookListParams, HookReloadParams,
    ModelListParams, RepositoryResolveParams, TurnResultParams, TurnStartParams,
    WorkspaceAttachAdoptParams, WorkspaceAttachParams, WorkspaceAttachReleaseParams,
    WorkspaceAttachRenewParams, WorkspaceCloseParams, WorkspaceCreateParams, WorkspaceDeleteParams,
    WorkspaceDiffParams, WorkspaceGetParams, WorkspaceListParams, WorkspaceReopenParams,
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
            DaemonMethod::ModelList => {
                decode::<ModelListParams>(params)?;
                execute(self.coordinator.list_models().await)
            }
            DaemonMethod::RepositoryRegister => {
                execute(self.coordinator.register_repository(decode(params)?))
            }
            DaemonMethod::SignalCatalogLoad => {
                execute(self.coordinator.load_signal_catalog(decode(params)?))
            }
            DaemonMethod::SignalTypeList => {
                execute(self.coordinator.list_signal_types(decode(params)?))
            }
            DaemonMethod::SignalEmit => execute(self.coordinator.emit_signal(decode(params)?)),
            DaemonMethod::SignalList => execute(self.coordinator.list_signals(decode(params)?)),
            DaemonMethod::HookList => {
                let params = decode::<HookListParams>(params)?;
                encode(self.coordinator.list_hooks(params))
            }
            DaemonMethod::HookReload => execute(self.coordinator.reload_hooks(decode::<
                HookReloadParams,
            >(
                params
            )?)),
            DaemonMethod::HookDeliveryList => execute(
                self.coordinator
                    .list_hook_deliveries(decode::<HookDeliveryListParams>(params)?),
            ),
            DaemonMethod::RepositoryResolve => execute(
                self.coordinator
                    .resolve_repository(decode::<RepositoryResolveParams>(params)?),
            ),
            DaemonMethod::RepositoryList => {
                execute(self.coordinator.list_repositories(decode(params)?))
            }
            method @ (DaemonMethod::WorkspaceCreate
            | DaemonMethod::WorkspaceClose
            | DaemonMethod::WorkspaceReopen
            | DaemonMethod::WorkspaceDelete
            | DaemonMethod::WorkspaceList
            | DaemonMethod::WorkspaceGet
            | DaemonMethod::WorkspaceAttach
            | DaemonMethod::WorkspaceAttachRenew
            | DaemonMethod::WorkspaceAttachAdopt
            | DaemonMethod::WorkspaceAttachRelease
            | DaemonMethod::WorkspaceDiff) => self.handle_workspace(method, params).await,
            DaemonMethod::TurnStart => execute(
                self.coordinator
                    .start_turn(decode::<TurnStartParams>(params)?)
                    .await,
            ),
            DaemonMethod::TurnResult => execute(self.coordinator.turn_result(decode::<
                TurnResultParams,
            >(
                params
            )?)),
            DaemonMethod::EventList => execute(
                self.coordinator
                    .list_events(decode::<EventListParams>(params)?)
                    .await,
            ),
            DaemonMethod::DecisionGet => execute(self.coordinator.get_decision(decode::<
                DecisionGetParams,
            >(
                params
            )?)),
            DaemonMethod::DecisionRespond => execute(
                self.coordinator
                    .respond_decision(decode::<DecisionRespondParams>(params)?)
                    .await,
            ),
            DaemonMethod::AuditRecord => execute(self.coordinator.record_audit(decode::<
                AuditRecordParams,
            >(
                params
            )?)),
        }
    }
}

impl DaemonHandler {
    async fn handle_workspace(
        &self,
        method: DaemonMethod,
        params: Value,
    ) -> Result<Value, RpcErrorPayload> {
        match method {
            DaemonMethod::WorkspaceCreate => execute(
                self.coordinator
                    .create_workspace(decode::<WorkspaceCreateParams>(params)?)
                    .await,
            ),
            DaemonMethod::WorkspaceClose => execute(
                self.coordinator
                    .close_workspace(decode::<WorkspaceCloseParams>(params)?)
                    .await,
            ),
            DaemonMethod::WorkspaceReopen => execute(
                self.coordinator
                    .reopen_workspace(decode::<WorkspaceReopenParams>(params)?)
                    .await,
            ),
            DaemonMethod::WorkspaceDelete => execute(
                self.coordinator
                    .delete_workspace(decode::<WorkspaceDeleteParams>(params)?)
                    .await,
            ),
            DaemonMethod::WorkspaceList => execute(
                self.coordinator
                    .list_workspaces(decode::<WorkspaceListParams>(params)?)
                    .await,
            ),
            DaemonMethod::WorkspaceGet => execute(
                self.coordinator
                    .get_workspace(decode::<WorkspaceGetParams>(params)?)
                    .await,
            ),
            DaemonMethod::WorkspaceAttach => execute(
                self.coordinator
                    .attach_workspace(decode::<WorkspaceAttachParams>(params)?)
                    .await,
            ),
            DaemonMethod::WorkspaceAttachRenew => execute(
                self.coordinator
                    .renew_workspace_attach(decode::<WorkspaceAttachRenewParams>(params)?),
            ),
            DaemonMethod::WorkspaceAttachAdopt => execute(
                self.coordinator
                    .adopt_workspace_thread(decode::<WorkspaceAttachAdoptParams>(params)?)
                    .await,
            ),
            DaemonMethod::WorkspaceAttachRelease => execute(
                self.coordinator
                    .release_workspace_attach(decode::<WorkspaceAttachReleaseParams>(params)?),
            ),
            DaemonMethod::WorkspaceDiff => execute(self.coordinator.workspace_diff(decode::<
                WorkspaceDiffParams,
            >(
                params
            )?)),
            _ => unreachable!("non-workspace method routed to workspace handler"),
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
    let data = source.data();
    let message = match &source {
        CoordinatorError::Worker(error) => {
            error!(%error, "Codex operation failed");
            public_worker_error(error)
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
    RpcErrorPayload {
        code: code.to_owned(),
        message,
        data,
    }
}

fn public_worker_error(error: &WorkerError) -> String {
    let WorkerError::Runtime(source) = error else {
        return "Codex returned a response CoCo could not use".to_owned();
    };
    let Some(CodexError::Rpc { code, message, .. }) = source.downcast_ref::<CodexError>() else {
        return "Codex could not accept the operation".to_owned();
    };
    format!(
        "Codex rejected the operation ({code}): {}",
        bounded_single_line(message, 512)
    )
}

fn bounded_single_line(value: &str, limit: usize) -> String {
    let mut sanitized = value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .take(limit + 1)
        .collect::<String>();
    if sanitized.chars().count() > limit {
        sanitized = sanitized.chars().take(limit.saturating_sub(1)).collect();
        sanitized.push('…');
    }
    sanitized
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn exposes_only_the_bounded_codex_rpc_message() {
        let payload = map_coordinator_error(CoordinatorError::Worker(WorkerError::runtime(
            CodexError::Rpc {
                code: -32600,
                message: "experimental capability required\n\u{1b}[31m".to_owned(),
                data: Some(json!({"secret": "must-not-escape"})),
            },
        )));

        assert_eq!(payload.code, "CODEX_ERROR");
        assert_eq!(
            payload.message,
            "Codex rejected the operation (-32600): experimental capability required  [31m"
        );
        assert_eq!(payload.data, None);
        assert!(!payload.message.contains("must-not-escape"));
    }

    #[test]
    fn keeps_non_rpc_runtime_failures_private() {
        let payload = map_coordinator_error(CoordinatorError::Worker(WorkerError::runtime(
            std::io::Error::other("private transport detail"),
        )));

        assert_eq!(payload.message, "Codex could not accept the operation");
    }
}
