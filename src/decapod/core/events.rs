//! Canonical append-only event streams for the local datastore.
//!
//! Legacy JSONL files are migration inputs only. Runtime event writes go
//! through these tables so the local SQLite shape is the same shape exposed
//! by the cloud datastore.

use crate::core::db;
use crate::core::error;
use crate::core::schemas;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{BufRead, BufReader, Read};
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

pub const STREAMS: &[(&str, &str)] = &[
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

const LEGACY_FILES: &[(&str, &str)] = &[
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

pub fn table_for_stream(stream: &str) -> Option<&'static str> {
    STREAMS
        .iter()
        .find_map(|(name, table)| (*name == stream).then_some(*table))
}

pub fn canonical_db_path(root: &Path) -> PathBuf {
    root.join(schemas::LOCAL_DB_NAME)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredEvent {
    pub stream: String,
    pub event_id: String,
    pub ts: String,
    pub seq: u64,
    pub event_type: String,
    pub payload: Value,
    pub actor: String,
}

/// Read canonical events without exposing their current table layout to callers.
pub fn query(
    root: &Path,
    stream: &str,
    limit: usize,
) -> Result<Vec<StoredEvent>, error::DecapodError> {
    let table = table_for_stream(stream).ok_or_else(|| {
        error::DecapodError::ValidationError(format!("unknown event stream: {stream}"))
    })?;
    let path = canonical_db_path(root);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let conn = db::db_connect_for_validate(&path.to_string_lossy())?;
    let table_exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        [table],
        |row| row.get(0),
    )?;
    if !table_exists {
        return Ok(Vec::new());
    }
    let sql_limit = i64::try_from(limit.max(1)).unwrap_or(i64::MAX);
    let mut stmt = conn.prepare(&format!(
        "SELECT event_id, ts, seq, event_type, payload, actor FROM {table} ORDER BY seq DESC, event_id DESC LIMIT ?1"
    ))?;
    let rows = stmt.query_map([sql_limit], |row| {
        let payload: String = row.get(4)?;
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, u64>(2)?,
            row.get::<_, String>(3)?,
            payload,
            row.get::<_, String>(5)?,
        ))
    })?;
    let mut events = Vec::new();
    for row in rows {
        let (event_id, ts, seq, event_type, payload, actor) = row?;
        let payload = serde_json::from_str(&payload).map_err(|err| {
            error::DecapodError::ValidationError(format!(
                "invalid canonical event payload in {table} for {event_id}: {err}"
            ))
        })?;
        events.push(StoredEvent {
            stream: stream.to_string(),
            event_id,
            ts,
            seq,
            event_type,
            payload,
            actor,
        });
    }
    Ok(events)
}

pub fn latest(root: &Path, stream: &str) -> Result<Option<StoredEvent>, error::DecapodError> {
    Ok(query(root, stream, 1)?.into_iter().next())
}

pub fn exists(root: &Path, stream: &str) -> Result<bool, error::DecapodError> {
    Ok(latest(root, stream)?.is_some())
}

pub fn actors(root: &Path, stream: &str) -> Result<Vec<String>, error::DecapodError> {
    let mut actors = query(root, stream, usize::MAX)?
        .into_iter()
        .map(|event| event.actor)
        .collect::<Vec<_>>();
    actors.sort();
    actors.dedup();
    Ok(actors)
}

/// Line-oriented compatibility representation derived exclusively from the
/// canonical SQLite store. This does not read or write a JSONL artifact.
pub fn query_serialized_lines(root: &Path, stream: &str) -> Result<String, error::DecapodError> {
    let mut records = query(root, stream, usize::MAX)?;
    records.sort_by_key(|event| event.seq);
    let mut out = String::new();
    for event in records {
        out.push_str(&serde_json::to_string(&event.payload).map_err(|err| {
            error::DecapodError::ValidationError(format!(
                "failed to serialize canonical {stream} event {}: {err}",
                event.event_id
            ))
        })?);
        out.push('\n');
    }
    Ok(out)
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
         );
         CREATE TABLE IF NOT EXISTS legacy_event_imports (
             filename TEXT PRIMARY KEY,
             content_hash TEXT NOT NULL,
             record_count INTEGER NOT NULL,
             imported_at TEXT NOT NULL
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
    let mut imported = 0;
    for &(filename, stream) in LEGACY_FILES {
        let path = data_root.join(filename);
        if !path.is_file() {
            continue;
        }
        let already_imported: bool = conn.query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM legacy_event_imports
                 WHERE filename = ?1
             )",
            [filename],
            |row| row.get(0),
        )?;
        if already_imported {
            continue;
        }
        let content_hash = file_content_hash(&path)?;
        let file = fs::File::open(&path).map_err(error::DecapodError::IoError)?;
        let mut record_count = 0usize;
        for (line_no, line) in BufReader::new(file).lines().enumerate() {
            let line = line.map_err(error::DecapodError::IoError)?;
            if line.trim().is_empty() {
                continue;
            }
            record_count += 1;
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
            let event_id = value.get("event_id").and_then(Value::as_str).unwrap();
            if let Some(existing) = load_canonical_event_shape(conn, stream, event_id)? {
                let matches = canonical_event_matches(stream, &existing, &value)?;
                if !matches {
                    return Err(error::DecapodError::ValidationError(format!(
                        "LEGACY_EVENT_CONFLICT: {}:{} event_id '{}' differs from the canonical {stream} event",
                        path.display(),
                        line_no + 1,
                        event_id
                    )));
                }
                continue;
            }
            append_on_conn(conn, stream, &value)?;
            imported += 1;
        }
        conn.execute(
            "INSERT INTO legacy_event_imports(filename, content_hash, record_count, imported_at)
             VALUES(?1, ?2, ?3, ?4)
             ON CONFLICT(filename) DO UPDATE SET
                 content_hash = excluded.content_hash,
                 record_count = excluded.record_count,
                 imported_at = excluded.imported_at",
            params![
                filename,
                content_hash,
                i64::try_from(record_count).unwrap_or(i64::MAX),
                crate::core::time::now_epoch_z()
            ],
        )?;
    }
    Ok(imported)
}

