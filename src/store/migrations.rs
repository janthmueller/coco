use rusqlite::Connection;

use super::StoreError;

pub(super) fn migrate(connection: &Connection) -> Result<(), StoreError> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version > 2 {
        return Err(StoreError::UnsupportedSchema(version));
    }
    if version == 2 {
        return Ok(());
    }
    if version == 1 {
        return migrate_retired_task_goal(connection);
    }
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
         CREATE TABLE IF NOT EXISTS tasks (
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
         CREATE TABLE IF NOT EXISTS turns (
            id TEXT PRIMARY KEY,
            task_id TEXT NOT NULL REFERENCES tasks(id),
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
         CREATE INDEX IF NOT EXISTS turns_task_idx ON turns(task_id, requested_at_ms);
         CREATE TABLE IF NOT EXISTS events (
            sequence INTEGER PRIMARY KEY AUTOINCREMENT,
            id TEXT NOT NULL UNIQUE,
            task_id TEXT REFERENCES tasks(id),
            turn_id TEXT REFERENCES turns(id),
            kind TEXT NOT NULL,
            source TEXT NOT NULL CHECK (source IN ('coco', 'git', 'codex')),
            source_method TEXT,
            occurred_at_ms INTEGER,
            recorded_at_ms INTEGER NOT NULL,
            payload_json TEXT NOT NULL CHECK (json_valid(payload_json))
         );
         CREATE INDEX IF NOT EXISTS events_task_sequence_idx ON events(task_id, sequence);
         CREATE TABLE IF NOT EXISTS audit_events (
            sequence INTEGER PRIMARY KEY AUTOINCREMENT,
            id TEXT NOT NULL UNIQUE,
            source TEXT NOT NULL,
            action TEXT NOT NULL,
            task_id TEXT REFERENCES tasks(id),
            operation_id TEXT,
            outcome TEXT NOT NULL CHECK (outcome IN ('succeeded', 'failed')),
            details_json TEXT NOT NULL CHECK (json_valid(details_json)),
            occurred_at_ms INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS audit_task_sequence_idx
            ON audit_events(task_id, sequence);
         PRAGMA user_version = 2;
         COMMIT;",
    )?;
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
