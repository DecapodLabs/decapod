//! Canonical append-only event streams for the local datastore.
//!
//! Legacy JSONL files are migration inputs only. Runtime event writes go
//! through these tables so the local SQLite shape is the same shape exposed
//! by the cloud datastore.

use crate::core::db;
use crate::core::error;
use crate::core::schemas;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

pub const BROKER: &str = "broker";
pub const TODO: &str = "todo";
pub const FEDERATION: &str = "federation";
pub const EXTERNAL_ACTIONS: &str = "external_actions";
pub const TRACES: &str = "traces";
pub const VERIFICATION: &str = "verification";
pub const WATCHER: &str = "watcher";
pub const MAP: &str = "map";
pub const LCM: &str = "lcm";
pub const KNOWLEDGE: &str = "knowledge";

const STREAMS: &[(&str, &str)] = &[
    (BROKER, "broker_events"),
    (TODO, "todo_events"),
    (FEDERATION, "federation_events"),
    (EXTERNAL_ACTIONS, "external_actions_events"),
    (TRACES, "traces_events"),
    (VERIFICATION, "verification_events"),
    (WATCHER, "watcher_events"),
    (MAP, "map_events"),
    (LCM, "lcm_events"),
    (KNOWLEDGE, "knowledge_events"),
];

pub fn table_for_stream(stream: &str) -> Option<&'static str> {
    STREAMS
        .iter()
        .find_map(|(name, table)| (*name == stream).then_some(*table))
}

pub fn canonical_db_path(root: &Path) -> PathBuf {
    root.join(schemas::LOCAL_DB_NAME)
}

/// Ensure every canonical event stream exists in the shared datastore.
pub fn ensure_tables(conn: &Connection) -> Result<(), error::DecapodError> {
    for (_, table) in STREAMS {
        if *table != "federation_events" {
            let ddl = schemas::CANONICAL_EVENT_TABLE_SCHEMA.replace("{table}", table);
            conn.execute_batch(&ddl)?;
        }
    }

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS federation_events (
             event_id TEXT PRIMARY KEY,
             ts TEXT NOT NULL,
             event_type TEXT NOT NULL,
             node_id TEXT,
             payload TEXT NOT NULL,
             actor TEXT NOT NULL,
             seq INTEGER NOT NULL DEFAULT 0
         );
         CREATE TABLE IF NOT EXISTS task_events (
             event_id TEXT PRIMARY KEY,
             ts TEXT NOT NULL,
             event_type TEXT NOT NULL,
             task_id TEXT,
             payload TEXT NOT NULL,
             actor TEXT NOT NULL,
             seq INTEGER NOT NULL DEFAULT 0
         );",
    )?;

    ensure_column(
        conn,
        "federation_events",
        "seq",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(conn, "task_events", "seq", "INTEGER NOT NULL DEFAULT 0")?;
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_federation_events_seq ON federation_events(seq);
         CREATE INDEX IF NOT EXISTS idx_task_events_seq ON task_events(seq);
         CREATE TRIGGER IF NOT EXISTS federation_events_assign_seq
         AFTER INSERT ON federation_events
         WHEN NEW.seq = 0
         BEGIN
           UPDATE federation_events
           SET seq = (SELECT COALESCE(MAX(seq), 0) + 1 FROM federation_events WHERE event_id <> NEW.event_id)
           WHERE event_id = NEW.event_id;
         END;
         CREATE TRIGGER IF NOT EXISTS task_events_assign_seq
         AFTER INSERT ON task_events
         WHEN NEW.seq = 0
         BEGIN
           UPDATE task_events
           SET seq = (SELECT COALESCE(MAX(seq), 0) + 1 FROM task_events WHERE event_id <> NEW.event_id)
           WHERE event_id = NEW.event_id;
         END;",
    )?;
    backfill_sequence(conn, "federation_events")?;
    backfill_sequence(conn, "task_events")?;
    Ok(())
}

fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), error::DecapodError> {
    let exists: Option<String> = conn
        .query_row(
            &format!("SELECT name FROM pragma_table_info('{table}') WHERE name = ?1"),
            [column],
            |row| row.get(0),
        )
        .optional()
        .map_err(error::DecapodError::RusqliteError)?;
    if exists.is_none() {
        conn.execute(
            &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
            [],
        )?;
    }
    Ok(())
}

fn backfill_sequence(conn: &Connection, table: &str) -> Result<(), error::DecapodError> {
    conn.execute(
        &format!(
            "WITH ordered AS (
                 SELECT event_id, ROW_NUMBER() OVER (ORDER BY ts, event_id) AS next_seq
                 FROM {table}
                 WHERE seq = 0
             )
             UPDATE {table}
             SET seq = (SELECT next_seq FROM ordered WHERE ordered.event_id = {table}.event_id)
             WHERE event_id IN (SELECT event_id FROM ordered)"
        ),
        [],
    )?;
    Ok(())
}

pub fn append(root: &Path, stream: &str, event: &Value) -> Result<u64, error::DecapodError> {
    let path = canonical_db_path(root);
    let conn = db::db_connect(&path.to_string_lossy())?;
    ensure_tables(&conn)?;
    append_on_conn(&conn, stream, event)
}

