use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use serde_json::{Map, Value, json};
use tracing::{error, warn};

use super::{Coordinator, CoordinatorError};
use crate::domain::{
    DecisionApprovalPrompt, DecisionFileChange, DecisionKind, DecisionOption, DecisionPermission,
    DecisionPrompt, DecisionQuestion, DecisionState, EventKind, EventSource, Turn, Workspace,
};
use crate::protocol::{
    DecisionGetParams, DecisionRespondParams, DecisionResult, DecisionSubmission,
};
use crate::store::{EventDraft, NewDecision, StoreError, StoredDecision};

const MAX_PRESENTATION_STRING_BYTES: usize = 32 * 1024;
const MAX_REASON_BYTES: usize = 4 * 1024;
const MAX_FILE_DIFF_BYTES: usize = 64 * 1024;

struct ProjectedDecision {
    kind: DecisionKind,
    prompt: DecisionPrompt,
    native_options: Vec<Value>,
}

impl Coordinator {
    pub(super) fn capture_decision_request(
        &self,
        native_request_id: Value,
        method: &str,
        params: &Value,
        workspace: &Workspace,
        turn: Option<&Turn>,
    ) -> Result<bool, StoreError> {
        let Some(projected) = self.project_decision(method, params) else {
            return Ok(false);
        };
        let Some(codex_thread_id) = params
            .get("threadId")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
        else {
            warn!(
                method,
                "ignoring decision request without a direct threadId"
            );
            return Ok(false);
        };
        let codex_turn_id = params
            .get("turnId")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let kind = projected.kind;
        self.store.create_decision_with_event(
            NewDecision {
                workspace_id: workspace.id.clone(),
                turn_id: turn.map(|turn| turn.id.clone()),
                codex_thread_id,
                codex_turn_id,
                runtime_generation: self.runtime_generation.clone(),
                native_request_id,
                method: method.to_owned(),
                kind,
                prompt: projected.prompt,
                native_options: projected.native_options,
            },
            EventDraft {
                workspace_id: Some(workspace.id.clone()),
                turn_id: turn.map(|turn| turn.id.clone()),
                kind: EventKind::DecisionRequested,
                source: EventSource::Codex,
                source_method: Some(method.to_owned()),
                occurred_at_ms: params.get("startedAtMs").and_then(Value::as_i64),
                payload: json!({
                    "kind": kind,
                    "itemId": params.get("itemId"),
                }),
            },
        )?;
        Ok(true)
    }

    pub(super) fn cache_file_change_preview(&self, params: &Value) {
        let Some(thread_id) = params.get("threadId").and_then(Value::as_str) else {
            return;
        };
        let Some(item_id) = params
            .pointer("/item/id")
            .or_else(|| params.get("itemId"))
            .and_then(Value::as_str)
        else {
            return;
        };
        let changes = params
            .pointer("/item/changes")
            .or_else(|| params.get("changes"))
            .and_then(Value::as_array)
            .and_then(|changes| project_file_changes(changes));
        let Some(changes) = changes else {
            warn!("discarding a file-change preview that cannot be presented safely");
            return;
        };
        self.file_change_previews
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert((thread_id.to_owned(), item_id.to_owned()), changes);
    }

