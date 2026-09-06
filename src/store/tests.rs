use serde_json::json;

use super::*;
use crate::domain::{CodexThreadStatus, Task, TaskLifecycle, TaskPhase, TaskWaitReason};

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

fn new_task(repository_id: &str, name: &str) -> NewTask {
    NewTask {
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

fn ready_task(store: &Store, repository_id: &str, name: &str) -> Task {
    let (task, _) = store
        .create_task_with_event(
            new_task(repository_id, name),
            EventDraft::task(EventKind::TaskCreated, EventSource::Coco, json!({})),
        )
        .unwrap();
    let (task, _) = store
        .transition_task_lifecycle_with_event(
            &task.id,
            TaskLifecycle::Provisioning,
            TaskLifecycle::Starting,
            None,
            EventDraft::task(EventKind::WorktreeCreated, EventSource::Git, json!({})),
        )
        .unwrap();
    store
        .bind_thread_with_event(
            &task.id,
            TaskLifecycle::Starting,
            NewThreadBinding {
                thread_id: format!("thread-{name}"),
                parent_thread_id: None,
                status: CodexThreadStatus::Idle,
                runtime_generation: "runtime-test".to_owned(),
            },
            EventDraft::task(EventKind::AgentStarted, EventSource::Codex, json!({})),
        )
        .unwrap()
        .0
}

#[test]
fn retires_v1_goal_from_task_projection_without_losing_legacy_data() {
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
                'task-v1', 'repo-v1', 'legacy', 'legacy goal', 'fresh', '{}',
                '{"name":"default","sourcePath":null,"sourceHash":"sha256:test","effectiveSettings":{}}',
                'waiting_for_input', 'thread-v1', 1, 1
             );
             INSERT INTO child_reference VALUES ('child-v1', 'task-v1');
             PRAGMA user_version = 1;"#,
        )
        .unwrap();

    let store = Store::from_connection(connection).unwrap();
    let task = store.task_by_id("task-v1").unwrap().unwrap();
    assert_eq!(task.name, "legacy");
    assert!(serde_json::to_value(&task).unwrap().get("goal").is_none());
    let connection = store.lock().unwrap();
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 3);
    assert_eq!(task.lifecycle, TaskLifecycle::Ready);
    assert_eq!(task.phase, TaskPhase::Unavailable);
    assert_eq!(
        task.thread_runtime
            .as_ref()
            .map(|snapshot| &snapshot.status),
        Some(&CodexThreadStatus::Active {
            active_flags: vec!["waitingOnUserInput".to_owned()]
        })
    );
    assert!(!task.thread_runtime.unwrap().is_fresh);
    let legacy_goal: Option<String> = connection
        .query_row(
            "SELECT legacy_goal FROM tasks WHERE id = 'task-v1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(legacy_goal.as_deref(), Some("legacy goal"));
    connection
        .execute(
            "UPDATE tasks SET legacy_goal = NULL WHERE id = 'task-v1'",
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
    let (task, created) = store
        .create_task_with_event(
            new_task(&repo.id, "atomic"),
            EventDraft::task(
                EventKind::TaskCreated,
                EventSource::Coco,
                json!({"version": 1}),
            ),
        )
        .unwrap();
    assert_eq!(task.phase, TaskPhase::Provisioning);
    assert_eq!(created.task_id.as_deref(), Some(task.id.as_str()));

    let (task, _) = store
        .transition_task_lifecycle_with_event(
            &task.id,
            TaskLifecycle::Provisioning,
            TaskLifecycle::Starting,
            None,
            EventDraft::task(EventKind::WorktreeCreated, EventSource::Git, json!({})),
        )
        .unwrap();
    let effective_profile = ProfileSnapshot {
        name: "effective".to_owned(),
        source_path: Some(PathBuf::from("/tmp/profile.toml")),
        source_hash: "sha256:effective".to_owned(),
        effective_settings: json!({"approvalPolicy": "on-request"}),
    };
    let task = store
        .update_task_profile(&task.id, &effective_profile)
        .unwrap();
    assert_eq!(task.profile, effective_profile);

    let (task, _) = store
        .bind_thread_with_event(
            &task.id,
            TaskLifecycle::Starting,
            NewThreadBinding {
                thread_id: "thread-1".to_owned(),
                parent_thread_id: None,
                status: CodexThreadStatus::Idle,
                runtime_generation: "runtime-1".to_owned(),
            },
            EventDraft::task(EventKind::AgentStarted, EventSource::Codex, json!({})),
        )
        .unwrap();
    assert_eq!(task.codex_thread_id.as_deref(), Some("thread-1"));
    assert_eq!(task.lifecycle, TaskLifecycle::Ready);
    assert_eq!(task.phase, TaskPhase::Idle);
    assert_eq!(
        store.task_by_thread_id("thread-1").unwrap().unwrap().id,
        task.id
    );

    let (task, turn, _) = store
        .start_turn_with_event(
            &task.id,
            NewTurn {
                operation_id: Some("turn-operation".to_owned()),
                client_message_id: "client-message".to_owned(),
                codex_turn_id: Some("codex-turn".to_owned()),
                started_at_ms: Some(20),
            },
            EventDraft::task(EventKind::TurnStarted, EventSource::Codex, json!({})),
        )
        .unwrap();
    assert_eq!(task.phase, TaskPhase::Active);
    assert_eq!(task.active_turn_id.as_deref(), Some(turn.id.as_str()));
    assert_eq!(
        store
            .turn_by_operation_id("turn-operation")
            .unwrap()
            .unwrap()
            .id,
        turn.id
    );

    let (task, turn, _) = store
        .complete_turn_with_event(
            &task.id,
            &turn.id,
            TurnCompletion {
                phase: TurnPhase::Completed,
                error: None,
                completed_at_ms: Some(30),
            },
            EventDraft::task(EventKind::TurnCompleted, EventSource::Codex, json!({})),
        )
        .unwrap();
    assert_eq!(task.phase, TaskPhase::Idle);
    assert_eq!(task.active_turn_id, None);
    assert_eq!(turn.phase, TurnPhase::Completed);
    assert_eq!(turn.completed_at_ms, Some(30));
    assert_eq!(store.events_after(Some(&task.id), 0).unwrap().len(), 5);
}

#[test]
fn a_failed_turn_does_not_fail_the_task_lifecycle() {
    let store = Store::in_memory().unwrap();
    let repo = repository(Path::new("/tmp/source-failed-turn"));
    store.register_repository(&repo).unwrap();
    let task = ready_task(&store, &repo.id, "failed-turn");
    let (task, turn, _) = store
        .start_turn_with_event(
            &task.id,
            NewTurn {
                operation_id: Some("failed-turn-operation".to_owned()),
                client_message_id: "failed-client-message".to_owned(),
                codex_turn_id: Some("failed-codex-turn".to_owned()),
                started_at_ms: Some(40),
            },
            EventDraft::task(EventKind::TurnStarted, EventSource::Codex, json!({})),
        )
        .unwrap();
    let (task, turn, _) = store
        .complete_turn_with_event(
            &task.id,
            &turn.id,
            TurnCompletion {
                phase: TurnPhase::Failed,
                error: Some(json!({"code": "MODEL_ERROR", "message": "try again"})),
                completed_at_ms: Some(50),
            },
            EventDraft::task(EventKind::TurnCompleted, EventSource::Codex, json!({})),
        )
        .unwrap();

    assert_eq!(task.lifecycle, TaskLifecycle::Ready);
    assert_eq!(task.phase, TaskPhase::Idle);
    assert_eq!(task.last_error_code.as_deref(), Some("MODEL_ERROR"));
    assert_eq!(turn.phase, TurnPhase::Failed);
}

#[test]
fn failed_compare_and_set_does_not_append_an_event() {
    let store = Store::in_memory().unwrap();
    let repo = repository(Path::new("/tmp/source-cas"));
    store.register_repository(&repo).unwrap();
    let (task, _) = store
        .create_task_with_event(
            new_task(&repo.id, "cas"),
            EventDraft::task(EventKind::TaskCreated, EventSource::Coco, json!({})),
        )
        .unwrap();

    assert!(matches!(
        store.transition_task_lifecycle_with_event(
            &task.id,
            TaskLifecycle::Ready,
            TaskLifecycle::Completed,
            None,
            EventDraft::task(EventKind::TurnStarted, EventSource::Coco, json!({})),
        ),
        Err(StoreError::InvalidTaskTransition { .. })
    ));
    assert_eq!(store.events_after(Some(&task.id), 0).unwrap().len(), 1);
    assert_eq!(
        store.task_by_id(&task.id).unwrap().unwrap().phase,
        TaskPhase::Provisioning
    );
}

#[test]
fn native_thread_status_is_persisted_losslessly_and_can_be_staled() {
    let store = Store::in_memory().unwrap();
    let repo = repository(Path::new("/tmp/source-runtime"));
    store.register_repository(&repo).unwrap();
    let (task, _) = store
        .create_task_with_event(
            new_task(&repo.id, "runtime"),
            EventDraft::task(EventKind::TaskCreated, EventSource::Coco, json!({})),
        )
        .unwrap();
    let (task, _) = store
        .transition_task_lifecycle_with_event(
            &task.id,
            TaskLifecycle::Provisioning,
            TaskLifecycle::Starting,
            None,
            EventDraft::task(EventKind::WorktreeCreated, EventSource::Git, json!({})),
        )
        .unwrap();
    let (task, _) = store
        .bind_thread_with_event(
            &task.id,
            TaskLifecycle::Starting,
            NewThreadBinding {
                thread_id: "thread-runtime".to_owned(),
                parent_thread_id: None,
                status: CodexThreadStatus::Idle,
                runtime_generation: "runtime-1".to_owned(),
            },
            EventDraft::task(EventKind::AgentStarted, EventSource::Codex, json!({})),
        )
        .unwrap();
    let (task, event) = store
        .observe_thread_status_with_event(
            &task.id,
            CodexThreadStatus::Active {
                active_flags: vec![
                    "waitingOnUserInput".to_owned(),
                    "futureFlag".to_owned(),
                    "waitingOnApproval".to_owned(),
                    "futureFlag".to_owned(),
                ],
            },
            "runtime-2",
            EventDraft::task(
                EventKind::ThreadStatusChanged,
                EventSource::Codex,
                json!({}),
            ),
        )
        .unwrap();

    assert_eq!(event.kind, EventKind::ThreadStatusChanged);
    assert_eq!(task.lifecycle, TaskLifecycle::Ready);
    assert_eq!(task.phase, TaskPhase::WaitingForApproval);
    assert_eq!(
        task.wait_reasons,
        [TaskWaitReason::Approval, TaskWaitReason::UserInput]
    );
    let snapshot = task.thread_runtime.unwrap();
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
    let task = store.task_by_id(&task.id).unwrap().unwrap();
    assert_eq!(task.lifecycle, TaskLifecycle::Ready);
    assert_eq!(task.phase, TaskPhase::Unavailable);
    assert!(!task.thread_runtime.unwrap().is_fresh);
}

#[test]
fn audit_round_trips_sanitized_metadata() {
    let store = Store::in_memory().unwrap();
    let audit = store
        .append_audit(AuditDraft {
            source: "mcp:test".to_owned(),
            action: "tasks.list".to_owned(),
            task_id: None,
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
    let task_id;
    let unfinished_creation_id;
    let turn_id;
    {
        let store = Store::open(&path).unwrap();
        let repo = repository(&temp.path().join("source"));
        store.register_repository(&repo).unwrap();
        let (unfinished_creation, _) = store
            .create_task_with_event(
                new_task(&repo.id, "unfinished-creation"),
                EventDraft::task(EventKind::TaskCreated, EventSource::Coco, json!({})),
            )
            .unwrap();
        unfinished_creation_id = unfinished_creation.id;
        let (task, _) = store
            .create_task_with_event(
                new_task(&repo.id, "restart"),
                EventDraft::task(EventKind::TaskCreated, EventSource::Coco, json!({})),
            )
            .unwrap();
        let (task, _) = store
            .transition_task_lifecycle_with_event(
                &task.id,
                TaskLifecycle::Provisioning,
                TaskLifecycle::Starting,
                None,
                EventDraft::task(EventKind::WorktreeCreated, EventSource::Git, json!({})),
            )
            .unwrap();
        let (task, _) = store
            .bind_thread_with_event(
                &task.id,
                TaskLifecycle::Starting,
                NewThreadBinding {
                    thread_id: "thread-restart".to_owned(),
                    parent_thread_id: None,
                    status: CodexThreadStatus::Idle,
                    runtime_generation: "runtime-before-restart".to_owned(),
                },
                EventDraft::task(EventKind::AgentStarted, EventSource::Codex, json!({})),
            )
            .unwrap();
        let (_, turn, _) = store
            .start_turn_with_event(
                &task.id,
                NewTurn {
                    operation_id: Some("restart-operation".to_owned()),
                    client_message_id: "restart-message".to_owned(),
                    codex_turn_id: Some("codex-restart-turn".to_owned()),
                    started_at_ms: None,
                },
                EventDraft::task(EventKind::TurnStarted, EventSource::Codex, json!({})),
            )
            .unwrap();
        task_id = task.id;
        turn_id = turn.id;
    }

    let store = Store::open(&path).unwrap();
    let reconciled = store.reconcile_unfinished().unwrap();
    assert_eq!(reconciled.len(), 2);
    let unfinished_creation = store.task_by_id(&unfinished_creation_id).unwrap().unwrap();
    assert_eq!(unfinished_creation.lifecycle, TaskLifecycle::Failed);
    assert_eq!(unfinished_creation.phase, TaskPhase::Failed);
    assert_eq!(
        store.task_by_id(&task_id).unwrap().unwrap().lifecycle,
        TaskLifecycle::Ready
    );
    let task = store.task_by_id(&task_id).unwrap().unwrap();
    assert_eq!(task.phase, TaskPhase::Unavailable);
    assert!(!task.thread_runtime.unwrap().is_fresh);
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
