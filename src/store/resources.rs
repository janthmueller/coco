use std::collections::HashMap;

use rusqlite::{OptionalExtension, TransactionBehavior, params};

use super::rows::require_workspace;
use super::{Store, StoreError, json_to_sql_error, now_ms};
use crate::domain::runtime::{WorkspaceResourcePolicy, WorkspaceResourcePolicySnapshot};

impl Store {
    pub(crate) fn workspace_resource_policy(
        &self,
        workspace_id: &str,
    ) -> Result<WorkspaceResourcePolicySnapshot, StoreError> {
        let connection = self.lock()?;
        require_workspace(&connection, workspace_id)?;
        read_policy(&connection, workspace_id)
    }

    pub(crate) fn workspace_resource_policies(
        &self,
    ) -> Result<HashMap<String, WorkspaceResourcePolicySnapshot>, StoreError> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT workspace_id, revision, policy_json FROM workspace_resource_policies",
        )?;
        let rows = statement.query_map([], |row| {
            let workspace_id: String = row.get(0)?;
            let snapshot = map_policy(row, 1, 2)?;
            Ok((workspace_id, snapshot))
        })?;
        rows.collect::<Result<HashMap<_, _>, _>>()
            .map_err(StoreError::from)
    }

    pub(crate) fn replace_workspace_resource_policy(
        &self,
        workspace_id: &str,
        expected_revision: u64,
        policy: &WorkspaceResourcePolicy,
    ) -> Result<WorkspaceResourcePolicySnapshot, StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_workspace(&transaction, workspace_id)?;
        let current = read_policy(&transaction, workspace_id)?;
        if current.revision != expected_revision {
            return Err(StoreError::ResourcePolicyRevisionMismatch {
                expected: expected_revision,
                actual: current.revision,
            });
        }
        let revision =
            expected_revision
                .checked_add(1)
                .ok_or_else(|| StoreError::InvalidStoredValue {
                    field: "workspace resource policy revision",
                    value: expected_revision.to_string(),
                })?;
        let revision_sql = i64::try_from(revision).map_err(|_| StoreError::InvalidStoredValue {
            field: "workspace resource policy revision",
            value: revision.to_string(),
        })?;
        let policy_json = serde_json::to_string(policy).map_err(json_to_sql_error)?;
        transaction.execute(
            "INSERT INTO workspace_resource_policies (
                workspace_id, revision, policy_json, updated_at_ms
             ) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(workspace_id) DO UPDATE SET
                revision = excluded.revision,
                policy_json = excluded.policy_json,
                updated_at_ms = excluded.updated_at_ms",
            params![workspace_id, revision_sql, policy_json, now_ms()],
        )?;
        transaction.commit()?;
        Ok(WorkspaceResourcePolicySnapshot {
            revision,
            policy: policy.clone(),
        })
    }
}

fn read_policy(
    connection: &rusqlite::Connection,
    workspace_id: &str,
) -> Result<WorkspaceResourcePolicySnapshot, StoreError> {
    connection
        .query_row(
            "SELECT revision, policy_json FROM workspace_resource_policies WHERE workspace_id = ?1",
            [workspace_id],
            |row| map_policy(row, 0, 1),
        )
        .optional()
        .map(Option::unwrap_or_default)
        .map_err(StoreError::from)
}

fn map_policy(
    row: &rusqlite::Row<'_>,
    revision_index: usize,
    policy_index: usize,
) -> rusqlite::Result<WorkspaceResourcePolicySnapshot> {
    let revision: i64 = row.get(revision_index)?;
    let revision = u64::try_from(revision).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            revision_index,
            rusqlite::types::Type::Integer,
            Box::new(StoreError::InvalidStoredValue {
                field: "workspace resource policy revision",
                value: revision.to_string(),
            }),
        )
    })?;
    let encoded: String = row.get(policy_index)?;
    let policy = serde_json::from_str::<WorkspaceResourcePolicy>(&encoded).map_err(|source| {
        rusqlite::Error::FromSqlConversionFailure(
            policy_index,
            rusqlite::types::Type::Text,
            Box::new(source),
        )
    })?;
    policy.validate().map_err(|source| {
        rusqlite::Error::FromSqlConversionFailure(
            policy_index,
            rusqlite::types::Type::Text,
            Box::new(source),
        )
    })?;
    Ok(WorkspaceResourcePolicySnapshot { revision, policy })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::store::tests::{ready_workspace, repository};

    #[test]
    fn policies_are_versioned_and_removed_with_their_workspace() {
        let store = Store::in_memory().unwrap();
        let repository = repository(Path::new("/tmp/resource-policy"));
        store.register_repository(&repository).unwrap();
        let workspace = ready_workspace(&store, &repository.id, "limits");

        assert_eq!(
            store.workspace_resource_policy(&workspace.id).unwrap(),
            WorkspaceResourcePolicySnapshot::default()
        );
        let policy = WorkspaceResourcePolicy {
            memory_max_bytes: Some(512 * 1024 * 1024),
            cpu_max_millicores: Some(1_500),
            ..WorkspaceResourcePolicy::default()
        };
        let snapshot = store
            .replace_workspace_resource_policy(&workspace.id, 0, &policy)
            .unwrap();
        assert_eq!(snapshot.revision, 1);
        assert_eq!(
            store.workspace_resource_policy(&workspace.id).unwrap(),
            snapshot
        );
        assert!(matches!(
            store.replace_workspace_resource_policy(&workspace.id, 0, &policy),
            Err(StoreError::ResourcePolicyRevisionMismatch {
                expected: 0,
                actual: 1
            })
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
        assert!(store.workspace_resource_policies().unwrap().is_empty());
    }
}