    pub(super) fn forget_file_change_preview(&self, params: &Value) {
        let Some(thread_id) = params.get("threadId").and_then(Value::as_str) else {
            return;
        };
        let Some(item_id) = params
            .pointer("/item/id")
            .or_else(|| params.get("itemId"))
            .and_then(Value::as_str)
        else {
            return;
        };
        self.file_change_previews
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&(thread_id.to_owned(), item_id.to_owned()));
    }

    pub(super) fn clear_file_change_previews(&self) {
        self.file_change_previews
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }

    pub(crate) fn get_decision(
        &self,
        params: DecisionGetParams,
    ) -> Result<DecisionResult, CoordinatorError> {
        let stored = self.require_decision(&params.decision_id)?;
        self.decision_result(stored)
    }

    pub(crate) async fn respond_decision(
        &self,
        params: DecisionRespondParams,
    ) -> Result<DecisionResult, CoordinatorError> {
        let stored = self.require_decision(&params.decision_id)?;
        if stored.runtime_generation != self.runtime_generation {
            return Err(StoreError::DecisionGenerationMismatch {
                decision_id: stored.decision.id,
            }
            .into());
        }
        if stored.decision.state != DecisionState::Pending {
            return Err(StoreError::InvalidDecisionState {
                decision_id: stored.decision.id,
                actual: stored.decision.state.as_str().to_owned(),
            }
            .into());
        }
        let (native_response, summary) = response_for_submission(&stored, params.submission)?;
        let submitted = self.store.mark_decision_submitted(
            &params.decision_id,
            &self.runtime_generation,
            &summary,
        )?;
        if let Err(source) = self
            .worker
            .respond_to_request(submitted.native_request_id.clone(), native_response)
            .await
        {
            if let Err(store_error) = self.store.orphan_submitted_decision(
                &params.decision_id,
                &self.runtime_generation,
                "native_response_failed",
            ) {
                error!(decision_id = %params.decision_id, %store_error, "could not orphan a failed decision response");
            }
            return Err(CoordinatorError::Worker(source));
        }
        self.decision_result(submitted)
    }

    fn require_decision(&self, id: &str) -> Result<StoredDecision, CoordinatorError> {
        self.store
            .decision_by_id(id)?
            .ok_or_else(|| StoreError::NotFound {
                entity: "decision",
                id: id.to_owned(),
            })
            .map_err(CoordinatorError::from)
    }

    fn decision_result(&self, stored: StoredDecision) -> Result<DecisionResult, CoordinatorError> {
        let workspace = self
            .store
            .workspace_by_id(&stored.decision.workspace_id)?
            .ok_or_else(|| StoreError::NotFound {
                entity: "workspace",
                id: stored.decision.workspace_id.clone(),
            })?;
        let repository = self.repository_by_id(&workspace.repository_id)?;
        Ok(DecisionResult {
            decision: stored.decision,
            workspace,
            repository: (&repository).into(),
        })
    }

    fn project_decision(&self, method: &str, params: &Value) -> Option<ProjectedDecision> {
        match method {
            "item/commandExecution/requestApproval" => project_command_approval(params),
            "item/fileChange/requestApproval" => {
                let changes = decision_preview_key(params).and_then(|key| {
                    self.file_change_previews
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .get(&key)
                        .cloned()
                });
                project_file_change_approval(params, changes?)
            }
            "item/tool/requestUserInput" => project_user_input(params),
            _ => None,
        }
    }
}

fn response_for_submission(
    stored: &StoredDecision,
    submission: DecisionSubmission,
) -> Result<(Value, Value), CoordinatorError> {
    match (&stored.decision.prompt, submission) {
        (DecisionPrompt::Approval(prompt), DecisionSubmission::Choice { choice }) => {
            let options = &prompt.options;
            let index = choice
                .checked_sub(1)
                .and_then(|choice| usize::try_from(choice).ok())
                .ok_or_else(|| {
                    CoordinatorError::InvalidParams("choice must be at least 1".into())
                })?;
            let native = stored.native_options.get(index).ok_or_else(|| {
                CoordinatorError::InvalidParams(format!(
                    "choice must be between 1 and {}",
                    options.len()
                ))
            })?;
            if options.len() != stored.native_options.len() {
                return Err(CoordinatorError::InvalidParams(
                    "the stored decision options are inconsistent".to_owned(),
                ));
            }
            Ok((
                json!({"decision": native}),
                json!({"choice": choice, "label": options[index].label}),
            ))
        }
        (DecisionPrompt::UserInput { questions }, DecisionSubmission::Answers { answers }) => {
            validate_answers(questions, &answers)?;
            let native_answers = questions
                .iter()
                .map(|question| {
                    (
                        question.id.clone(),
                        json!({"answers": [answers[&question.id]]}),
                    )
                })
                .collect::<Map<_, _>>();
            Ok((
                json!({"answers": native_answers}),
                json!({
                    "questionIds": questions.iter().map(|question| &question.id).collect::<Vec<_>>(),
                    "secretQuestionCount": questions.iter().filter(|question| question.is_secret).count(),
                }),
            ))
        }
        (DecisionPrompt::Approval(_), DecisionSubmission::Answers { .. }) => Err(
            CoordinatorError::InvalidParams("this request expects a numbered choice".to_owned()),
        ),
        (DecisionPrompt::UserInput { .. }, DecisionSubmission::Choice { .. }) => {
            Err(CoordinatorError::InvalidParams(
                "this request expects answers to its questions".to_owned(),
            ))
        }
    }
}

