//! Canonical append-only event streams for the local datastore.
//!
//! Classification of remaining JSONL references:
//! - **sealed migration input**: filenames listed in `LEGACY_JSONL_STREAMS` (import only)
//! - **serialization-only / test fixtures**: checkout fixtures under `tests/`
//! - **schema name constants**: historical filenames kept for import path matching
//! - **not runtime authority**: after import, `decapod.db` event tables own the stream
//!
//! Runtime event writes go through these tables so the local SQLite shape matches
//! the cloud datastore. JSONL is never a live runtime write target.

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

/// Known event stream names. All streams share the single `events` table (#1127).
pub const STREAMS: &[&str] = &[
    BROKER,
    TODO,
    FEDERATION,
    EXTERNAL_ACTIONS,
    TRACES,
    VERIFICATION,
    WATCHER,
    MAP,
    LCM,
    KNOWLEDGE,
];

/// Physical table name for every stream after consolidation.
pub const EVENTS_TABLE: &str = "events";

/// Historical physical table names used only by migration into `events`.
const LEGACY_STREAM_TABLES: &[(&str, &str)] = &[
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
    if STREAMS.contains(&stream) {
        Some(EVENTS_TABLE)
    } else {
        None
    }
}

pub fn is_known_stream(stream: &str) -> bool {
    STREAMS.contains(&stream)
}

fn subject_for_stream(stream: &str, event: &Value) -> (Option<String>, Option<String>) {
    match stream {
        TODO => {
            let id = event
                .get("task_id")
                .or_else(|| event.pointer("/payload/task_id"))
                .and_then(Value::as_str)
                .map(str::to_string);
            (Some("task".into()), id)
        }
        FEDERATION => {
            let id = event
                .get("node_id")
                .or_else(|| event.pointer("/payload/node_id"))
                .and_then(Value::as_str)
                .map(str::to_string);
            (Some("node".into()), id)
        }
        _ => (None, None),
    }
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
    if !is_known_stream(stream) {
        return Err(error::DecapodError::ValidationError(format!(
            "unknown event stream: {stream}"
        )));
    }
    let path = canonical_db_path(root);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let conn = db::db_connect_for_validate(&path.to_string_lossy())?;
    let table_exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'events')",
        [],
        |row| row.get(0),
    )?;
    if !table_exists {
        return Ok(Vec::new());
    }
    let sql_limit = i64::try_from(limit.max(1)).unwrap_or(i64::MAX);
    let mut stmt = conn.prepare(
        "SELECT event_id, ts, seq, event_type, payload, actor FROM events
         WHERE stream = ?1
         ORDER BY seq DESC, event_id DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![stream, sql_limit], |row| {
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
                "invalid canonical event payload in events for {event_id}: {err}"
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

/// Ensure the unified events table exists and fold historical stream tables when present.
pub fn ensure_tables(conn: &Connection) -> Result<(), error::DecapodError> {
    conn.execute_batch(schemas::EVENTS_TABLE_SCHEMA)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS legacy_event_imports (
             filename TEXT PRIMARY KEY,
             content_hash TEXT NOT NULL,
             record_count INTEGER NOT NULL,
             imported_at TEXT NOT NULL
         );",
    )?;
    migrate_legacy_stream_tables_into_events(conn)?;
    Ok(())
}

fn table_exists_local(conn: &Connection, name: &str) -> Result<bool, error::DecapodError> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        [name],
        |row| row.get(0),
    )?;
    Ok(exists)
}

fn column_exists(
    conn: &Connection,
    table: &str,
    column: &str,
) -> Result<bool, error::DecapodError> {
    let exists: bool = conn.query_row(
        &format!("SELECT EXISTS(SELECT 1 FROM pragma_table_info('{table}') WHERE name = ?1)"),
        [column],
        |row| row.get(0),
    )?;
    Ok(exists)
}