pub fn append_on_conn(
    conn: &Connection,
    stream: &str,
    event: &Value,
) -> Result<u64, error::DecapodError> {
    let table = table_for_stream(stream).ok_or_else(|| {
        error::DecapodError::ValidationError(format!("unknown event stream: {stream}"))
    })?;
    let event_id = event_id(event);
    let ts = event
        .get("ts")
        .and_then(Value::as_str)
        .unwrap_or("1970-01-01T00:00:00Z")
        .to_string();
    let event_type = event
        .get("event_type")
        .or_else(|| event.get("op"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let actor = event
        .get("actor")
        .or_else(|| event.get("actor_id"))
        .and_then(Value::as_str)
        .unwrap_or("decapod");
    let seq: u64 = conn.query_row(
        &format!("SELECT COALESCE(MAX(seq), 0) + 1 FROM {table}"),
        [],
        |row| row.get(0),
    )?;
    conn.execute(
        &format!(
            "INSERT OR IGNORE INTO {table}(event_id, ts, seq, event_type, payload, actor)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)"
        ),
        params![
            event_id,
            ts,
            seq,
            event_type,
            serde_json::to_string(event).unwrap(),
            actor
        ],
    )?;
    Ok(seq)
}

fn event_id(event: &Value) -> String {
    if let Some(id) = event.get("event_id").and_then(Value::as_str) {
        return id.to_string();
    }
    crate::core::ulid::new_ulid()
}

/// Import the legacy JSONL streams without modifying or deleting the source.
/// Re-running this function is safe because event IDs are unique.
pub fn import_legacy_jsonl(
    data_root: &Path,
    conn: &Connection,
) -> Result<usize, error::DecapodError> {
    ensure_tables(conn)?;
    let files = [
        ("broker.events.jsonl", BROKER),
        (schemas::TODO_EVENTS_NAME, TODO),
        (schemas::FEDERATION_EVENTS_NAME, FEDERATION),
        ("external_actions.events.jsonl", EXTERNAL_ACTIONS),
        ("traces.jsonl", TRACES),
        ("traces.events.jsonl", TRACES),
        ("verification_events.jsonl", VERIFICATION),
        ("watcher.events.jsonl", WATCHER),
        (schemas::MAP_EVENTS_NAME, MAP),
        (schemas::LCM_EVENTS_NAME, LCM),
        ("knowledge.retrieval.events.jsonl", KNOWLEDGE),
        ("knowledge.decay.events.jsonl", KNOWLEDGE),
        ("knowledge.promotions.jsonl", KNOWLEDGE),
        ("knowledge.promotions.events.jsonl", KNOWLEDGE),
    ];
    let mut imported = 0;
    for (filename, stream) in files {
        let path = data_root.join(filename);
        if !path.is_file() {
            continue;
        }
        let file = fs::File::open(&path).map_err(error::DecapodError::IoError)?;
        for (line_no, line) in BufReader::new(file).lines().enumerate() {
            let line = line.map_err(error::DecapodError::IoError)?;
            if line.trim().is_empty() {
                continue;
            }
            let mut value: Value = serde_json::from_str(&line).map_err(|e| {
                error::DecapodError::ValidationError(format!(
                    "invalid legacy event in {}:{}: {e}",
                    path.display(),
                    line_no + 1
                ))
            })?;
            if value.get("event_id").and_then(Value::as_str).is_none() {
                let mut digest = Sha256::new();
                digest.update(stream.as_bytes());
                digest.update([0]);
                digest.update(line_no.to_string().as_bytes());
                digest.update([0]);
                digest.update(line.as_bytes());
                let id = format!(
                    "legacy_{}",
                    digest
                        .finalize()
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<String>()
                );
                value["event_id"] = Value::String(id);
            }
            let before: i64 = conn.query_row(
                &format!(
                    "SELECT COUNT(*) FROM {} WHERE event_id = ?1",
                    table_for_stream(stream).unwrap()
                ),
                [value.get("event_id").and_then(Value::as_str).unwrap()],
                |row| row.get(0),
            )?;
            append_on_conn(conn, stream, &value)?;
            if before == 0 {
                imported += 1;
            }
        }
    }
    Ok(imported)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn legacy_import_is_idempotent_and_preserves_json() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("broker.events.jsonl"),
            "{\"event_id\":\"e1\",\"ts\":\"2026-01-01T00:00:00Z\",\"op\":\"x\"}\n",
        )
        .unwrap();
        let conn = Connection::open(dir.path().join("decapod.db")).unwrap();
        assert_eq!(import_legacy_jsonl(dir.path(), &conn).unwrap(), 1);
        assert_eq!(import_legacy_jsonl(dir.path(), &conn).unwrap(), 0);
        let payload: String = conn
            .query_row(
                "SELECT payload FROM broker_events WHERE event_id = 'e1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(serde_json::from_str::<Value>(&payload).unwrap()["op"], "x");
    }
}