fn validate_answers(
    questions: &[DecisionQuestion],
    answers: &BTreeMap<String, String>,
) -> Result<(), CoordinatorError> {
    let expected = questions
        .iter()
        .map(|question| question.id.as_str())
        .collect::<HashSet<_>>();
    if answers.len() != expected.len() || answers.keys().any(|id| !expected.contains(id.as_str())) {
        return Err(CoordinatorError::InvalidParams(
            "answers must contain exactly one entry for every question".to_owned(),
        ));
    }
    for question in questions {
        let answer = &answers[&question.id];
        if answer.trim().is_empty() {
            return Err(CoordinatorError::InvalidParams(format!(
                "answer for question {:?} must not be empty",
                question.id
            )));
        }
        if !question.options.is_empty()
            && !question.allows_other
            && !question
                .options
                .iter()
                .any(|option| option.label == *answer)
        {
            return Err(CoordinatorError::InvalidParams(format!(
                "answer for question {:?} must be one of its listed options",
                question.id
            )));
        }
    }
    Ok(())
}

fn project_command_approval(params: &Value) -> Option<ProjectedDecision> {
    let (options, native_options) = project_command_options(params.get("availableDecisions"))?;
    let additional_permissions = project_additional_permissions(params)?;
    let (network_host, network_protocol) = project_network_context(params)?;
    Some(ProjectedDecision {
        kind: DecisionKind::CommandApproval,
        prompt: DecisionPrompt::Approval(Box::new(DecisionApprovalPrompt {
            title: "Codex wants to run a command".to_owned(),
            reason: bounded_optional_string(params.get("reason"), MAX_REASON_BYTES),
            command: bounded_optional_string(params.get("command"), MAX_PRESENTATION_STRING_BYTES),
            cwd: bounded_optional_string(params.get("cwd"), MAX_PRESENTATION_STRING_BYTES)
                .map(PathBuf::from),
            network_host,
            network_protocol,
            grant_root: None,
            additional_permissions,
            changes: Vec::new(),
            options,
        })),
        native_options,
    })
}

fn project_file_change_approval(
    params: &Value,
    changes: Vec<DecisionFileChange>,
) -> Option<ProjectedDecision> {
    if changes.is_empty() {
        warn!("leaving a file-change approval unanswered because no safe preview is available");
        return None;
    }
    let (options, native_options) = plain_approval_options();
    Some(ProjectedDecision {
        kind: DecisionKind::FileChangeApproval,
        prompt: DecisionPrompt::Approval(Box::new(DecisionApprovalPrompt {
            title: "Codex wants to change files".to_owned(),
            reason: bounded_optional_string(params.get("reason"), MAX_REASON_BYTES),
            command: None,
            cwd: None,
            network_host: None,
            network_protocol: None,
            grant_root: params
                .get("grantRoot")
                .and_then(|value| bounded_required_string(value, MAX_PRESENTATION_STRING_BYTES))
                .map(PathBuf::from),
            additional_permissions: Vec::new(),
            changes,
            options,
        })),
        native_options,
    })
}

fn project_user_input(params: &Value) -> Option<ProjectedDecision> {
    let questions = params.get("questions")?.as_array()?;
    if questions.is_empty() {
        warn!("ignoring requestUserInput without questions");
        return None;
    }
    let mut seen = HashSet::new();
    let mut projected = Vec::with_capacity(questions.len());
    for question in questions {
        let id = question.get("id")?.as_str()?.to_owned();
        if id.trim().is_empty() || !seen.insert(id.clone()) {
            warn!("ignoring requestUserInput with empty or duplicate question IDs");
            return None;
        }
        let header = bounded_required_string(question.get("header")?, MAX_REASON_BYTES)?;
        let prompt =
            bounded_required_string(question.get("question")?, MAX_PRESENTATION_STRING_BYTES)?;
        let options = match question.get("options") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(options)) => options
                .iter()
                .map(|option| {
                    Some(DecisionOption {
                        label: bounded_required_string(
                            option.get("label")?,
                            MAX_PRESENTATION_STRING_BYTES,
                        )?,
                        description: bounded_optional_string(
                            option.get("description"),
                            MAX_PRESENTATION_STRING_BYTES,
                        ),
                    })
                })
                .collect::<Option<Vec<_>>>()?,
            Some(_) => return None,
        };
        projected.push(DecisionQuestion {
            id,
            header,
            question: prompt,
            options,
            allows_other: question
                .get("isOther")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            is_secret: question
                .get("isSecret")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        });
    }
    Some(ProjectedDecision {
        kind: DecisionKind::UserInput,
        prompt: DecisionPrompt::UserInput {
            questions: projected,
        },
        native_options: Vec::new(),
    })
}

