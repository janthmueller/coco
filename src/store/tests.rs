use serde_json::json;

use super::*;
use crate::domain::{
    CodexThreadStatus, DecisionApprovalPrompt, DecisionKind, DecisionOption, DecisionPrompt,
    DecisionState, Turn, Workspace, WorkspaceLifecycle, WorkspacePhase, WorkspaceWaitReason,
};

fn repository(root: &Path) -> Repository {
    Repository {
        id: "repo-test".to_owned(),
        root_path: root.to_owned(),
        git_common_dir: root.join(".git"),
        display_name: "fixture".to_owned(),
        is_linked_worktree: false,
        created_at_ms: 1,
        updated_at_ms: 1,
    }
}

fn new_workspace(repository_id: &str, name: &str) -> NewWorkspace {
    NewWorkspace {
        create_operation_id: Some(format!("create-{name}")),
        repository_id: repository_id.to_owned(),
        name: name.to_owned(),
        context_mode: ContextMode::Fresh,
        context: json!({"version": 1, "mode": "fresh"}),
        profile: ProfileSnapshot {
            name: "default".to_owned(),
            source_path: None,
            source_hash: "sha256:test".to_owned(),
            effective_settings: json!({"network_access": false}),
        },
        branch_name: Some(format!("coco/{name}")),
        base_sha: Some("0123456789abcdef".to_owned()),
        worktree_path: Some(PathBuf::from(format!("/tmp/worktrees/{name}"))),
    }
}

fn ready_workspace(store: &Store, repository_id: &str, name: &str) -> Workspace {
    let (workspace, _) = store
        .create_workspace_with_event(
            new_workspace(repository_id, name),
            EventDraft::workspace(EventKind::WorkspaceCreated, EventSource::Coco, json!({})),
        )
        .unwrap();
    let (workspace, _) = store
        .transition_workspace_lifecycle_with_event(
            &workspace.id,
            WorkspaceLifecycle::Provisioning,
            WorkspaceLifecycle::Starting,
            None,
            EventDraft::workspace(EventKind::WorktreeCreated, EventSource::Git, json!({})),
        )
        .unwrap();
    store
        .bind_thread_with_event(
            &workspace.id,
            WorkspaceLifecycle::Starting,
            WorkspaceLifecycle::Ready,
            NewThreadBinding {
                thread_id: format!("thread-{name}"),
                parent_thread_id: None,
                status: CodexThreadStatus::Idle,
                runtime_generation: "runtime-test".to_owned(),
            },
            EventDraft::workspace(EventKind::AgentStarted, EventSource::Codex, json!({})),
        )
        .unwrap()
        .0
}

fn pending_command_decision(workspace: &Workspace, turn: &Turn) -> NewDecision {
    NewDecision {
        workspace_id: workspace.id.clone(),
        turn_id: Some(turn.id.clone()),
        codex_thread_id: workspace.codex_thread_id.clone().unwrap(),
        codex_turn_id: turn.codex_turn_id.clone(),
        runtime_generation: "runtime-test".to_owned(),
        native_request_id: json!(17),
        method: "item/commandExecution/requestApproval".to_owned(),
        kind: DecisionKind::CommandApproval,
        prompt: DecisionPrompt::Approval(Box::new(DecisionApprovalPrompt {
            title: "Run a command".to_owned(),
            reason: Some("test".to_owned()),
            command: Some("git status".to_owned()),
            cwd: Some(PathBuf::from("/tmp/worktree")),
            network_host: None,
            network_protocol: None,
            grant_root: None,
            additional_permissions: Vec::new(),
            changes: Vec::new(),
            options: vec![DecisionOption {
                label: "Approve once".to_owned(),
                description: None,
            }],
        })),
        native_options: vec![json!("accept")],
    }
}

