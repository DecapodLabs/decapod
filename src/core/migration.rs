//! Version detection and automatic migration system.
//!
//! This module handles detecting Decapod version changes and running
//! necessary migrations for schema updates, data transformations, etc.

use crate::core::error;
use crate::core::schemas;
use rusqlite::Connection;
use std::fs;
use std::path::Path;
use ulid::Ulid;

/// Current Decapod version from Cargo.toml
pub const DECAPOD_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Migration definition
pub struct Migration {
    /// Version this migration targets (e.g., "0.1.6")
    pub target_version: &'static str,
    /// Human-readable description
    pub description: &'static str,
    /// Migration function
    pub up: fn(&Path) -> Result<(), error::DecapodError>,
}

/// All migrations in chronological order
pub fn all_migrations() -> Vec<Migration> {
    vec![
        // Reconstruct event log from legacy databases
        Migration {
            target_version: "0.1.7",
            description: "Reconstruct todo event log from database state",
            up: migrate_reconstruct_todo_events,
        },
        Migration {
            target_version: "0.27.0",
            description: "Consolidate fragmented databases into core bins",
            up: migrate_consolidate_databases,
        },
    ]
}

/// Run any pending migrations (idempotent — safe to call every startup)
pub fn check_and_migrate(decapod_root: &Path) -> Result<(), error::DecapodError> {
    run_migrations(decapod_root)?;
    Ok(())
}

pub fn check_and_migrate_with_backup<F>(
    decapod_root: &Path,
    verify: F,
) -> Result<(), error::DecapodError>
where
    F: FnOnce(&Path) -> Result<(), error::DecapodError>,
{
    let data_root = decapod_root.join("data");
    if !schema_upgrade_pending(&data_root)? {
        run_migrations(decapod_root)?;
        verify(&data_root)?;
        return Ok(());
    }

    let Some(backup_dir) = create_data_backup(&data_root)? else {
        run_migrations(decapod_root)?;
        verify(&data_root)?;
        return Ok(());
    };

    let result = (|| -> Result<(), error::DecapodError> {
        run_migrations(decapod_root)?;
        verify(&data_root)?;
        Ok(())
    })();

    if let Err(err) = result {
        restore_data_backup(&data_root, &backup_dir)?;
        let _ = fs::remove_dir_all(&backup_dir);
        return Err(error::DecapodError::ValidationError(format!(
            "Migration failed; restored .decapod/data backup from {}: {}",
            backup_dir.display(),
            err
        )));
    }

    fs::remove_dir_all(&backup_dir).map_err(error::DecapodError::IoError)?;
    Ok(())
}

fn schema_upgrade_pending(data_root: &Path) -> Result<bool, error::DecapodError> {
    let todo_db = data_root.join(schemas::TODO_DB_NAME);
    if !todo_db.exists() {
        return Ok(false);
    }
    let conn = Connection::open(&todo_db).map_err(error::DecapodError::RusqliteError)?;
    let version_res: Result<String, _> = conn.query_row(
        "SELECT value FROM meta WHERE key = 'schema_version'",
        [],
        |row| row.get(0),
    );
    let current_version = version_res
        .ok()
        .and_then(|raw| raw.parse::<u32>().ok())
        .unwrap_or(0);
    Ok(current_version < schemas::TODO_SCHEMA_VERSION)
}

fn create_data_backup(data_root: &Path) -> Result<Option<std::path::PathBuf>, error::DecapodError> {
    if !data_root.exists() {
        return Ok(None);
    }
    let backup_dir = data_root.join(format!(
        ".migration_backup_{}_{}",
        DECAPOD_VERSION.replace('.', "_"),
        Ulid::new()
    ));
    fs::create_dir_all(&backup_dir).map_err(error::DecapodError::IoError)?;

    for entry in fs::read_dir(data_root).map_err(error::DecapodError::IoError)? {
        let entry = entry.map_err(error::DecapodError::IoError)?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".db") || name.ends_with(".jsonl") {
            fs::copy(&path, backup_dir.join(&name)).map_err(error::DecapodError::IoError)?;
        }
    }
    Ok(Some(backup_dir))
}

fn restore_data_backup(data_root: &Path, backup_dir: &Path) -> Result<(), error::DecapodError> {
    for entry in fs::read_dir(backup_dir).map_err(error::DecapodError::IoError)? {
        let entry = entry.map_err(error::DecapodError::IoError)?;
        let backup_file = entry.path();
        if !backup_file.is_file() {
            continue;
        }
        let name = entry.file_name();
        fs::copy(&backup_file, data_root.join(name)).map_err(error::DecapodError::IoError)?;
    }
    Ok(())
}