fn project_command_options(available: Option<&Value>) -> Option<(Vec<DecisionOption>, Vec<Value>)> {
    let Some(available) = available.filter(|value| !value.is_null()) else {
        return Some(plain_approval_options());
    };
    let available = available.as_array()?;
    if available.is_empty() {
        return None;
    }
    let mut options = Vec::with_capacity(available.len());
    let mut native = Vec::with_capacity(available.len());
    for value in available {
        let option = describe_native_decision(value)?;
        options.push(option);
        native.push(value.clone());
    }
    Some((options, native))
}

fn project_network_context(params: &Value) -> Option<(Option<String>, Option<String>)> {
    let Some(context) = params
        .get("networkApprovalContext")
        .filter(|value| !value.is_null())
    else {
        return Some((None, None));
    };
    if !object_has_exact_keys(context, &["host", "protocol"]) {
        return None;
    }
    let host = bounded_required_string(context.get("host")?, MAX_PRESENTATION_STRING_BYTES)?;
    let protocol = context.get("protocol")?.as_str()?;
    if !matches!(protocol, "http" | "https" | "socks5Tcp" | "socks5Udp") {
        return None;
    }
    Some((Some(host), Some(protocol.to_owned())))
}

fn plain_approval_options() -> (Vec<DecisionOption>, Vec<Value>) {
    let native = vec![
        json!("accept"),
        json!("acceptForSession"),
        json!("decline"),
        json!("cancel"),
    ];
    let options = native
        .iter()
        .map(|value| describe_native_decision(value).expect("built-in decisions are known"))
        .collect();
    (options, native)
}

fn describe_native_decision(value: &Value) -> Option<DecisionOption> {
    let (label, description) = match value.as_str() {
        Some("accept") => (
            "Approve once".to_owned(),
            "Run this operation once.".to_owned(),
        ),
        Some("acceptForSession") => (
            "Approve for this session".to_owned(),
            "Run it now and stop asking for matching operations in this Codex session.".to_owned(),
        ),
        Some("decline") => (
            "Decline".to_owned(),
            "Do not run it; let Codex continue.".to_owned(),
        ),
        Some("cancel") => (
            "Decline and stop".to_owned(),
            "Do not run it and interrupt the turn.".to_owned(),
        ),
        Some(_) => return None,
        None if object_has_only_known_keys(value, &["acceptWithExecpolicyAmendment"]) => {
            let amendment = value.get("acceptWithExecpolicyAmendment")?;
            if !object_has_exact_keys(amendment, &["execpolicy_amendment"]) {
                return None;
            }
            let rule = amendment
                .get("execpolicy_amendment")?
                .as_array()?
                .iter()
                .map(Value::as_str)
                .collect::<Option<Vec<_>>>()?
                .join(" ");
            if rule.is_empty() {
                return None;
            }
            (
                "Approve and remember this command rule".to_owned(),
                format!("Run it now and apply the command policy rule: {rule}"),
            )
        }
        None if object_has_only_known_keys(value, &["applyNetworkPolicyAmendment"]) => {
            let amendment = value.get("applyNetworkPolicyAmendment")?;
            if !object_has_exact_keys(amendment, &["network_policy_amendment"]) {
                return None;
            }
            let amendment = amendment.get("network_policy_amendment")?;
            if !object_has_exact_keys(amendment, &["action", "host"]) {
                return None;
            }
            let action = amendment.get("action")?.as_str()?;
            if !matches!(action, "allow" | "deny") {
                return None;
            }
            let host = amendment.get("host")?.as_str()?;
            (
                format!("Apply network rule: {action} {host}"),
                "Apply Codex's proposed persistent network policy rule.".to_owned(),
            )
        }
        None => return None,
    };
    Some(DecisionOption {
        label,
        description: Some(bounded_string(&description, MAX_PRESENTATION_STRING_BYTES).0),
    })
}