fn legacy_v1_connection() -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            r#"PRAGMA foreign_keys = ON;
             CREATE TABLE repositories (
                id TEXT PRIMARY KEY,
                root_path TEXT NOT NULL UNIQUE,
                git_common_dir TEXT NOT NULL UNIQUE,
                display_name TEXT NOT NULL,
                is_linked_worktree INTEGER NOT NULL,
                created_at_ms INTEGER NOT NULL,
                updated_at_ms INTEGER NOT NULL
             );
             CREATE TABLE tasks (
                id TEXT PRIMARY KEY,
                create_operation_id TEXT UNIQUE,
                repository_id TEXT NOT NULL REFERENCES repositories(id),
                name TEXT NOT NULL,
                goal TEXT NOT NULL CHECK (length(trim(goal)) > 0),
                context_mode TEXT NOT NULL,
                context_json TEXT NOT NULL,
                profile_json TEXT NOT NULL,
                phase TEXT NOT NULL,
                branch_name TEXT,
                base_sha TEXT,
                worktree_path TEXT UNIQUE,
                codex_thread_id TEXT UNIQUE,
                parent_thread_id TEXT,
                active_turn_id TEXT,
                last_error_code TEXT,
                last_error_message TEXT,
                created_at_ms INTEGER NOT NULL,
                updated_at_ms INTEGER NOT NULL,
                completed_at_ms INTEGER,
                UNIQUE(repository_id, name),
                UNIQUE(repository_id, branch_name)
             );
             CREATE TABLE turns (
                id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL REFERENCES tasks(id),
                requested_at_ms INTEGER NOT NULL
             );
             CREATE INDEX turns_task_idx ON turns(task_id, requested_at_ms);
             CREATE TABLE events (
                sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                task_id TEXT REFERENCES tasks(id),
                kind TEXT NOT NULL
             );
             CREATE INDEX events_task_sequence_idx ON events(task_id, sequence);
             CREATE TABLE audit_events (
                sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                task_id TEXT REFERENCES tasks(id),
                action TEXT NOT NULL
             );
             CREATE INDEX audit_task_sequence_idx ON audit_events(task_id, sequence);
             CREATE TABLE child_reference (
                id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL REFERENCES tasks(id)
             );
             INSERT INTO repositories VALUES (
                'repo-v1', '/tmp/source', '/tmp/source/.git', 'source', 0, 1, 1
             );
             INSERT INTO tasks (
                id, repository_id, name, goal, context_mode, context_json,
                profile_json, phase, codex_thread_id, created_at_ms, updated_at_ms
             ) VALUES (
                'workspace-v1', 'repo-v1', 'legacy', 'legacy goal', 'fresh', '{}',
                '{"name":"default","sourcePath":null,"sourceHash":"sha256:test","effectiveSettings":{}}',
                'waiting_for_input', 'thread-v1', 1, 1
             );
             INSERT INTO child_reference VALUES ('child-v1', 'workspace-v1');
             INSERT INTO turns VALUES ('turn-v1', 'workspace-v1', 1);
             INSERT INTO events (task_id, kind) VALUES ('workspace-v1', 'task.created');
             INSERT INTO audit_events (task_id, action) VALUES ('workspace-v1', 'agents.send');
             PRAGMA user_version = 1;"#,
        )
        .unwrap();
    connection
}

