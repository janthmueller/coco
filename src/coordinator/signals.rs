use super::{Coordinator, CoordinatorError};
use crate::domain::hooks::HookEventKind;
use crate::domain::signals::{
    MAX_SIGNAL_PAGE, Signal, SignalError, SignalFilter, SignalPage, SignalType,
};
use crate::protocol::{
    RepositoryScope, SignalCatalogLoadParams, SignalEmitParams, SignalListParams,
    SignalTypeListParams,
};
use uuid::Uuid;

mod catalog;
mod schema;

impl Coordinator {
    pub(crate) fn load_signal_catalog(
        &self,
        params: SignalCatalogLoadParams,
    ) -> Result<Vec<SignalType>, CoordinatorError> {
        let (repository, _) = self.registered_repository_for_path(&params.repository)?;
        let definitions = catalog::load(&params.directory, &repository.id)?;
        Ok(self.store.register_signal_types(definitions)?)
    }

    pub(crate) fn list_signal_types(
        &self,
        params: SignalTypeListParams,
    ) -> Result<Vec<SignalType>, CoordinatorError> {
        let (repository, _) = self.registered_repository_for_path(&params.repository)?;
        Ok(self.store.list_signal_types(&repository.id)?)
    }

    pub(crate) fn emit_signal(&self, params: SignalEmitParams) -> Result<Signal, CoordinatorError> {
        schema::validate_name(&params.name)?;
        schema::validate_version(params.version)?;
        schema::validate_json(&params.payload)?;
        if params.idempotency_key.trim().is_empty() || params.idempotency_key.len() > 256 {
            return Err(CoordinatorError::InvalidParams(
                "signal idempotency key must contain 1–256 bytes".into(),
            ));
        }
        let (repository, _) = self.registered_repository_for_path(&params.repository)?;
        let workspace = self
            .store
            .workspace_by_thread_id(&params.thread_id)?
            .filter(|workspace| workspace.repository_id == repository.id)
            .ok_or(SignalError::InvalidSender)?;
        let definition = self
            .store
            .signal_type(&repository.id, &params.name, params.version)?
            .ok_or(SignalError::UnknownType)?;
        if let Some(schema) = &definition.payload_schema {
            let validator = schema::compile(schema)?;
            if let Err(error) = validator.validate(&params.payload) {
                let path = error.instance_path().to_string();
                let path = if path.is_empty() { "(root)" } else { &path };
                return Err(SignalError::InvalidPayload(format!(
                    "{path} (schema rule {}); inspect signals.types and correct the payload before retrying",
                    error.schema_path()
                )).into());
            }
        }
        let signal = Signal {
            id: Uuid::now_v7().to_string(),
            sequence: 0,
            repository_id: repository.id.clone(),
            workspace_id: workspace.id.clone(),
            repository_name: repository.display_name.clone(),
            workspace_name: workspace.name.clone(),
            thread_id: params.thread_id,
            name: params.name,
            version: params.version,
            payload: params.payload,
            idempotency_key: params.idempotency_key,
            occurred_at_ms: 0,
        };
        let hook = self.hooks.event(
            HookEventKind::SignalEmitted,
            &repository,
            &workspace,
            serde_json::json!({
                "signalId": signal.id,
                "name": signal.name,
                "version": signal.version,
                "payload": signal.payload,
            }),
        );
        let notify = hook.is_some();
        let signal = self.store.emit_signal_with_hook(signal, hook)?;
        if notify {
            self.hooks.notify();
        }
        Ok(signal)
    }

    pub(crate) fn list_signals(
        &self,
        params: SignalListParams,
    ) -> Result<SignalPage, CoordinatorError> {
        if !(1..=MAX_SIGNAL_PAGE).contains(&params.limit) {
            return Err(CoordinatorError::InvalidParams(
                "signal page limit must be 1–100".into(),
            ));
        }
        if let Some(name) = &params.name {
            schema::validate_name(name)?;
        }
        if params
            .workspace_id
            .as_ref()
            .is_some_and(|id| Uuid::parse_str(id).is_err())
        {
            return Err(CoordinatorError::InvalidParams(
                "signal workspace filter must be a workspace UUID".into(),
            ));
        }
        if params
            .after
            .as_ref()
            .is_some_and(|cursor| cursor.len() > 256)
        {
            return Err(SignalError::InvalidCursor.into());
        }
        let repository_id = match params.scope {
            RepositoryScope::Repository { path } => {
                Some(self.registered_repository_for_path(&path)?.0.id)
            }
            RepositoryScope::AllRepositories => None,
        };
        Ok(self.store.list_signals(
            &SignalFilter {
                repository_id,
                workspace_id: params.workspace_id,
                name: params.name,
            },
            params.after.as_deref(),
            params.limit,
        )?)
    }
}