/// A successful single-datastore migration already proved that its legacy
/// inputs were parsed and copied. Record that durable proof so later versions
/// never reinterpret those archives as live authority.
pub fn mark_previously_consolidated_legacy_inputs(
    data_root: &Path,
    conn: &Connection,
    migration_id: &str,
) -> Result<usize, error::DecapodError> {
    ensure_tables(conn)?;
    let mut marked = 0;
    for &(filename, _) in LEGACY_FILES {
        if !data_root.join(filename).is_file() {
            continue;
        }
        marked += conn.execute(
            "INSERT OR IGNORE INTO legacy_event_imports(
                 filename, content_hash, record_count, imported_at
             ) VALUES(?1, ?2, -1, ?3)",
            params![
                filename,
                format!("proven-by:{migration_id}"),
                crate::core::time::now_epoch_z()
            ],
        )?;
    }
    Ok(marked)
}

fn file_content_hash(path: &Path) -> Result<String, error::DecapodError> {
    let mut file = fs::File::open(path).map_err(error::DecapodError::IoError)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(error::DecapodError::IoError)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

#[derive(Debug)]
struct CanonicalEventShape {
    ts: String,
    event_type: String,
    payload: Value,
    actor: String,
    node_id: Option<String>,
}

fn load_canonical_event_shape(
    conn: &Connection,
    stream: &str,
    event_id: &str,
) -> Result<Option<CanonicalEventShape>, error::DecapodError> {
    let table = table_for_stream(stream).unwrap();
    let node_column = if stream == FEDERATION {
        "node_id"
    } else {
        "NULL"
    };
    let existing = conn
        .query_row(
            &format!(
                "SELECT ts, event_type, payload, actor, {node_column} FROM {table} WHERE event_id = ?1"
            ),
            [event_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            },
        )
        .optional()?;
    let Some((ts, event_type, payload, actor, node_id)) = existing else {
        return Ok(None);
    };
    let payload = serde_json::from_str(&payload).map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "invalid canonical {stream} event payload for '{event_id}': {e}"
        ))
    })?;
    Ok(Some(CanonicalEventShape {
        ts,
        event_type,
        payload,
        actor,
        node_id,
    }))
}