fn migrate_legacy_stream_tables_into_events(conn: &Connection) -> Result<(), error::DecapodError> {
    for &(stream, table) in LEGACY_STREAM_TABLES {
        if !table_exists_local(conn, table)? {
            continue;
        }
        if table == "federation_events" {
            let has_node = column_exists(conn, table, "node_id")?;
            if has_node {
                conn.execute(
                    "INSERT OR IGNORE INTO events(event_id, ts, seq, stream, subject_kind, subject_id, event_type, payload, actor)
                     SELECT event_id, ts, COALESCE(seq, 0), ?1, 'node', node_id, event_type, payload, actor
                     FROM federation_events",
                    [stream],
                )?;
            } else {
                conn.execute(
                    "INSERT OR IGNORE INTO events(event_id, ts, seq, stream, subject_kind, subject_id, event_type, payload, actor)
                     SELECT event_id, ts, COALESCE(seq, 0), ?1, NULL, NULL, event_type, payload, actor
                     FROM federation_events",
                    [stream],
                )?;
            }
        } else {
            let has_seq = column_exists(conn, table, "seq")?;
            if has_seq {
                conn.execute(
                    &format!(
                        "INSERT OR IGNORE INTO events(event_id, ts, seq, stream, subject_kind, subject_id, event_type, payload, actor)
                         SELECT event_id, ts, seq, ?1, NULL, NULL, event_type, payload, actor FROM {table}"
                    ),
                    [stream],
                )?;
            } else {
                conn.execute(
                    &format!(
                        "INSERT OR IGNORE INTO events(event_id, ts, seq, stream, subject_kind, subject_id, event_type, payload, actor)
                         SELECT event_id, ts, 0, ?1, NULL, NULL, event_type, payload, actor FROM {table}"
                    ),
                    [stream],
                )?;
            }
        }
        conn.execute(&format!("DROP TABLE IF EXISTS {table}"), [])?;
    }

    if table_exists_local(conn, "task_events")? {
        let has_seq = column_exists(conn, "task_events", "seq")?;
        if has_seq {
            conn.execute(
                "INSERT OR IGNORE INTO events(event_id, ts, seq, stream, subject_kind, subject_id, event_type, payload, actor)
                 SELECT event_id, ts, COALESCE(seq, 0), 'todo', 'task', task_id, event_type, payload, actor
                 FROM task_events",
                [],
            )?;
        } else {
            conn.execute(
                "INSERT OR IGNORE INTO events(event_id, ts, seq, stream, subject_kind, subject_id, event_type, payload, actor)
                 SELECT event_id, ts, 0, 'todo', 'task', task_id, event_type, payload, actor
                 FROM task_events",
                [],
            )?;
        }
        conn.execute("DROP TABLE IF EXISTS task_events", [])?;
    }

    for stream in STREAMS {
        backfill_stream_sequence(conn, stream)?;
    }
    Ok(())
}

fn backfill_stream_sequence(conn: &Connection, stream: &str) -> Result<(), error::DecapodError> {
    conn.execute(
        "WITH ordered AS (
             SELECT event_id, ROW_NUMBER() OVER (ORDER BY ts, event_id) AS next_seq
             FROM events
             WHERE stream = ?1 AND (seq IS NULL OR seq = 0)
         )
         UPDATE events
         SET seq = (SELECT next_seq FROM ordered WHERE ordered.event_id = events.event_id)
         WHERE event_id IN (SELECT event_id FROM ordered)",
        [stream],
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
    if !is_known_stream(stream) {
        return Err(error::DecapodError::ValidationError(format!(
            "unknown event stream: {stream}"
        )));
    }
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
    let (subject_kind, subject_id) = subject_for_stream(stream, event);
    let seq: u64 = conn.query_row(
        "SELECT COALESCE(MAX(seq), 0) + 1 FROM events WHERE stream = ?1",
        [stream],
        |row| row.get(0),
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO events(event_id, ts, seq, stream, subject_kind, subject_id, event_type, payload, actor)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            event_id,
            ts,
            seq,
            stream,
            subject_kind,
            subject_id,
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
    let existing = conn
        .query_row(
            "SELECT ts, event_type, payload, actor, subject_id
             FROM events WHERE stream = ?1 AND event_id = ?2",
            params![stream, event_id],
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
#[path = "../../../tests/unit/core/events_tests.rs"]
mod tests;