/// Run all idempotent migrations
fn run_migrations(decapod_root: &Path) -> Result<(), error::DecapodError> {
    for migration in all_migrations() {
        // All migrations are idempotent — they check internally if work is needed
        (migration.up)(decapod_root)?;
    }
    Ok(())
}

// Migration functions:

/// Reconstruct todo.events.jsonl from current todo.db state (for legacy migrations)
fn migrate_reconstruct_todo_events(decapod_root: &Path) -> Result<(), error::DecapodError> {
    use serde_json::json;
    use std::io::Write;

    let db_path = decapod_root.join("data/todo.db");
    let events_path = decapod_root.join("data/todo.events.jsonl");

    if !db_path.exists() {
        return Ok(()); // Nothing to migrate
    }

    // Check if events file is empty or missing
    let needs_migration = if events_path.exists() {
        fs::metadata(&events_path)
            .map(|m| m.len() == 0)
            .unwrap_or(true)
    } else {
        true
    };

    if !needs_migration {
        return Ok(()); // Already has events
    }

    let conn = Connection::open(&db_path).map_err(error::DecapodError::RusqliteError)?;

    // Read all tasks from database
    let mut stmt = conn
        .prepare("SELECT id, title, status, created_at FROM tasks ORDER BY created_at")
        .map_err(error::DecapodError::RusqliteError)?;

    let tasks = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?, // id
                row.get::<_, String>(1)?, // title
                row.get::<_, String>(2)?, // status
                row.get::<_, String>(3)?, // created_at (TEXT in schema)
            ))
        })
        .map_err(error::DecapodError::RusqliteError)?;

    // Create events file
    let mut file = fs::File::create(&events_path).map_err(error::DecapodError::IoError)?;

    // Write task.add event for each task
    for task in tasks {
        let (id, title, status, created_at) = task.map_err(error::DecapodError::RusqliteError)?;

        let event = json!({
            "ts": created_at,
            "event_id": format!("MIGRATION_{}", id),
            "event_type": "task.add",
            "task_id": id,
            "payload": {
                "title": title,
            },
            "actor": "migration",
        });

        writeln!(file, "{}", event).map_err(error::DecapodError::IoError)?;

        // If task is done, add task.done event
        if status == "done" {
            let complete_event = json!({
                "ts": created_at,
                "event_id": format!("MIGRATION_{}_DONE", id),
                "event_type": "task.done",
                "task_id": id,
                "payload": {},
                "actor": "migration",
            });

            writeln!(file, "{}", complete_event).map_err(error::DecapodError::IoError)?;
        }
    }

    Ok(())
}

