use rusqlite::Connection;

use super::StoreError;

pub(super) fn migrate(connection: &Connection) -> Result<(), StoreError> {
    let mut version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version > 4 {
        return Err(StoreError::UnsupportedSchema(version));
    }
    if version == 4 {
        return Ok(());
    }
    if version == 1 {
        migrate_retired_task_goal(connection)?;
        version = 2;
    }
    if version == 2 {
        migrate_task_runtime_ownership(connection)?;
        version = 3;
    }
    if version == 3 {
        return migrate_workspace_vocabulary(connection);
    }
    create_current_schema(connection)
}

fn create_current_schema(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "BEGIN IMMEDIATE;
         CREATE TABLE IF NOT EXISTS repositories (
            id TEXT PRIMARY KEY,
            root_path TEXT NOT NULL UNIQUE,
            git_common_dir TEXT NOT NULL UNIQUE,
            display_name TEXT NOT NULL,
            is_linked_worktree INTEGER NOT NULL CHECK (is_linked_worktree IN (0, 1)),
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS workspaces (
            id TEXT PRIMARY KEY,
            create_operation_id TEXT UNIQUE,
            repository_id TEXT NOT NULL REFERENCES repositories(id),
            name TEXT NOT NULL,
            legacy_goal TEXT,
            context_mode TEXT NOT NULL CHECK (context_mode IN ('fresh', 'fork', 'handoff')),
            context_json TEXT NOT NULL CHECK (json_valid(context_json)),
            profile_json TEXT NOT NULL CHECK (json_valid(profile_json)),
            lifecycle TEXT NOT NULL CHECK (lifecycle IN (
                'provisioning', 'starting', 'ready', 'completed', 'failed'
            )),
            thread_status_json TEXT CHECK (
                thread_status_json IS NULL OR json_valid(thread_status_json)
            ),
            thread_status_generation TEXT,
            thread_status_observed_at_ms INTEGER,
            thread_status_is_fresh INTEGER NOT NULL DEFAULT 0
                CHECK (thread_status_is_fresh IN (0, 1)),
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
            CHECK (
                (thread_status_json IS NULL AND thread_status_generation IS NULL
                    AND thread_status_observed_at_ms IS NULL
                    AND thread_status_is_fresh = 0)
                OR
                (thread_status_json IS NOT NULL AND thread_status_generation IS NOT NULL
                    AND thread_status_observed_at_ms IS NOT NULL)
            ),
            UNIQUE(repository_id, name),
            UNIQUE(repository_id, branch_name)
         );
         CREATE TABLE IF NOT EXISTS turns (
            id TEXT PRIMARY KEY,
            workspace_id TEXT NOT NULL REFERENCES workspaces(id),
            operation_id TEXT UNIQUE,
            client_message_id TEXT NOT NULL UNIQUE,
            codex_turn_id TEXT UNIQUE,
            phase TEXT NOT NULL CHECK (phase IN (
                'starting', 'in_progress', 'completed', 'failed', 'interrupted'
            )),
            requested_at_ms INTEGER NOT NULL,
            started_at_ms INTEGER,
            completed_at_ms INTEGER,
            error_json TEXT CHECK (error_json IS NULL OR json_valid(error_json))
         );
         CREATE INDEX IF NOT EXISTS turns_workspace_idx
            ON turns(workspace_id, requested_at_ms);
         CREATE TABLE IF NOT EXISTS events (
            sequence INTEGER PRIMARY KEY AUTOINCREMENT,
            id TEXT NOT NULL UNIQUE,
            workspace_id TEXT REFERENCES workspaces(id),
            turn_id TEXT REFERENCES turns(id),
            kind TEXT NOT NULL,
            source TEXT NOT NULL CHECK (source IN ('coco', 'git', 'codex')),
            source_method TEXT,
            occurred_at_ms INTEGER,
            recorded_at_ms INTEGER NOT NULL,
            payload_json TEXT NOT NULL CHECK (json_valid(payload_json))
         );
         CREATE INDEX IF NOT EXISTS events_workspace_sequence_idx
            ON events(workspace_id, sequence);
         CREATE TABLE IF NOT EXISTS audit_events (
            sequence INTEGER PRIMARY KEY AUTOINCREMENT,
            id TEXT NOT NULL UNIQUE,
            source TEXT NOT NULL,
            action TEXT NOT NULL,
            workspace_id TEXT REFERENCES workspaces(id),
            operation_id TEXT,
            outcome TEXT NOT NULL CHECK (outcome IN ('succeeded', 'failed')),
            details_json TEXT NOT NULL CHECK (json_valid(details_json)),
            occurred_at_ms INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS audit_workspace_sequence_idx
            ON audit_events(workspace_id, sequence);
         PRAGMA user_version = 4;
         COMMIT;",
    )?;
    Ok(())
}

fn migrate_workspace_vocabulary(connection: &Connection) -> Result<(), StoreError> {
    let migration = connection.execute_batch(
        "BEGIN IMMEDIATE;
         ALTER TABLE tasks RENAME TO workspaces;
         ALTER TABLE turns RENAME COLUMN task_id TO workspace_id;
         ALTER TABLE events RENAME COLUMN task_id TO workspace_id;
         ALTER TABLE audit_events RENAME COLUMN task_id TO workspace_id;
         DROP INDEX IF EXISTS turns_task_idx;
         DROP INDEX IF EXISTS events_task_sequence_idx;
         DROP INDEX IF EXISTS audit_task_sequence_idx;
         CREATE INDEX turns_workspace_idx ON turns(workspace_id, requested_at_ms);
         CREATE INDEX events_workspace_sequence_idx ON events(workspace_id, sequence);
         CREATE INDEX audit_workspace_sequence_idx
            ON audit_events(workspace_id, sequence);
         UPDATE events SET kind = 'workspace.created' WHERE kind = 'task.created';
         UPDATE events SET kind = 'workspace.completed' WHERE kind = 'task.completed';
         UPDATE audit_events SET action = CASE action
            WHEN 'tasks.list' THEN 'workspaces.list'
            WHEN 'agents.status' THEN 'workspaces.status'
            WHEN 'changes.diff' THEN 'workspaces.diff'
            WHEN 'agents.send' THEN 'workspaces.send'
            ELSE action
         END;
         PRAGMA user_version = 4;
         COMMIT;",
    );
    if migration.is_err() {
        let _ = connection.execute_batch("ROLLBACK;");
    }
    migration?;
    Ok(())
}

fn migrate_task_runtime_ownership(connection: &Connection) -> Result<(), StoreError> {
    connection.pragma_update(None, "foreign_keys", false)?;
    let migration = connection.execute_batch(
        r#"BEGIN IMMEDIATE;
         CREATE TABLE tasks_v3 (
            id TEXT PRIMARY KEY,
            create_operation_id TEXT UNIQUE,
            repository_id TEXT NOT NULL REFERENCES repositories(id),
            name TEXT NOT NULL,
            legacy_goal TEXT,
            context_mode TEXT NOT NULL CHECK (context_mode IN ('fresh', 'fork', 'handoff')),
            context_json TEXT NOT NULL CHECK (json_valid(context_json)),
            profile_json TEXT NOT NULL CHECK (json_valid(profile_json)),
            lifecycle TEXT NOT NULL CHECK (lifecycle IN (
                'provisioning', 'starting', 'ready', 'completed', 'failed'
            )),
            thread_status_json TEXT CHECK (
                thread_status_json IS NULL OR json_valid(thread_status_json)
            ),
            thread_status_generation TEXT,
            thread_status_observed_at_ms INTEGER,
            thread_status_is_fresh INTEGER NOT NULL DEFAULT 0
                CHECK (thread_status_is_fresh IN (0, 1)),
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
            CHECK (
                (thread_status_json IS NULL AND thread_status_generation IS NULL
                    AND thread_status_observed_at_ms IS NULL
                    AND thread_status_is_fresh = 0)
                OR
                (thread_status_json IS NOT NULL AND thread_status_generation IS NOT NULL
                    AND thread_status_observed_at_ms IS NOT NULL)
            ),
            UNIQUE(repository_id, name),
            UNIQUE(repository_id, branch_name)
         );
         INSERT INTO tasks_v3 (
            id, create_operation_id, repository_id, name, legacy_goal, context_mode,
            context_json, profile_json, lifecycle, thread_status_json,
            thread_status_generation, thread_status_observed_at_ms,
            thread_status_is_fresh, branch_name, base_sha, worktree_path,
            codex_thread_id, parent_thread_id, active_turn_id, last_error_code,
            last_error_message, created_at_ms, updated_at_ms, completed_at_ms
         ) SELECT
            id, create_operation_id, repository_id, name, legacy_goal, context_mode,
            context_json, profile_json,
            CASE
                WHEN phase = 'provisioning' THEN 'provisioning'
                WHEN phase = 'starting' THEN 'starting'
                WHEN phase = 'completed' THEN 'completed'
                WHEN codex_thread_id IS NOT NULL THEN 'ready'
                ELSE 'failed'
            END,
            CASE phase
                WHEN 'idle' THEN '{"type":"idle"}'
                WHEN 'active' THEN '{"type":"active","activeFlags":[]}'
                WHEN 'waiting_for_approval' THEN
                    '{"type":"active","activeFlags":["waitingOnApproval"]}'
                WHEN 'waiting_for_input' THEN
                    '{"type":"active","activeFlags":["waitingOnUserInput"]}'
                ELSE NULL
            END,
            CASE WHEN phase IN (
                'idle', 'active', 'waiting_for_approval', 'waiting_for_input'
            ) THEN 'legacy-v2' ELSE NULL END,
            CASE WHEN phase IN (
                'idle', 'active', 'waiting_for_approval', 'waiting_for_input'
            ) THEN updated_at_ms ELSE NULL END,
            0,
            branch_name, base_sha, worktree_path, codex_thread_id, parent_thread_id,
            active_turn_id, last_error_code, last_error_message, created_at_ms,
            updated_at_ms, completed_at_ms
         FROM tasks;
         DROP TABLE tasks;
         ALTER TABLE tasks_v3 RENAME TO tasks;
         PRAGMA user_version = 3;
         COMMIT;"#,
    );
    if migration.is_err() {
        let _ = connection.execute_batch("ROLLBACK;");
    }
    let foreign_keys = connection.pragma_update(None, "foreign_keys", true);
    migration?;
    foreign_keys?;
    Ok(())
}

fn migrate_retired_task_goal(connection: &Connection) -> Result<(), StoreError> {
    connection.pragma_update(None, "foreign_keys", false)?;
    let migration = connection.execute_batch(
        "BEGIN IMMEDIATE;
         CREATE TABLE tasks_v2 (
            id TEXT PRIMARY KEY,
            create_operation_id TEXT UNIQUE,
            repository_id TEXT NOT NULL REFERENCES repositories(id),
            name TEXT NOT NULL,
            legacy_goal TEXT,
            context_mode TEXT NOT NULL CHECK (context_mode IN ('fresh', 'fork', 'handoff')),
            context_json TEXT NOT NULL CHECK (json_valid(context_json)),
            profile_json TEXT NOT NULL CHECK (json_valid(profile_json)),
            phase TEXT NOT NULL CHECK (phase IN (
                'provisioning', 'starting', 'active', 'waiting_for_approval',
                'waiting_for_input', 'idle', 'completed', 'failed', 'interrupted'
            )),
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
         INSERT INTO tasks_v2 (
            id, create_operation_id, repository_id, name, legacy_goal, context_mode,
            context_json, profile_json, phase, branch_name, base_sha, worktree_path,
            codex_thread_id, parent_thread_id, active_turn_id, last_error_code,
            last_error_message, created_at_ms, updated_at_ms, completed_at_ms
         ) SELECT
            id, create_operation_id, repository_id, name, goal, context_mode,
            context_json, profile_json, phase, branch_name, base_sha, worktree_path,
            codex_thread_id, parent_thread_id, active_turn_id, last_error_code,
            last_error_message, created_at_ms, updated_at_ms, completed_at_ms
         FROM tasks;
         DROP TABLE tasks;
         ALTER TABLE tasks_v2 RENAME TO tasks;
         PRAGMA user_version = 2;
         COMMIT;",
    );
    if migration.is_err() {
        let _ = connection.execute_batch("ROLLBACK;");
    }
    let foreign_keys = connection.pragma_update(None, "foreign_keys", true);
    migration?;
    foreign_keys?;
    Ok(())
}
