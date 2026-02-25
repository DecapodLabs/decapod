//! Version detection and automatic migration system.
//!
//! This module handles detecting Decapod version changes and running
//! necessary migrations for schema updates, data transformations, etc.

use crate::core::db;
use crate::core::error;
use crate::core::schemas;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use ulid::Ulid;

/// Current Decapod version from Cargo.toml
pub const DECAPOD_VERSION: &str = env!("CARGO_PKG_VERSION");
const GENERATED_VERSION_COUNTER: &str = "generated/version_counter.json";
const GENERATED_APPLIED_MIGRATIONS: &str = "generated/migrations/applied.json";

/// Migration definition
pub struct Migration {
    /// Stable migration identifier for durable applied-ledger tracking.
    pub id: &'static str,
    /// Minimum decapod version where this migration is valid to run.
    pub min_version: &'static str,
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
            id: "todo.events.reconstruct.v001",
            min_version: "0.1.7",
            target_version: "0.1.7",
            description: "Reconstruct todo event log from database state",
            up: migrate_reconstruct_todo_events,
        },
        Migration {
            id: "db.consolidate.core_bins.v001",
            min_version: "0.27.0",
            target_version: "0.27.0",
            description: "Consolidate fragmented databases into core bins",
            up: migrate_consolidate_databases,
        },
        Migration {
            id: "todo.ids.typed.v015",
            min_version: "0.41.1",
            target_version: "0.41.1",
            description: "Migrate legacy todo IDs to typed <type4>_<16> format",
            up: migrate_todo_ids_to_typed_format,
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GeneratedVersionCounter {
    schema_version: String,
    version_count: u64,
    initialized_with_version: String,
    last_seen_version: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppliedMigrationEntry {
    id: String,
    min_version: String,
    target_version: String,
    applied_at: String,
    applied_by_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct AppliedMigrationLedger {
    schema_version: String,
    entries: Vec<AppliedMigrationEntry>,
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
    let conn = db::db_connect(&todo_db.to_string_lossy())?;
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
    touch_generated_version_counter(decapod_root)?;
    let mut applied = load_applied_migrations(decapod_root)?;
    for migration in all_migrations() {
        if !version_gte(DECAPOD_VERSION, migration.min_version) {
            continue;
        }
        if !version_gte(DECAPOD_VERSION, migration.target_version) {
            continue;
        }
        if applied.entries.iter().any(|e| e.id == migration.id) {
            continue;
        }
        (migration.up)(decapod_root)?;
        applied.entries.push(AppliedMigrationEntry {
            id: migration.id.to_string(),
            min_version: migration.min_version.to_string(),
            target_version: migration.target_version.to_string(),
            applied_at: crate::core::time::now_epoch_z(),
            applied_by_version: DECAPOD_VERSION.to_string(),
        });
        store_applied_migrations(decapod_root, &applied)?;
    }
    Ok(())
}

fn parse_version(v: &str) -> [u64; 3] {
    let mut out = [0u64; 3];
    for (idx, part) in v.split('.').take(3).enumerate() {
        let digits: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
        out[idx] = digits.parse::<u64>().unwrap_or(0);
    }
    out
}

fn version_gte(left: &str, right: &str) -> bool {
    parse_version(left) >= parse_version(right)
}

fn touch_generated_version_counter(decapod_root: &Path) -> Result<(), error::DecapodError> {
    let path = decapod_root.join(GENERATED_VERSION_COUNTER);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(error::DecapodError::IoError)?;
    }
    let now = crate::core::time::now_epoch_z();
    let mut counter = if path.exists() {
        let raw = fs::read_to_string(&path).map_err(error::DecapodError::IoError)?;
        serde_json::from_str::<GeneratedVersionCounter>(&raw).unwrap_or(GeneratedVersionCounter {
            schema_version: "1.0.0".to_string(),
            version_count: 1,
            initialized_with_version: DECAPOD_VERSION.to_string(),
            last_seen_version: DECAPOD_VERSION.to_string(),
            updated_at: now.clone(),
        })
    } else {
        GeneratedVersionCounter {
            schema_version: "1.0.0".to_string(),
            version_count: 1,
            initialized_with_version: DECAPOD_VERSION.to_string(),
            last_seen_version: DECAPOD_VERSION.to_string(),
            updated_at: now.clone(),
        }
    };

    if counter.last_seen_version != DECAPOD_VERSION {
        counter.version_count = counter.version_count.saturating_add(1);
        counter.last_seen_version = DECAPOD_VERSION.to_string();
    }
    counter.updated_at = now;
    let body = serde_json::to_string_pretty(&counter)
        .map_err(|e| error::DecapodError::ValidationError(e.to_string()))?;
    fs::write(path, body).map_err(error::DecapodError::IoError)?;
    Ok(())
}

fn load_applied_migrations(
    decapod_root: &Path,
) -> Result<AppliedMigrationLedger, error::DecapodError> {
    let path = decapod_root.join(GENERATED_APPLIED_MIGRATIONS);
    if !path.exists() {
        return Ok(AppliedMigrationLedger {
            schema_version: "1.0.0".to_string(),
            entries: vec![],
        });
    }
    let raw = fs::read_to_string(path).map_err(error::DecapodError::IoError)?;
    let mut ledger = serde_json::from_str::<AppliedMigrationLedger>(&raw).unwrap_or_default();
    if ledger.schema_version.is_empty() {
        ledger.schema_version = "1.0.0".to_string();
    }
    Ok(ledger)
}

fn store_applied_migrations(
    decapod_root: &Path,
    ledger: &AppliedMigrationLedger,
) -> Result<(), error::DecapodError> {
    let path = decapod_root.join(GENERATED_APPLIED_MIGRATIONS);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(error::DecapodError::IoError)?;
    }
    let body = serde_json::to_string_pretty(ledger)
        .map_err(|e| error::DecapodError::ValidationError(e.to_string()))?;
    fs::write(path, body).map_err(error::DecapodError::IoError)?;
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

    let conn = db::db_connect(&db_path.to_string_lossy())?;

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
    let gov_conn = db::db_connect(&gov_path.to_string_lossy())?;
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

    // 2. Consolidate Memory Bin (knowledge, federation, decisions, aptitude)
    let mem_path = data_root.join(schemas::MEMORY_DB_NAME);
    let mem_conn = db::db_connect(&mem_path.to_string_lossy())?;
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
        let k_conn = db::db_connect(&knowledge_db.to_string_lossy())?;
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
    let auto_conn = db::db_connect(&auto_path.to_string_lossy())?;
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
        "aptitude.db",
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

fn is_typed_todo_id(id: &str) -> bool {
    let mut parts = id.split('_');
    let Some(prefix) = parts.next() else {
        return false;
    };
    let Some(suffix) = parts.next() else {
        return false;
    };
    if parts.next().is_some() {
        return false;
    }
    prefix.len() == 4
        && prefix.chars().all(|c| c.is_ascii_lowercase())
        && suffix.len() == 16
        && suffix.chars().all(|c| c.is_ascii_alphanumeric())
}

fn typed_todo_type(category: &str, title: &str, old_id: &str) -> &'static str {
    let c = category.to_ascii_lowercase();
    let t = title.to_ascii_lowercase();
    let all = format!("{c} {t} {old_id}").to_ascii_lowercase();
    if c.contains("test") || all.contains("test") {
        "test"
    } else if c.contains("doc") || all.contains("readme") || all.contains("doc") {
        "docs"
    } else if c.contains("bug") || all.contains("fix") || all.contains("bug") {
        "bugs"
    } else if c.contains("sec") || all.contains("security") || all.contains("auth") {
        "secu"
    } else if c.contains("perf") || all.contains("perf") {
        "perf"
    } else if c.contains("infra") || all.contains("infra") || all.contains("deploy") {
        "infr"
    } else if c.contains("backend") || c == "database" || all.contains("server") {
        "bend"
    } else if c.contains("frontend") || all.contains("ui") || all.contains("web") {
        "fend"
    } else if c == "ci" || all.contains("ci") || all.contains("pipeline") {
        "cicd"
    } else if c.contains("refactor") || all.contains("cleanup") {
        "reft"
    } else if c.contains("tool") || all.contains("cli") {
        "tool"
    } else if c.contains("feature") || all.contains("feature") || all.contains("implement") {
        "feat"
    } else {
        "arch"
    }
}

fn typed_todo_suffix(seed: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(seed.as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(16);
    for b in digest {
        out.push_str(&format!("{:02x}", b));
        if out.len() >= 16 {
            out.truncate(16);
            break;
        }
    }
    out
}

fn rewrite_csv_task_ids(csv: &str, id_map: &HashMap<String, String>) -> String {
    let mut changed = false;
    let mut mapped = Vec::new();
    for part in csv.split(',') {
        let token = part.trim();
        if token.is_empty() {
            continue;
        }
        if let Some(new_id) = id_map.get(token) {
            changed = true;
            mapped.push(new_id.clone());
        } else {
            mapped.push(token.to_string());
        }
    }
    if changed {
        mapped.join(",")
    } else {
        csv.to_string()
    }
}

fn rewrite_json_task_ids(value: &mut Value, id_map: &HashMap<String, String>) {
    match value {
        Value::String(s) => {
            if let Some(mapped) = id_map.get(s) {
                *s = mapped.clone();
            } else if s.contains(',') {
                *s = rewrite_csv_task_ids(s, id_map);
            }
        }
        Value::Array(items) => {
            for item in items {
                rewrite_json_task_ids(item, id_map);
            }
        }
        Value::Object(obj) => {
            for v in obj.values_mut() {
                rewrite_json_task_ids(v, id_map);
            }
        }
        _ => {}
    }
}

fn table_has_column(
    conn: &Connection,
    table: &str,
    column: &str,
) -> Result<bool, error::DecapodError> {
    let pragma = format!("PRAGMA table_info({})", table);
    let mut stmt = conn
        .prepare(&pragma)
        .map_err(error::DecapodError::RusqliteError)?;
    let mut rows = stmt.query([]).map_err(error::DecapodError::RusqliteError)?;
    while let Some(row) = rows.next().map_err(error::DecapodError::RusqliteError)? {
        let name: String = row.get(1).map_err(error::DecapodError::RusqliteError)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool, error::DecapodError> {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
        [table],
        |_| Ok(true),
    )
    .optional()
    .map_err(error::DecapodError::RusqliteError)
    .map(|v| v.unwrap_or(false))
}

fn migrate_todo_ids_to_typed_format(decapod_root: &Path) -> Result<(), error::DecapodError> {
    let data_root = decapod_root.join("data");
    let todo_db = data_root.join(schemas::TODO_DB_NAME);
    if !todo_db.exists() {
        return Ok(());
    }

    let mut conn = db::db_connect(&todo_db.to_string_lossy())?;
    let tasks_exists: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='tasks'",
            [],
            |_| Ok(true),
        )
        .optional()
        .map_err(error::DecapodError::RusqliteError)?
        .unwrap_or(false);
    if !tasks_exists {
        return Ok(());
    }

    let mut existing_ids = HashSet::new();
    let mut legacy_rows = Vec::new();
    {
        let has_category = table_has_column(&conn, "tasks", "category")?;
        let has_title = table_has_column(&conn, "tasks", "title")?;
        let select_sql = match (has_category, has_title) {
            (true, true) => "SELECT id, category, title FROM tasks ORDER BY created_at, id",
            (true, false) => "SELECT id, category, '' as title FROM tasks ORDER BY created_at, id",
            (false, true) => "SELECT id, '' as category, title FROM tasks ORDER BY created_at, id",
            (false, false) => {
                "SELECT id, '' as category, '' as title FROM tasks ORDER BY created_at, id"
            }
        };
        let mut stmt = conn
            .prepare(select_sql)
            .map_err(error::DecapodError::RusqliteError)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1).unwrap_or_default(),
                    row.get::<_, String>(2).unwrap_or_default(),
                ))
            })
            .map_err(error::DecapodError::RusqliteError)?;
        for row in rows {
            let (id, category, title) = row.map_err(error::DecapodError::RusqliteError)?;
            existing_ids.insert(id.clone());
            if !is_typed_todo_id(&id) {
                legacy_rows.push((id, category, title));
            }
        }
    }
    if legacy_rows.is_empty() {
        return Ok(());
    }

    let mut id_map: HashMap<String, String> = HashMap::new();
    for (old_id, category, title) in legacy_rows {
        let task_type = typed_todo_type(&category, &title, &old_id);
        let mut attempt = 0usize;
        loop {
            let seed = if attempt == 0 {
                old_id.clone()
            } else {
                format!("{old_id}:{attempt}")
            };
            let candidate = format!("{}_{}", task_type, typed_todo_suffix(&seed));
            if candidate == old_id {
                id_map.insert(old_id.clone(), candidate);
                break;
            }
            if !existing_ids.contains(&candidate) && !id_map.values().any(|v| v == &candidate) {
                id_map.insert(old_id.clone(), candidate.clone());
                existing_ids.insert(candidate);
                break;
            }
            attempt += 1;
        }
    }

    let sql = include_str!("sql/todo_task_id_v15_migration.sql");
    conn.execute_batch("PRAGMA foreign_keys=OFF;")
        .map_err(error::DecapodError::RusqliteError)?;
    let tx = conn
        .transaction()
        .map_err(error::DecapodError::RusqliteError)?;

    tx.execute(
        "CREATE TEMP TABLE task_id_migration_map(
            old_id TEXT PRIMARY KEY,
            new_id TEXT NOT NULL UNIQUE
        )",
        [],
    )
    .map_err(error::DecapodError::RusqliteError)?;
    for (old_id, new_id) in &id_map {
        tx.execute(
            "INSERT INTO task_id_migration_map(old_id, new_id) VALUES(?1, ?2)",
            [old_id, new_id],
        )
        .map_err(error::DecapodError::RusqliteError)?;
    }

    let full_schema_compatible = table_has_column(&tx, "tasks", "parent_task_id")?
        && table_exists(&tx, "task_verification")?
        && table_has_column(&tx, "task_verification", "todo_id")?
        && table_exists(&tx, "task_owners")?
        && table_has_column(&tx, "task_owners", "task_id")?
        && table_exists(&tx, "task_dependencies")?
        && table_has_column(&tx, "task_dependencies", "task_id")?
        && table_has_column(&tx, "task_dependencies", "depends_on_task_id")?
        && table_exists(&tx, "task_events")?
        && table_has_column(&tx, "task_events", "task_id")?;

    if full_schema_compatible {
        tx.execute_batch(sql)
            .map_err(error::DecapodError::RusqliteError)?;
    } else {
        let run_if = |cond: bool, statement: &str| -> Result<(), error::DecapodError> {
            if cond {
                tx.execute(statement, [])
                    .map_err(error::DecapodError::RusqliteError)?;
            }
            Ok(())
        };
        run_if(
            table_has_column(&tx, "tasks", "parent_task_id")?,
            "UPDATE tasks
             SET parent_task_id = (
                 SELECT m.new_id FROM task_id_migration_map m WHERE m.old_id = tasks.parent_task_id
             )
             WHERE parent_task_id IN (SELECT old_id FROM task_id_migration_map)",
        )?;
        run_if(
            table_exists(&tx, "task_verification")?
                && table_has_column(&tx, "task_verification", "todo_id")?,
            "UPDATE task_verification
             SET todo_id = (
                 SELECT m.new_id FROM task_id_migration_map m WHERE m.old_id = task_verification.todo_id
             )
             WHERE todo_id IN (SELECT old_id FROM task_id_migration_map)",
        )?;
        run_if(
            table_exists(&tx, "task_owners")? && table_has_column(&tx, "task_owners", "task_id")?,
            "UPDATE task_owners
             SET task_id = (
                 SELECT m.new_id FROM task_id_migration_map m WHERE m.old_id = task_owners.task_id
             )
             WHERE task_id IN (SELECT old_id FROM task_id_migration_map)",
        )?;
        run_if(
            table_exists(&tx, "task_dependencies")?
                && table_has_column(&tx, "task_dependencies", "task_id")?,
            "UPDATE task_dependencies
             SET task_id = (
                 SELECT m.new_id FROM task_id_migration_map m WHERE m.old_id = task_dependencies.task_id
             )
             WHERE task_id IN (SELECT old_id FROM task_id_migration_map)",
        )?;
        run_if(
            table_exists(&tx, "task_dependencies")?
                && table_has_column(&tx, "task_dependencies", "depends_on_task_id")?,
            "UPDATE task_dependencies
             SET depends_on_task_id = (
                 SELECT m.new_id FROM task_id_migration_map m WHERE m.old_id = task_dependencies.depends_on_task_id
             )
             WHERE depends_on_task_id IN (SELECT old_id FROM task_id_migration_map)",
        )?;
        run_if(
            table_exists(&tx, "task_events")? && table_has_column(&tx, "task_events", "task_id")?,
            "UPDATE task_events
             SET task_id = (
                 SELECT m.new_id FROM task_id_migration_map m WHERE m.old_id = task_events.task_id
             )
             WHERE task_id IN (SELECT old_id FROM task_id_migration_map)",
        )?;
        tx.execute(
            "UPDATE tasks
             SET id = (
                 SELECT m.new_id FROM task_id_migration_map m WHERE m.old_id = tasks.id
             )
             WHERE id IN (SELECT old_id FROM task_id_migration_map)",
            [],
        )
        .map_err(error::DecapodError::RusqliteError)?;
    }

    {
        let has_depends_on = table_has_column(&tx, "tasks", "depends_on")?;
        let has_blocks = table_has_column(&tx, "tasks", "blocks")?;
        let select_sql = match (has_depends_on, has_blocks) {
            (true, true) => "SELECT id, depends_on, blocks FROM tasks",
            (true, false) => "SELECT id, depends_on, '' as blocks FROM tasks",
            (false, true) => "SELECT id, '' as depends_on, blocks FROM tasks",
            (false, false) => "SELECT id, '' as depends_on, '' as blocks FROM tasks",
        };
        let mut stmt = tx
            .prepare(select_sql)
            .map_err(error::DecapodError::RusqliteError)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1).unwrap_or_default(),
                    row.get::<_, String>(2).unwrap_or_default(),
                ))
            })
            .map_err(error::DecapodError::RusqliteError)?;
        let mut rewrites = Vec::new();
        for row in rows {
            let (task_id, depends_on, blocks) = row.map_err(error::DecapodError::RusqliteError)?;
            let next_depends = rewrite_csv_task_ids(&depends_on, &id_map);
            let next_blocks = rewrite_csv_task_ids(&blocks, &id_map);
            if next_depends != depends_on || next_blocks != blocks {
                rewrites.push((task_id, next_depends, next_blocks));
            }
        }
        drop(stmt);
        if has_depends_on || has_blocks {
            for (task_id, depends_on, blocks) in rewrites {
                match (has_depends_on, has_blocks) {
                    (true, true) => {
                        tx.execute(
                            "UPDATE tasks SET depends_on = ?1, blocks = ?2 WHERE id = ?3",
                            rusqlite::params![depends_on, blocks, task_id],
                        )
                        .map_err(error::DecapodError::RusqliteError)?;
                    }
                    (true, false) => {
                        tx.execute(
                            "UPDATE tasks SET depends_on = ?1 WHERE id = ?2",
                            rusqlite::params![depends_on, task_id],
                        )
                        .map_err(error::DecapodError::RusqliteError)?;
                    }
                    (false, true) => {
                        tx.execute(
                            "UPDATE tasks SET blocks = ?1 WHERE id = ?2",
                            rusqlite::params![blocks, task_id],
                        )
                        .map_err(error::DecapodError::RusqliteError)?;
                    }
                    (false, false) => {}
                }
            }
        }
    }

    if tx
        .query_row(
            "SELECT 1 FROM pragma_table_info('tasks') WHERE name='hash'",
            [],
            |_| Ok(true),
        )
        .optional()
        .map_err(error::DecapodError::RusqliteError)?
        .unwrap_or(false)
    {
        tx.execute(
            "UPDATE tasks
             SET hash = lower(substr(id, instr(id, '_') + 1, 6))
             WHERE instr(id, '_') > 0",
            [],
        )
        .map_err(error::DecapodError::RusqliteError)?;
    }

    if table_exists(&tx, "task_events")? && table_has_column(&tx, "task_events", "payload")? {
        let mut stmt = tx
            .prepare("SELECT event_id, payload FROM task_events")
            .map_err(error::DecapodError::RusqliteError)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(error::DecapodError::RusqliteError)?;
        let mut payload_rewrites = Vec::new();
        for row in rows {
            let (event_id, payload_raw) = row.map_err(error::DecapodError::RusqliteError)?;
            if let Ok(mut payload_json) = serde_json::from_str::<Value>(&payload_raw) {
                rewrite_json_task_ids(&mut payload_json, &id_map);
                if let Ok(next_raw) = serde_json::to_string(&payload_json) {
                    if next_raw != payload_raw {
                        payload_rewrites.push((event_id, next_raw));
                    }
                }
            }
        }
        drop(stmt);
        for (event_id, payload) in payload_rewrites {
            tx.execute(
                "UPDATE task_events SET payload = ?1 WHERE event_id = ?2",
                rusqlite::params![payload, event_id],
            )
            .map_err(error::DecapodError::RusqliteError)?;
        }
    }

    tx.commit().map_err(error::DecapodError::RusqliteError)?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")
        .map_err(error::DecapodError::RusqliteError)?;

    let events_path = data_root.join(schemas::TODO_EVENTS_NAME);
    if events_path.exists() {
        let content = fs::read_to_string(&events_path).map_err(error::DecapodError::IoError)?;
        let mut rewritten = Vec::new();
        let mut changed = false;
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let mut value: Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => {
                    rewritten.push(line.to_string());
                    continue;
                }
            };
            rewrite_json_task_ids(&mut value, &id_map);
            let next = serde_json::to_string(&value)
                .map_err(|e| error::DecapodError::ValidationError(e.to_string()))?;
            if next != line {
                changed = true;
            }
            rewritten.push(next);
        }
        if changed {
            fs::write(events_path, rewritten.join("\n") + "\n")
                .map_err(error::DecapodError::IoError)?;
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