#[test]
fn migrates_v1_tasks_to_workspaces_without_losing_data() {
    let store = Store::from_connection(legacy_v1_connection()).unwrap();
    let workspace = store.workspace_by_id("workspace-v1").unwrap().unwrap();
    assert_eq!(workspace.name, "legacy");
    assert!(
        serde_json::to_value(&workspace)
            .unwrap()
            .get("goal")
            .is_none()
    );
    let connection = store.lock().unwrap();
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 5);
    assert_eq!(workspace.lifecycle, WorkspaceLifecycle::Ready);
    assert_eq!(workspace.phase, WorkspacePhase::Unavailable);
    assert_eq!(
        workspace
            .thread_runtime
            .as_ref()
            .map(|snapshot| &snapshot.status),
        Some(&CodexThreadStatus::Active {
            active_flags: vec!["waitingOnUserInput".to_owned()]
        })
    );
    assert!(!workspace.thread_runtime.unwrap().is_fresh);
    let legacy_goal: Option<String> = connection
        .query_row(
            "SELECT legacy_goal FROM workspaces WHERE id = 'workspace-v1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(legacy_goal.as_deref(), Some("legacy goal"));
    let migrated_turn_workspace: String = connection
        .query_row(
            "SELECT workspace_id FROM turns WHERE id = 'turn-v1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(migrated_turn_workspace, "workspace-v1");
    let migrated_kind: String = connection
        .query_row("SELECT kind FROM events", [], |row| row.get(0))
        .unwrap();
    assert_eq!(migrated_kind, "workspace.created");
    let migrated_action: String = connection
        .query_row("SELECT action FROM audit_events", [], |row| row.get(0))
        .unwrap();
    assert_eq!(migrated_action, "workspaces.send");
    let child_parent: String = connection
        .query_row(
            "SELECT \"table\" FROM pragma_foreign_key_list('child_reference')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(child_parent, "workspaces");
    let retired_table_count: i64 = connection
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'tasks'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(retired_table_count, 0);
    for index in [
        "turns_workspace_idx",
        "events_workspace_sequence_idx",
        "audit_workspace_sequence_idx",
    ] {
        let count: i64 = connection
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
                [index],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "missing migrated index {index}");
    }
    connection
        .execute(
            "UPDATE workspaces SET legacy_goal = NULL WHERE id = 'workspace-v1'",
            [],
        )
        .unwrap();
    let violations: i64 = connection
        .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(violations, 0);
}

#[test]
fn state_and_events_change_atomically() {
    let store = Store::in_memory().unwrap();
    let repo = repository(Path::new("/tmp/source"));
    store.register_repository(&repo).unwrap();
    let (workspace, created) = store
        .create_workspace_with_event(
            new_workspace(&repo.id, "atomic"),
            EventDraft::workspace(
                EventKind::WorkspaceCreated,
                EventSource::Coco,
                json!({"version": 1}),
            ),
        )
        .unwrap();
    assert_eq!(workspace.phase, WorkspacePhase::Provisioning);
    assert_eq!(created.workspace_id.as_deref(), Some(workspace.id.as_str()));

    let (workspace, _) = store
        .transition_workspace_lifecycle_with_event(
            &workspace.id,
            WorkspaceLifecycle::Provisioning,
            WorkspaceLifecycle::Starting,
            None,
            EventDraft::workspace(EventKind::WorktreeCreated, EventSource::Git, json!({})),
        )
        .unwrap();
    let effective_profile = ProfileSnapshot {
        name: "effective".to_owned(),
        source_path: Some(PathBuf::from("/tmp/profile.toml")),
        source_hash: "sha256:effective".to_owned(),
        effective_settings: json!({"approvalPolicy": "on-request"}),
    };
    let workspace = store
        .update_workspace_profile(&workspace.id, &effective_profile)
        .unwrap();
    assert_eq!(workspace.profile, effective_profile);

    let (workspace, _) = store
        .bind_thread_with_event(
            &workspace.id,
            WorkspaceLifecycle::Starting,
            WorkspaceLifecycle::Ready,
            NewThreadBinding {
                thread_id: "thread-1".to_owned(),
                parent_thread_id: None,
                status: CodexThreadStatus::Idle,
                runtime_generation: "runtime-1".to_owned(),
            },
            EventDraft::workspace(EventKind::AgentStarted, EventSource::Codex, json!({})),
        )
        .unwrap();
    assert_eq!(workspace.codex_thread_id.as_deref(), Some("thread-1"));
    assert_eq!(workspace.lifecycle, WorkspaceLifecycle::Ready);
    assert_eq!(workspace.phase, WorkspacePhase::Idle);
    assert_eq!(
        store
            .workspace_by_thread_id("thread-1")
            .unwrap()
            .unwrap()
            .id,
        workspace.id
    );

    let (workspace, turn, _) = store
        .start_turn_with_event(
            &workspace.id,
            NewTurn {
                operation_id: Some("turn-operation".to_owned()),
                client_message_id: "client-message".to_owned(),
                codex_turn_id: Some("codex-turn".to_owned()),
                started_at_ms: Some(20),
            },
            EventDraft::workspace(EventKind::TurnStarted, EventSource::Codex, json!({})),
        )
        .unwrap();
    assert_eq!(workspace.phase, WorkspacePhase::Active);
    assert_eq!(workspace.active_turn_id.as_deref(), Some(turn.id.as_str()));
    assert_eq!(
        store
            .turn_by_operation_id("turn-operation")
            .unwrap()
            .unwrap()
            .id,
        turn.id
    );

    let (workspace, turn, _) = store
        .complete_turn_with_event(
            &workspace.id,
            &turn.id,
            TurnCompletion {
                phase: TurnPhase::Completed,
                error: None,
                completed_at_ms: Some(30),
            },
            EventDraft::workspace(EventKind::TurnCompleted, EventSource::Codex, json!({})),
        )
        .unwrap();
    assert_eq!(workspace.phase, WorkspacePhase::Idle);
    assert_eq!(workspace.active_turn_id, None);
    assert_eq!(turn.phase, TurnPhase::Completed);
    assert_eq!(turn.completed_at_ms, Some(30));
    assert_eq!(store.events_after(Some(&workspace.id), 0).unwrap().len(), 5);
}

#[test]
fn decisions_are_generation_bound_and_transition_atomically_with_events() {
    let store = Store::in_memory().unwrap();
    let repo = repository(Path::new("/tmp/source-decisions"));
    store.register_repository(&repo).unwrap();
    let workspace = ready_workspace(&store, &repo.id, "decisions");
    let (_, turn, _) = store
        .start_turn_with_event(
            &workspace.id,
            NewTurn {
                operation_id: Some("decision-turn".to_owned()),
                client_message_id: "decision-message".to_owned(),
                codex_turn_id: Some("codex-decision-turn".to_owned()),
                started_at_ms: Some(10),
            },
            EventDraft::workspace(EventKind::TurnStarted, EventSource::Codex, json!({})),
        )
        .unwrap();
    let (stored, event) = store
        .create_decision_with_event(
            pending_command_decision(&workspace, &turn),
            EventDraft::workspace(
                EventKind::DecisionRequested,
                EventSource::Codex,
                json!({"kind": "command_approval"}),
            ),
        )
        .unwrap();
    assert_eq!(stored.decision.state, DecisionState::Pending);
    assert_eq!(stored.native_request_id, json!(17));
    assert_eq!(stored.native_options, [json!("accept")]);
    assert_eq!(event.payload["decisionId"], stored.decision.id);
    assert_eq!(
        store
            .open_decisions_for_workspace(&workspace.id)
            .unwrap()
            .len(),
        1
    );

    assert!(matches!(
        store.mark_decision_submitted(&stored.decision.id, "another-runtime", &json!({})),
        Err(StoreError::DecisionGenerationMismatch { .. })
    ));
    let submitted = store
        .mark_decision_submitted(
            &stored.decision.id,
            "runtime-test",
            &json!({"choice": 1, "label": "Approve once"}),
        )
        .unwrap();
    assert_eq!(submitted.decision.state, DecisionState::Submitted);
    assert!(matches!(
        store.mark_decision_submitted(&stored.decision.id, "runtime-test", &json!({})),
        Err(StoreError::InvalidDecisionState { .. })
    ));

    let resolved = store
        .resolve_decision_by_native_request("runtime-test", "thread-decisions", &json!(17))
        .unwrap()
        .unwrap();
    assert_eq!(resolved.decision.state, DecisionState::Resolved);
    assert!(
        store
            .open_decisions_for_workspace(&workspace.id)
            .unwrap()
            .is_empty()
    );
    let events = store.events_after(Some(&workspace.id), 0).unwrap();
    assert_eq!(events[events.len() - 2].kind, EventKind::DecisionRequested);
    assert_eq!(events.last().unwrap().kind, EventKind::DecisionResolved);
}

#[test]
fn open_decisions_are_orphaned_without_replaying_native_requests() {
    let store = Store::in_memory().unwrap();
    let repo = repository(Path::new("/tmp/source-orphaned-decision"));
    store.register_repository(&repo).unwrap();
    let workspace = ready_workspace(&store, &repo.id, "orphaned-decision");
    let (_, turn, _) = store
        .start_turn_with_event(
            &workspace.id,
            NewTurn {
                operation_id: Some("orphan-turn".to_owned()),
                client_message_id: "orphan-message".to_owned(),
                codex_turn_id: Some("codex-orphan-turn".to_owned()),
                started_at_ms: None,
            },
            EventDraft::workspace(EventKind::TurnStarted, EventSource::Codex, json!({})),
        )
        .unwrap();
    let (stored, _) = store
        .create_decision_with_event(
            pending_command_decision(&workspace, &turn),
            EventDraft::workspace(EventKind::DecisionRequested, EventSource::Codex, json!({})),
        )
        .unwrap();

    assert_eq!(
        store
            .orphan_open_decisions(Some("runtime-test"), "restart")
            .unwrap(),
        1
    );
    let orphaned = store.decision_by_id(&stored.decision.id).unwrap().unwrap();
    assert_eq!(orphaned.decision.state, DecisionState::Orphaned);
    assert!(
        store
            .open_decisions_for_workspace(&workspace.id)
            .unwrap()
            .is_empty()
    );
    assert_eq!(store.orphan_open_decisions(None, "again").unwrap(), 0);
}

#[test]
fn a_failed_turn_does_not_fail_the_workspace_lifecycle() {
    let store = Store::in_memory().unwrap();
    let repo = repository(Path::new("/tmp/source-failed-turn"));
    store.register_repository(&repo).unwrap();
    let workspace = ready_workspace(&store, &repo.id, "failed-turn");
    let (workspace, turn, _) = store
        .start_turn_with_event(
            &workspace.id,
            NewTurn {
                operation_id: Some("failed-turn-operation".to_owned()),
                client_message_id: "failed-client-message".to_owned(),
                codex_turn_id: Some("failed-codex-turn".to_owned()),
                started_at_ms: Some(40),
            },
            EventDraft::workspace(EventKind::TurnStarted, EventSource::Codex, json!({})),
        )
        .unwrap();
    let (workspace, turn, _) = store
        .complete_turn_with_event(
            &workspace.id,
            &turn.id,
            TurnCompletion {
                phase: TurnPhase::Failed,
                error: Some(json!({"code": "MODEL_ERROR", "message": "try again"})),
                completed_at_ms: Some(50),
            },
            EventDraft::workspace(EventKind::TurnCompleted, EventSource::Codex, json!({})),
        )
        .unwrap();

    assert_eq!(workspace.lifecycle, WorkspaceLifecycle::Ready);
    assert_eq!(workspace.phase, WorkspacePhase::Idle);
    assert_eq!(workspace.last_error_code.as_deref(), Some("MODEL_ERROR"));
    assert_eq!(turn.phase, TurnPhase::Failed);
}

#[test]
fn failed_compare_and_set_does_not_append_an_event() {
    let store = Store::in_memory().unwrap();
    let repo = repository(Path::new("/tmp/source-cas"));
    store.register_repository(&repo).unwrap();
    let (workspace, _) = store
        .create_workspace_with_event(
            new_workspace(&repo.id, "cas"),
            EventDraft::workspace(EventKind::WorkspaceCreated, EventSource::Coco, json!({})),
        )
        .unwrap();

    assert!(matches!(
        store.transition_workspace_lifecycle_with_event(
            &workspace.id,
            WorkspaceLifecycle::Ready,
            WorkspaceLifecycle::Completed,
            None,
            EventDraft::workspace(EventKind::TurnStarted, EventSource::Coco, json!({})),
        ),
        Err(StoreError::InvalidWorkspaceTransition { .. })
    ));
    assert_eq!(store.events_after(Some(&workspace.id), 0).unwrap().len(), 1);
    assert_eq!(
        store.workspace_by_id(&workspace.id).unwrap().unwrap().phase,
        WorkspacePhase::Provisioning
    );
}

#[test]
fn native_thread_status_is_persisted_losslessly_and_can_be_staled() {
    let store = Store::in_memory().unwrap();
    let repo = repository(Path::new("/tmp/source-runtime"));
    store.register_repository(&repo).unwrap();
    let (workspace, _) = store
        .create_workspace_with_event(
            new_workspace(&repo.id, "runtime"),
            EventDraft::workspace(EventKind::WorkspaceCreated, EventSource::Coco, json!({})),
        )
        .unwrap();
    let (workspace, _) = store
        .transition_workspace_lifecycle_with_event(
            &workspace.id,
            WorkspaceLifecycle::Provisioning,
            WorkspaceLifecycle::Starting,
            None,
            EventDraft::workspace(EventKind::WorktreeCreated, EventSource::Git, json!({})),
        )
        .unwrap();
    let (workspace, _) = store
        .bind_thread_with_event(
            &workspace.id,
            WorkspaceLifecycle::Starting,
            WorkspaceLifecycle::Ready,
            NewThreadBinding {
                thread_id: "thread-runtime".to_owned(),
                parent_thread_id: None,
                status: CodexThreadStatus::Idle,
                runtime_generation: "runtime-1".to_owned(),
            },
            EventDraft::workspace(EventKind::AgentStarted, EventSource::Codex, json!({})),
        )
        .unwrap();
    let (workspace, event) = store
        .observe_thread_status_with_event(
            &workspace.id,
            CodexThreadStatus::Active {
                active_flags: vec![
                    "waitingOnUserInput".to_owned(),
                    "futureFlag".to_owned(),
                    "waitingOnApproval".to_owned(),
                    "futureFlag".to_owned(),
                ],
            },
            "runtime-2",
            EventDraft::workspace(
                EventKind::ThreadStatusChanged,
                EventSource::Codex,
                json!({}),
            ),
        )
        .unwrap();

    assert_eq!(event.kind, EventKind::ThreadStatusChanged);
    assert_eq!(workspace.lifecycle, WorkspaceLifecycle::Ready);
    assert_eq!(workspace.phase, WorkspacePhase::WaitingForApproval);
    assert_eq!(
        workspace.wait_reasons,
        [
            WorkspaceWaitReason::Approval,
            WorkspaceWaitReason::UserInput
        ]
    );
    let snapshot = workspace.thread_runtime.unwrap();
    assert_eq!(snapshot.runtime_generation, "runtime-2");
    assert_eq!(
        snapshot.status,
        CodexThreadStatus::Active {
            active_flags: vec![
                "futureFlag".to_owned(),
                "waitingOnApproval".to_owned(),
                "waitingOnUserInput".to_owned(),
            ]
        }
    );

    assert_eq!(store.mark_thread_statuses_stale().unwrap(), 1);
    let workspace = store.workspace_by_id(&workspace.id).unwrap().unwrap();
    assert_eq!(workspace.lifecycle, WorkspaceLifecycle::Ready);
    assert_eq!(workspace.phase, WorkspacePhase::Unavailable);
    assert!(!workspace.thread_runtime.unwrap().is_fresh);
}

#[test]
fn audit_round_trips_sanitized_metadata() {
    let store = Store::in_memory().unwrap();
    let audit = store
        .append_audit(AuditDraft {
            source: "mcp:test".to_owned(),
            action: "workspaces.list".to_owned(),
            workspace_id: None,
            operation_id: None,
            outcome: AuditOutcome::Succeeded,
            details: json!({"count": 2}),
            occurred_at_ms: Some(42),
        })
        .unwrap();
    assert_eq!(audit.details, json!({"count": 2}));
    assert_eq!(store.audits_after(None, 0).unwrap(), [audit]);
}

#[test]
fn restart_reconciliation_marks_inflight_state_interrupted() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("coco.sqlite3");
    let workspace_id;
    let unfinished_creation_id;
    let turn_id;
    {
        let store = Store::open(&path).unwrap();
        let repo = repository(&temp.path().join("source"));
        store.register_repository(&repo).unwrap();
        let (unfinished_creation, _) = store
            .create_workspace_with_event(
                new_workspace(&repo.id, "unfinished-creation"),
                EventDraft::workspace(EventKind::WorkspaceCreated, EventSource::Coco, json!({})),
            )
            .unwrap();
        unfinished_creation_id = unfinished_creation.id;
        let (workspace, _) = store
            .create_workspace_with_event(
                new_workspace(&repo.id, "restart"),
                EventDraft::workspace(EventKind::WorkspaceCreated, EventSource::Coco, json!({})),
            )
            .unwrap();
        let (workspace, _) = store
            .transition_workspace_lifecycle_with_event(
                &workspace.id,
                WorkspaceLifecycle::Provisioning,
                WorkspaceLifecycle::Starting,
                None,
                EventDraft::workspace(EventKind::WorktreeCreated, EventSource::Git, json!({})),
            )
            .unwrap();
        let (workspace, _) = store
            .bind_thread_with_event(
                &workspace.id,
                WorkspaceLifecycle::Starting,
                WorkspaceLifecycle::Ready,
                NewThreadBinding {
                    thread_id: "thread-restart".to_owned(),
                    parent_thread_id: None,
                    status: CodexThreadStatus::Idle,
                    runtime_generation: "runtime-before-restart".to_owned(),
                },
                EventDraft::workspace(EventKind::AgentStarted, EventSource::Codex, json!({})),
            )
            .unwrap();
        let (_, turn, _) = store
            .start_turn_with_event(
                &workspace.id,
                NewTurn {
                    operation_id: Some("restart-operation".to_owned()),
                    client_message_id: "restart-message".to_owned(),
                    codex_turn_id: Some("codex-restart-turn".to_owned()),
                    started_at_ms: None,
                },
                EventDraft::workspace(EventKind::TurnStarted, EventSource::Codex, json!({})),
            )
            .unwrap();
        workspace_id = workspace.id;
        turn_id = turn.id;
    }

    let store = Store::open(&path).unwrap();
    let reconciled = store.reconcile_unfinished().unwrap();
    assert_eq!(reconciled.len(), 2);
    let unfinished_creation = store
        .workspace_by_id(&unfinished_creation_id)
        .unwrap()
        .unwrap();
    assert_eq!(unfinished_creation.lifecycle, WorkspaceLifecycle::Failed);
    assert_eq!(unfinished_creation.phase, WorkspacePhase::Failed);
    assert_eq!(
        store
            .workspace_by_id(&workspace_id)
            .unwrap()
            .unwrap()
            .lifecycle,
        WorkspaceLifecycle::Ready
    );
    let workspace = store.workspace_by_id(&workspace_id).unwrap().unwrap();
    assert_eq!(workspace.phase, WorkspacePhase::Unavailable);
    assert!(!workspace.thread_runtime.unwrap().is_fresh);
    assert_eq!(
        store.turn_by_id(&turn_id).unwrap().unwrap().phase,
        TurnPhase::Interrupted
    );

    #[cfg(unix)]
    {
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let parent_mode = fs::metadata(temp.path()).unwrap().permissions().mode() & 0o777;
        assert_eq!(parent_mode, 0o700);
    }
}