fn canonical_event_matches(
    stream: &str,
    existing: &CanonicalEventShape,
    incoming: &Value,
) -> Result<bool, error::DecapodError> {
    if existing.payload == *incoming {
        return Ok(true);
    }

    // Some pre-boundary stream writers stored the domain payload separately
    // from the common event envelope columns. Compare that representation
    // semantically so an upgrade does not mistake a storage-shape change for
    // a conflicting event identity.
    let incoming_payload = incoming.get("payload");
    let incoming_ts = incoming
        .get("ts")
        .and_then(Value::as_str)
        .unwrap_or("1970-01-01T00:00:00Z");
    let incoming_event_type = incoming
        .get("event_type")
        .or_else(|| incoming.get("op"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let incoming_actor = incoming
        .get("actor")
        .or_else(|| incoming.get("actor_id"))
        .and_then(Value::as_str)
        .unwrap_or("decapod");
    let envelope_matches = incoming_payload == Some(&existing.payload)
        && incoming_ts == existing.ts
        && incoming_event_type == existing.event_type
        && incoming_actor == existing.actor;
    if !envelope_matches || stream != FEDERATION {
        return Ok(envelope_matches);
    }
    Ok(incoming.get("node_id").and_then(Value::as_str) == existing.node_id.as_deref())
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

    #[test]
    fn canonical_query_boundary_reads_appended_events() {
        let dir = tempdir().unwrap();
        let event = serde_json::json!({
            "event_id": "watcher-1",
            "ts": "2026-01-01T00:00:00Z",
            "event_type": "watcher.run",
            "actor": "agent-a"
        });
        append(dir.path(), WATCHER, &event).unwrap();
        assert!(exists(dir.path(), WATCHER).unwrap());
        let latest = latest(dir.path(), WATCHER).unwrap().unwrap();
        assert_eq!(latest.event_id, "watcher-1");
        assert_eq!(latest.payload, event);
        assert_eq!(actors(dir.path(), WATCHER).unwrap(), vec!["agent-a"]);
    }

    #[test]
    fn migrated_watcher_events_remain_observable_without_legacy_jsonl() {
        let dir = tempdir().unwrap();
        let legacy_path = dir.path().join("watcher.events.jsonl");
        fs::write(
            &legacy_path,
            "{\"event_id\":\"legacy-watch-1\",\"ts\":\"1785620000Z\",\"event_type\":\"watcher.run\",\"actor\":\"watcher\"}\n",
        )
        .unwrap();
        let conn = Connection::open(dir.path().join("decapod.db")).unwrap();
        assert_eq!(import_legacy_jsonl(dir.path(), &conn).unwrap(), 1);
        drop(conn);

        fs::remove_file(legacy_path).unwrap();
        let event = latest(dir.path(), WATCHER).unwrap().unwrap();
        assert_eq!(event.event_id, "legacy-watch-1");
        assert_eq!(event.event_type, "watcher.run");
    }

    #[test]
    fn legacy_import_fails_visibly_on_conflicting_event_identity() {
        let dir = tempdir().unwrap();
        let conn = Connection::open(dir.path().join("decapod.db")).unwrap();
        ensure_tables(&conn).unwrap();
        append_on_conn(
            &conn,
            WATCHER,
            &serde_json::json!({
                "event_id": "conflict-1",
                "ts": "1785620000Z",
                "event_type": "watcher.run",
                "actor": "watcher",
                "status": "canonical"
            }),
        )
        .unwrap();
        fs::write(
            dir.path().join("watcher.events.jsonl"),
            "{\"event_id\":\"conflict-1\",\"ts\":\"1785620000Z\",\"event_type\":\"watcher.run\",\"actor\":\"watcher\",\"status\":\"legacy\"}\n",
        )
        .unwrap();

        let error = import_legacy_jsonl(dir.path(), &conn).expect_err("conflict must fail");
        assert!(error.to_string().contains("LEGACY_EVENT_CONFLICT"));
    }

    #[test]
    fn legacy_import_accepts_equivalent_split_envelope_storage() {
        let dir = tempdir().unwrap();
        let conn = Connection::open(dir.path().join("decapod.db")).unwrap();
        ensure_tables(&conn).unwrap();
        conn.execute(
            "INSERT INTO federation_events(event_id, ts, event_type, node_id, payload, actor) VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                "federation-equivalent",
                "2026-08-01T00:00:00Z",
                "node.create",
                "node-1",
                "{\"title\":\"Equivalent\"}",
                "agent"
            ],
        )
        .unwrap();
        fs::write(
            dir.path().join(schemas::FEDERATION_EVENTS_NAME),
            "{\"event_id\":\"federation-equivalent\",\"ts\":\"2026-08-01T00:00:00Z\",\"event_type\":\"node.create\",\"node_id\":\"node-1\",\"payload\":{\"title\":\"Equivalent\"},\"actor\":\"agent\"}\n",
        )
        .unwrap();
        assert_eq!(import_legacy_jsonl(dir.path(), &conn).unwrap(), 0);
    }

    #[test]
    fn proven_consolidation_retires_legacy_files_as_runtime_inputs() {
        let dir = tempdir().unwrap();
        let legacy_path = dir.path().join("watcher.events.jsonl");
        fs::write(
            &legacy_path,
            "{\"event_id\":\"retired-watch\",\"event_type\":\"watcher.run\"}\n",
        )
        .unwrap();
        let conn = Connection::open(dir.path().join("decapod.db")).unwrap();
        ensure_tables(&conn).unwrap();
        append_on_conn(
            &conn,
            WATCHER,
            &serde_json::json!({
                "event_id": "canonical-watch",
                "event_type": "watcher.run"
            }),
        )
        .unwrap();

        assert_eq!(
            mark_previously_consolidated_legacy_inputs(
                dir.path(),
                &conn,
                "db.consolidate.single_datastore.v001"
            )
            .unwrap(),
            1
        );
        fs::write(&legacy_path, "not live evidence anymore\n").unwrap();
        assert_eq!(import_legacy_jsonl(dir.path(), &conn).unwrap(), 0);
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM watcher_events", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
}