fn project_additional_permissions(params: &Value) -> Option<Vec<DecisionPermission>> {
    let Some(permissions) = params
        .get("additionalPermissions")
        .filter(|value| !value.is_null())
    else {
        return Some(Vec::new());
    };
    if !object_has_only_known_keys(permissions, &["fileSystem", "network"]) {
        return None;
    }
    let mut projected = Vec::new();
    if let Some(file_system) = permissions
        .get("fileSystem")
        .filter(|value| !value.is_null())
    {
        if !object_has_only_known_keys(
            file_system,
            &["entries", "globScanMaxDepth", "read", "write"],
        ) {
            return None;
        }
        for (field, access) in [("read", "read"), ("write", "write")] {
            if let Some(paths) = file_system.get(field).filter(|value| !value.is_null()) {
                let paths = paths.as_array()?;
                let permissions = paths
                    .iter()
                    .map(|path| {
                        Some(DecisionPermission {
                            access: access.to_owned(),
                            target: bounded_string(path.as_str()?, MAX_PRESENTATION_STRING_BYTES).0,
                        })
                    })
                    .collect::<Option<Vec<_>>>()?;
                projected.extend(permissions);
            }
        }
        if let Some(entries) = file_system.get("entries").filter(|value| !value.is_null()) {
            let permissions = entries
                .as_array()?
                .iter()
                .map(|entry| {
                    if !object_has_exact_keys(entry, &["access", "path"]) {
                        return None;
                    }
                    let access = entry.get("access")?.as_str()?;
                    if !matches!(access, "read" | "write" | "deny") {
                        return None;
                    }
                    Some(DecisionPermission {
                        access: access.to_owned(),
                        target: describe_file_system_target(entry.get("path")?)?,
                    })
                })
                .collect::<Option<Vec<_>>>()?;
            projected.extend(permissions);
        }
    }
    if let Some(network) = permissions.get("network").filter(|value| !value.is_null()) {
        if !object_has_only_known_keys(network, &["enabled"]) {
            return None;
        }
        if let Some(enabled) = network.get("enabled").filter(|value| !value.is_null()) {
            projected.push(DecisionPermission {
                access: "network".to_owned(),
                target: if enabled.as_bool()? {
                    "enabled"
                } else {
                    "disabled"
                }
                .to_owned(),
            });
        }
    }
    Some(projected)
}

fn describe_file_system_target(value: &Value) -> Option<String> {
    let kind = value.get("type")?.as_str()?;
    let target = match kind {
        "path" if object_has_exact_keys(value, &["path", "type"]) => {
            value.get("path")?.as_str()?.to_owned()
        }
        "glob_pattern" if object_has_exact_keys(value, &["pattern", "type"]) => {
            format!("glob:{}", value.get("pattern")?.as_str()?)
        }
        "special" => {
            if !object_has_exact_keys(value, &["type", "value"]) {
                return None;
            }
            let special = value.get("value")?;
            let name = special.get("kind")?.as_str()?;
            match name {
                "root" | "minimal" | "tmpdir" | "slash_tmp"
                    if object_has_exact_keys(special, &["kind"]) =>
                {
                    name.to_owned()
                }
                "project_roots" if object_has_only_known_keys(special, &["kind", "subpath"]) => {
                    match special.get("subpath").filter(|value| !value.is_null()) {
                        Some(subpath) => format!("{name}:{}", subpath.as_str()?),
                        None => name.to_owned(),
                    }
                }
                "unknown"
                    if object_has_only_known_keys(special, &["kind", "path", "subpath"])
                        && special.get("path").is_some() =>
                {
                    let path = special.get("path")?.as_str()?;
                    match special.get("subpath").filter(|value| !value.is_null()) {
                        Some(subpath) => format!("{name}:{path}:{}", subpath.as_str()?),
                        None => format!("{name}:{path}"),
                    }
                }
                _ => return None,
            }
        }
        _ => return None,
    };
    Some(bounded_string(&target, MAX_PRESENTATION_STRING_BYTES).0)
}

fn decision_preview_key(params: &Value) -> Option<(String, String)> {
    Some((
        params.get("threadId")?.as_str()?.to_owned(),
        params.get("itemId")?.as_str()?.to_owned(),
    ))
}

fn project_file_changes(changes: &[Value]) -> Option<Vec<DecisionFileChange>> {
    let mut remaining = MAX_FILE_DIFF_BYTES;
    changes
        .iter()
        .map(|change| {
            if !object_has_exact_keys(change, &["diff", "kind", "path"]) {
                return None;
            }
            let path = bounded_required_string(change.get("path")?, MAX_PRESENTATION_STRING_BYTES)?;
            let kind = describe_patch_change_kind(change.get("kind")?)?;
            let raw_diff = change.get("diff")?.as_str()?;
            let (diff, diff_truncated) = bounded_string(raw_diff, remaining);
            remaining = remaining.saturating_sub(diff.len());
            Some(DecisionFileChange {
                path: PathBuf::from(path),
                kind,
                diff,
                diff_truncated,
            })
        })
        .collect()
}

