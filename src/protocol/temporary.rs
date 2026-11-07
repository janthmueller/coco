use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspaceAttachFinishParams {
    pub workspace_id: String,
    pub lease_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum WorkspaceCleanupResult {
    Deleted,
    Retained { reason: String },
    Incomplete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspaceAttachFinishResult {
    pub cleanup: Option<WorkspaceCleanupResult>,
}

impl super::DaemonRequest for WorkspaceAttachFinishParams {
    type Response = WorkspaceAttachFinishResult;
    const METHOD: super::DaemonMethod = super::DaemonMethod::WorkspaceAttachFinish;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspaceAttachDisarmCleanupParams {
    pub workspace_id: String,
    pub lease_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspaceAttachDisarmCleanupResult {}

impl super::DaemonRequest for WorkspaceAttachDisarmCleanupParams {
    type Response = WorkspaceAttachDisarmCleanupResult;
    const METHOD: super::DaemonMethod = super::DaemonMethod::WorkspaceAttachDisarmCleanup;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::TemporaryWorkspacePolicy;
    use serde_json::json;

    #[test]
    fn finish_requires_the_exact_workspace_and_lease_and_rejects_extra_targets() {
        let valid = json!({"workspaceId": "workspace", "leaseId": "lease"});
        let request: WorkspaceAttachFinishParams = serde_json::from_value(valid.clone()).unwrap();
        assert_eq!(serde_json::to_value(request).unwrap(), valid);
        for invalid in [
            json!({"workspaceId": "workspace"}),
            json!({"leaseId": "lease"}),
            json!({"workspaceId": "workspace", "leaseId": "lease", "threadId": "other"}),
        ] {
            assert!(serde_json::from_value::<WorkspaceAttachFinishParams>(invalid).is_err());
        }
    }

    #[test]
    fn policy_is_explicitly_typed_and_finish_distinguishes_retention_from_incomplete_cleanup() {
        assert!(
            !serde_json::from_value::<TemporaryWorkspacePolicy>(json!({}))
                .unwrap()
                .keep_thread
        );
        assert!(serde_json::from_value::<TemporaryWorkspacePolicy>(json!(true)).is_err());
        assert!(
            serde_json::from_value::<TemporaryWorkspacePolicy>(json!({"discardChanges": true}))
                .is_err()
        );
        for result in [
            WorkspaceCleanupResult::Deleted,
            WorkspaceCleanupResult::Retained {
                reason: "busy".into(),
            },
            WorkspaceCleanupResult::Incomplete,
        ] {
            assert_eq!(
                serde_json::from_value::<WorkspaceCleanupResult>(
                    serde_json::to_value(&result).unwrap()
                )
                .unwrap(),
                result
            );
        }
    }
}