fn migrate_consolidate_databases(decapod_root: &Path) -> Result<(), error::DecapodError> {
    let data_root = decapod_root.join("data");
    if !data_root.exists() {
        return Ok(());
    }

    // 1. Consolidate Governance Bin (health, policy, feedback, archive)
    let gov_path = data_root.join(schemas::GOVERNANCE_DB_NAME);
    let gov_conn = Connection::open(&gov_path).map_err(error::DecapodError::RusqliteError)?;
    gov_conn.execute_batch(schemas::HEALTH_DB_SCHEMA_CLAIMS)?;
    gov_conn.execute_batch(schemas::HEALTH_DB_SCHEMA_PROOF_EVENTS)?;
    gov_conn.execute_batch(schemas::HEALTH_DB_SCHEMA_HEALTH_CACHE)?;
    gov_conn.execute_batch(schemas::POLICY_DB_SCHEMA_APPROVALS)?;
    gov_conn.execute_batch(schemas::POLICY_DB_SCHEMA_INDEX)?;
    gov_conn.execute_batch(schemas::FEEDBACK_DB_SCHEMA)?;
    gov_conn.execute_batch(schemas::ARCHIVE_DB_SCHEMA)?;

    migrate_table(&data_root, "health.db", &gov_conn, "claims")?;
    migrate_table(&data_root, "health.db", &gov_conn, "proof_events")?;
    migrate_table(&data_root, "health.db", &gov_conn, "health_cache")?;
    migrate_table(&data_root, "policy.db", &gov_conn, "approvals")?;
    migrate_table(&data_root, "feedback.db", &gov_conn, "feedback")?;
    migrate_table(&data_root, "archive.db", &gov_conn, "archives")?;

    // 2. Consolidate Memory Bin (knowledge, federation, decisions, teammate)
    let mem_path = data_root.join(schemas::MEMORY_DB_NAME);
    let mem_conn = Connection::open(&mem_path).map_err(error::DecapodError::RusqliteError)?;
    mem_conn.execute_batch(schemas::MEMORY_DB_SCHEMA_META)?;
    mem_conn.execute_batch(schemas::MEMORY_DB_SCHEMA_NODES)?;
    mem_conn.execute_batch(schemas::MEMORY_DB_SCHEMA_SOURCES)?;
    mem_conn.execute_batch(schemas::MEMORY_DB_SCHEMA_EDGES)?;
    mem_conn.execute_batch(schemas::MEMORY_DB_SCHEMA_EVENTS)?;

    migrate_table(&data_root, "federation.db", &mem_conn, "nodes")?;
    migrate_table(&data_root, "federation.db", &mem_conn, "sources")?;
    migrate_table(&data_root, "federation.db", &mem_conn, "edges")?;
    migrate_table(&data_root, "federation.db", &mem_conn, "federation_events")?;

    // Legacy knowledge to nodes migration (simplified)
    let knowledge_db = data_root.join("knowledge.db");
    if knowledge_db.exists() {
        let k_conn = Connection::open(&knowledge_db).map_err(error::DecapodError::RusqliteError)?;
        // Guard against concurrent processes that may have created the file
        // but not yet populated the schema (race between Connection::open and
        // CREATE TABLE in initialize_knowledge_db).
        let has_table: bool = k_conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='knowledge'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);
        if has_table {
            let mut stmt = k_conn
                .prepare("SELECT id, title, content, provenance, created_at FROM knowledge")?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?;
            for r in rows {
                let (id, title, content, prov, ts) = r?;
                mem_conn.execute("INSERT OR IGNORE INTO nodes(id, node_type, title, body, created_at, updated_at, dir_path, scope) VALUES(?1, 'observation', ?2, ?3, ?4, ?4, '', 'repo')", rusqlite::params![id, title, content, ts])?;
                mem_conn.execute("INSERT OR IGNORE INTO sources(id, node_id, source, created_at) VALUES(?1, ?2, ?3, ?4)", rusqlite::params![Ulid::new().to_string(), id, prov, ts])?;
            }
        }
    }

    // 3. Consolidate Automation Bin (cron, reflex)
    let auto_path = data_root.join(schemas::AUTOMATION_DB_NAME);
    let auto_conn = Connection::open(&auto_path).map_err(error::DecapodError::RusqliteError)?;
    auto_conn.execute_batch(schemas::CRON_DB_SCHEMA)?;
    auto_conn.execute_batch(schemas::REFLEX_DB_SCHEMA)?;

    migrate_table(&data_root, "cron.db", &auto_conn, "cron_jobs")?;
    migrate_table(&data_root, "reflex.db", &auto_conn, "reflexes")?;

    // Cleanup legacy and backup files
    let legacy = [
        "health.db",
        "policy.db",
        "feedback.db",
        "archive.db",
        "knowledge.db",
        "federation.db",
        "decisions.db",
        "teammate.db",
        "cron.db",
        "reflex.db",
    ];
    for f in legacy {
        let p = data_root.join(f);
        if p.exists() {
            let _ = fs::remove_file(&p);
        }
        let bak = data_root.join(format!("{}.bak", f));
        if bak.exists() {
            let _ = fs::remove_file(&bak);
        }
    }

    Ok(())
}

fn migrate_table(
    data_root: &Path,
    source_db: &str,
    target_conn: &Connection,
    table: &str,
) -> Result<(), error::DecapodError> {
    let source_path = data_root.join(source_db);
    if !source_path.exists() {
        return Ok(());
    }

    target_conn
        .execute(
            &format!(
                "ATTACH DATABASE '{}' AS source",
                source_path.to_string_lossy()
            ),
            [],
        )
        .map_err(error::DecapodError::RusqliteError)?;

    let res = target_conn.execute(
        &format!(
            "INSERT OR IGNORE INTO main.{} SELECT * FROM source.{}",
            table, table
        ),
        [],
    );

    target_conn
        .execute("DETACH DATABASE source", [])
        .map_err(error::DecapodError::RusqliteError)?;

    res.map_err(error::DecapodError::RusqliteError)?;
    Ok(())
}