fn describe_patch_change_kind(value: &Value) -> Option<String> {
    let kind = value.get("type")?.as_str()?;
    match kind {
        "add" | "delete" if object_has_exact_keys(value, &["type"]) => Some(kind.to_owned()),
        "update" if object_has_only_known_keys(value, &["move_path", "type"]) => {
            match value.get("move_path").filter(|value| !value.is_null()) {
                Some(path) => Some(format!(
                    "move to {}",
                    bounded_required_string(path, MAX_PRESENTATION_STRING_BYTES)?
                )),
                None => Some("update".to_owned()),
            }
        }
        _ => None,
    }
}

fn object_has_exact_keys(value: &Value, expected: &[&str]) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == expected.len() && expected.iter().all(|key| object.contains_key(*key))
    })
}

fn object_has_only_known_keys(value: &Value, known: &[&str]) -> bool {
    value
        .as_object()
        .is_some_and(|object| object.keys().all(|key| known.contains(&key.as_str())))
}

fn bounded_required_string(value: &Value, limit: usize) -> Option<String> {
    let value = value.as_str()?;
    if value.trim().is_empty() {
        None
    } else {
        Some(bounded_string(value, limit).0)
    }
}

fn bounded_optional_string(value: Option<&Value>, limit: usize) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(|value| bounded_string(value, limit).0)
}

fn bounded_string(value: &str, limit: usize) -> (String, bool) {
    let normalized = value.replace("\r\n", "\n");
    let safe = normalized
        .chars()
        .map(|character| {
            if character == '\n' || character == '\t' || !character.is_control() {
                character
            } else {
                '\u{fffd}'
            }
        })
        .collect::<String>();
    if safe.len() <= limit {
        return (safe, false);
    }
    let mut end = limit;
    while !safe.is_char_boundary(end) {
        end -= 1;
    }
    (safe[..end].to_owned(), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_schema_shaped_file_changes_and_move_targets() {
        let changes = json!([
            {"path": "new.txt", "kind": {"type": "add"}, "diff": "+new"},
            {
                "path": "old.txt",
                "kind": {"type": "update", "move_path": "moved.txt"},
                "diff": "rename"
            }
        ]);
        let projected = project_file_changes(changes.as_array().unwrap()).unwrap();
        assert_eq!(projected[0].kind, "add");
        assert_eq!(projected[1].kind, "move to moved.txt");

        let hidden_field = json!([{
            "path": "new.txt",
            "kind": {"type": "add"},
            "diff": "+new",
            "futureMeaning": "not safe to hide"
        }]);
        assert!(project_file_changes(hidden_field.as_array().unwrap()).is_none());
    }

    #[test]
    fn projects_only_fully_understood_structured_approval_choices() {
        let choice = json!({
            "acceptWithExecpolicyAmendment": {
                "execpolicy_amendment": ["git", "status"]
            }
        });
        let projected = describe_native_decision(&choice).unwrap();
        assert!(projected.description.unwrap().contains("git status"));

        let extended = json!({
            "acceptWithExecpolicyAmendment": {
                "execpolicy_amendment": ["git", "status"],
                "futureEffect": true
            }
        });
        assert!(describe_native_decision(&extended).is_none());
    }

    #[test]
    fn rejects_additional_permissions_that_cannot_be_fully_presented() {
        let params = json!({
            "additionalPermissions": {
                "fileSystem": {
                    "entries": [{
                        "access": "write",
                        "path": {"type": "glob_pattern", "pattern": "/cache/**"}
                    }]
                },
                "network": {"enabled": true}
            }
        });
        assert_eq!(
            project_additional_permissions(&params).unwrap(),
            [
                DecisionPermission {
                    access: "write".to_owned(),
                    target: "glob:/cache/**".to_owned(),
                },
                DecisionPermission {
                    access: "network".to_owned(),
                    target: "enabled".to_owned(),
                }
            ]
        );

        let unknown = json!({
            "additionalPermissions": {"futurePermission": {"enabled": true}}
        });
        assert!(project_additional_permissions(&unknown).is_none());
    }

    #[test]
    fn neutralizes_terminal_control_sequences_in_presented_values() {
        let (safe, truncated) = bounded_string("safe\r\n\u{1b}[31mred\rtext", 1024);
        assert_eq!(safe, "safe\n�[31mred�text");
        assert!(!truncated);
        assert!(!safe.contains('\u{1b}'));
        assert!(!safe.contains('\r'));
    }
}
