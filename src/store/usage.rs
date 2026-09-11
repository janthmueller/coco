use rusqlite::{OptionalExtension, TransactionBehavior, params};

use super::rows::require_workspace;
use super::{Store, StoreError, json_to_sql_error, now_ms};
use crate::domain::usage::WorkspaceTokenUsageCheckpoint;

impl Store {
    /// Stores the newest monotonic cumulative checkpoint for one exact thread
    /// binding. Returns false when an older notification was ignored.
    pub(crate) fn observe_workspace_token_usage(
        &self,
        workspace_id: &str,
        checkpoint: &WorkspaceTokenUsageCheckpoint,
    ) -> Result<bool, StoreError> {
        checkpoint.validate()?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let workspace = require_workspace(&transaction, workspace_id)?;
        if workspace.codex_thread_id.as_deref() != Some(&checkpoint.thread_id) {
            return Err(StoreError::InvalidWorkspaceTransition {
                workspace_id: workspace_id.to_owned(),
                expected: format!("Codex thread {}", checkpoint.thread_id),
                actual: workspace.codex_thread_id.as_deref().map_or_else(
                    || "no Codex thread".to_owned(),
                    |id| format!("Codex thread {id}"),
                ),
            });
        }

        if let Some(current) = read_checkpoint(&transaction, workspace_id)?
            && current.thread_id == checkpoint.thread_id
            && current.total.total_tokens > checkpoint.total.total_tokens
        {
            transaction.commit()?;
            return Ok(false);
        }

        let encoded = serde_json::to_string(checkpoint).map_err(json_to_sql_error)?;
        transaction.execute(
            "INSERT INTO workspace_token_usage (
                workspace_id, thread_id, checkpoint_json, updated_at_ms
             ) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(workspace_id) DO UPDATE SET
                thread_id = excluded.thread_id,
                checkpoint_json = excluded.checkpoint_json,
                updated_at_ms = excluded.updated_at_ms",
            params![workspace_id, checkpoint.thread_id, encoded, now_ms()],
        )?;
        transaction.commit()?;
        Ok(true)
    }

    pub(crate) fn workspace_token_usage(
        &self,
        workspace_id: &str,
    ) -> Result<Option<WorkspaceTokenUsageCheckpoint>, StoreError> {
        let connection = self.lock()?;
        require_workspace(&connection, workspace_id)?;
        read_checkpoint(&connection, workspace_id)
    }
}

fn read_checkpoint(
    connection: &rusqlite::Connection,
    workspace_id: &str,
) -> Result<Option<WorkspaceTokenUsageCheckpoint>, StoreError> {
    connection
        .query_row(
            "SELECT thread_id, checkpoint_json FROM workspace_token_usage
             WHERE workspace_id = ?1",
            [workspace_id],
            |row| {
                let thread_id: String = row.get(0)?;
                let encoded: String = row.get(1)?;
                let checkpoint = serde_json::from_str::<WorkspaceTokenUsageCheckpoint>(&encoded)
                    .map_err(|source| {
                        rusqlite::Error::FromSqlConversionFailure(
                            1,
                            rusqlite::types::Type::Text,
                            Box::new(source),
                        )
                    })?;
                checkpoint.validate().map_err(|source| {
                    rusqlite::Error::FromSqlConversionFailure(
                        1,
                        rusqlite::types::Type::Text,
                        Box::new(source),
                    )
                })?;
                if checkpoint.thread_id != thread_id {
                    return Err(rusqlite::Error::FromSqlConversionFailure(
                        1,
                        rusqlite::types::Type::Text,
                        Box::new(StoreError::InvalidStoredValue {
                            field: "workspace token usage thread identity",
                            value: format!("{thread_id} != {}", checkpoint.thread_id),
                        }),
                    ));
                }
                Ok(checkpoint)
            },
        )
        .optional()
        .map_err(StoreError::from)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::domain::usage::{TokenUsageBreakdown, WORKSPACE_TOKEN_USAGE_SCHEMA_VERSION};
    use crate::store::tests::{ready_workspace, repository};

    fn checkpoint(thread_id: &str, total_tokens: u64) -> WorkspaceTokenUsageCheckpoint {
        WorkspaceTokenUsageCheckpoint {
            schema_version: WORKSPACE_TOKEN_USAGE_SCHEMA_VERSION,
            thread_id: thread_id.to_owned(),
            turn_id: "turn-1".to_owned(),
            total: TokenUsageBreakdown {
                total_tokens,
                input_tokens: total_tokens.saturating_sub(10),
                output_tokens: total_tokens.min(10),
                ..TokenUsageBreakdown::default()
            },
            last: TokenUsageBreakdown::default(),
            model_context_window: Some(200_000),
            runtime_generation: "runtime-1".to_owned(),
            observed_at_ms: 1,
        }
    }

    #[test]
    fn checkpoints_are_binding_safe_monotonic_and_cascade_on_delete() {
        let store = Store::in_memory().unwrap();
        let repository = repository(Path::new("/tmp/workspace-usage"));
        store.register_repository(&repository).unwrap();
        let workspace = ready_workspace(&store, &repository.id, "usage");
        let thread_id = workspace.codex_thread_id.as_deref().unwrap();

        assert!(
            store
                .observe_workspace_token_usage(&workspace.id, &checkpoint(thread_id, 120))
                .unwrap()
        );
        assert_eq!(
            store
                .workspace_token_usage(&workspace.id)
                .unwrap()
                .unwrap()
                .total
                .total_tokens,
            120
        );
        assert!(
            !store
                .observe_workspace_token_usage(&workspace.id, &checkpoint(thread_id, 100))
                .unwrap()
        );
        assert!(matches!(
            store.observe_workspace_token_usage(&workspace.id, &checkpoint("other", 130)),
            Err(StoreError::InvalidWorkspaceTransition { .. })
        ));

        store
            .begin_workspace_deletion(
                &workspace.id,
                crate::store::WorkspaceDeletionIntent {
                    from_open: true,
                    ..Default::default()
                },
                None,
            )
            .unwrap();
        store.delete_workspace_record(&workspace.id).unwrap();
        let count: i64 = store
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM workspace_token_usage", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn migrates_v13_without_changing_existing_workspace_state() {
        let store = Store::in_memory().unwrap();
        let repository = repository(Path::new("/tmp/workspace-usage-migration"));
        store.register_repository(&repository).unwrap();
        let workspace = ready_workspace(&store, &repository.id, "usage-migration");
        let connection = store.lock().unwrap();
        connection
            .execute_batch("DROP TABLE workspace_token_usage; PRAGMA user_version = 13;")
            .unwrap();

        crate::store::migrations::migrate(&connection).unwrap();

        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            14
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT count(*) FROM workspaces WHERE id = ?1",
                    [&workspace.id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT count(*) FROM sqlite_schema
                     WHERE type = 'table' AND name = 'workspace_token_usage'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
    }
}
