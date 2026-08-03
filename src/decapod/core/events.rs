//! Canonical append-only event streams for the local datastore.
//!
//! **Single source of truth:** `.decapod/data/decapod.db` (`events` + projection tables).
//!
//! Historical `*.jsonl` files under `.decapod/data/` are **one-shot migration inputs only**.
//! After a successful import they are moved to `.decapod/data/.retired-jsonl/` and are
//! never read again at runtime. Runtime writers must use [`append`] / [`append_on_conn`].
//!
//! Schema filename constants remain solely so migrations can discover old paths.

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
/// Assurance attestations (formerly `.decapod/managed/assurance_attestations.jsonl`).
pub const ASSURANCE: &str = "assurance";

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
    ASSURANCE,
];

/// Directory under `.decapod/data/` where retired JSONL migration inputs are moved.
pub const RETIRED_JSONL_DIR: &str = ".retired-jsonl";

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

/// Historical on-disk JSONL basenames under `.decapod/data/` (migration discovery only).
pub const LEGACY_JSONL_FILES: &[(&str, &str)] = &[
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

// Keep the private alias so existing call sites in this module stay short.
const LEGACY_FILES: &[(&str, &str)] = LEGACY_JSONL_FILES;

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
    // Federation domain writers and replay store only the inner payload object.
    // Other streams historically persist the full envelope as the payload column.
    let payload_value = domain_payload_for_storage(stream, event, &event_id, event_type)?;
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
            serde_json::to_string(&payload_value).unwrap(),
            actor
        ],
    )?;
    Ok(seq)
}

/// Choose the value written to `events.payload`.
///
/// For the federation stream, JSONL/API envelopes carry domain fields under a
/// nested `payload` object; only that inner object is stored (matching native
/// federation writers). Other streams keep the historical whole-envelope shape.
pub fn domain_payload_for_storage(
    stream: &str,
    event: &Value,
    event_id: &str,
    event_type: &str,
) -> Result<Value, error::DecapodError> {
    if stream != FEDERATION {
        return Ok(event.clone());
    }

    // Federation envelope form: a `payload` key is present. It must be an object;
    // only the inner object is stored.
    if let Some(inner) = event.get("payload") {
        if !inner.is_object() {
            return Err(error::DecapodError::ValidationError(format!(
                "LEGACY_EVENT_PAYLOAD: event '{event_id}' inner payload is not a JSON object"
            )));
        }
        let subject_id = event.get("node_id").and_then(Value::as_str);
        let actor = event
            .get("actor")
            .or_else(|| event.get("actor_id"))
            .and_then(Value::as_str)
            .unwrap_or("decapod");
        let ts = event
            .get("ts")
            .and_then(Value::as_str)
            .unwrap_or("1970-01-01T00:00:00Z");
        // When event_type is also present this is a full envelope; validate it.
        if event.get("event_type").is_some() {
            return normalize_event_payload(event_id, event_type, subject_id, actor, ts, event);
        }
        return Ok(inner.clone());
    }

    // Already a bare domain payload (native writer shape).
    Ok(event.clone())
}

/// True when `value` looks like a federation event envelope with a nested payload object.
pub fn looks_like_event_envelope(value: &Value) -> bool {
    let Some(obj) = value.as_object() else {
        return false;
    };
    obj.contains_key("event_type") && obj.get("payload").map(|p| p.is_object()).unwrap_or(false)
}

/// Normalize a stored or incoming federation payload into the canonical inner
/// domain object. Accepts:
/// - canonical inner payloads (returned as-is)
/// - legacy double-wrapped envelopes (validated then unwrapped)
///
/// Contradictory envelopes fail closed; they are never silently unwrapped.
pub fn normalize_event_payload(
    event_id: &str,
    event_type: &str,
    subject_id: Option<&str>,
    actor: &str,
    ts: &str,
    payload: &Value,
) -> Result<Value, error::DecapodError> {
    if !looks_like_event_envelope(payload) {
        return Ok(payload.clone());
    }
    validate_envelope_against_row(event_id, event_type, subject_id, actor, ts, payload)?;
    let inner = payload.get("payload").cloned().ok_or_else(|| {
        error::DecapodError::ValidationError(format!(
            "LEGACY_EVENT_PAYLOAD: event '{event_id}' envelope missing object payload"
        ))
    })?;
    if !inner.is_object() {
        return Err(error::DecapodError::ValidationError(format!(
            "LEGACY_EVENT_PAYLOAD: event '{event_id}' inner payload is not a JSON object"
        )));
    }
    Ok(inner)
}

fn validate_envelope_against_row(
    event_id: &str,
    event_type: &str,
    subject_id: Option<&str>,
    actor: &str,
    ts: &str,
    envelope: &Value,
) -> Result<(), error::DecapodError> {
    let outer_type = envelope
        .get("event_type")
        .and_then(Value::as_str)
        .unwrap_or("");
    if outer_type != event_type {
        return Err(error::DecapodError::ValidationError(format!(
            "LEGACY_EVENT_PAYLOAD: event '{event_id}' envelope event_type '{outer_type}' does not match row event_type '{event_type}'"
        )));
    }

    if let Some(outer_id) = envelope.get("event_id").and_then(Value::as_str)
        && outer_id != event_id
    {
        return Err(error::DecapodError::ValidationError(format!(
            "LEGACY_EVENT_PAYLOAD: event '{event_id}' envelope event_id '{outer_id}' does not match row identity"
        )));
    }

    if let (Some(row_node), Some(outer_node)) =
        (subject_id, envelope.get("node_id").and_then(Value::as_str))
        && outer_node != row_node
    {
        return Err(error::DecapodError::ValidationError(format!(
            "LEGACY_EVENT_PAYLOAD: event '{event_id}' envelope node_id '{outer_node}' does not match row subject_id '{row_node}'"
        )));
    }

    if let Some(outer_actor) = envelope
        .get("actor")
        .or_else(|| envelope.get("actor_id"))
        .and_then(Value::as_str)
        && outer_actor != actor
    {
        return Err(error::DecapodError::ValidationError(format!(
            "LEGACY_EVENT_PAYLOAD: event '{event_id}' envelope actor '{outer_actor}' does not match row actor '{actor}'"
        )));
    }

    if let Some(outer_ts) = envelope.get("ts").and_then(Value::as_str)
        && outer_ts != ts
    {
        return Err(error::DecapodError::ValidationError(format!(
            "LEGACY_EVENT_PAYLOAD: event '{event_id}' envelope ts does not match row ts"
        )));
    }

    let inner = envelope.get("payload").ok_or_else(|| {
        error::DecapodError::ValidationError(format!(
            "LEGACY_EVENT_PAYLOAD: event '{event_id}' envelope missing payload field"
        ))
    })?;
    if !inner.is_object() {
        return Err(error::DecapodError::ValidationError(format!(
            "LEGACY_EVENT_PAYLOAD: event '{event_id}' inner payload is not a JSON object"
        )));
    }
    Ok(())
}

/// Summary of an unwrap repair pass over federation event rows.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FederationPayloadRepairReport {
    pub candidates: usize,
    pub normalized: usize,
    pub unchanged: usize,
}

/// Idempotent repair: unwrap double-wrapped federation `events.payload` values
/// into the canonical inner domain object. Runs in one transaction; any
/// consistency failure aborts with zero rows changed.
pub fn repair_double_wrapped_federation_payloads(
    conn: &Connection,
) -> Result<FederationPayloadRepairReport, error::DecapodError> {
    ensure_tables(conn)?;
    let table_exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'events')",
        [],
        |row| row.get(0),
    )?;
    if !table_exists {
        return Ok(FederationPayloadRepairReport::default());
    }

    conn.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| {
        let mut stmt = conn.prepare(
            "SELECT event_id, event_type, subject_id, actor, ts, payload
             FROM events
             WHERE stream = 'federation'
             ORDER BY seq, event_id",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut report = FederationPayloadRepairReport::default();
        let mut updates: Vec<(String, String)> = Vec::new();

        for (event_id, event_type, subject_id, actor, ts, payload_raw) in rows {
            let payload: Value = serde_json::from_str(&payload_raw).map_err(|e| {
                error::DecapodError::ValidationError(format!(
                    "LEGACY_EVENT_PAYLOAD: event '{event_id}' has invalid JSON payload: {e}"
                ))
            })?;
            if !looks_like_event_envelope(&payload) {
                report.unchanged += 1;
                continue;
            }
            report.candidates += 1;
            let inner = normalize_event_payload(
                &event_id,
                &event_type,
                subject_id.as_deref(),
                &actor,
                &ts,
                &payload,
            )?;
            let inner_raw = serde_json::to_string(&inner).map_err(|e| {
                error::DecapodError::ValidationError(format!(
                    "LEGACY_EVENT_PAYLOAD: failed to serialize repaired payload for '{event_id}': {e}"
                ))
            })?;
            if inner_raw != payload_raw {
                updates.push((event_id, inner_raw));
                report.normalized += 1;
            } else {
                report.unchanged += 1;
            }
        }

        for (event_id, inner_raw) in updates {
            conn.execute(
                "UPDATE events SET payload = ?1 WHERE stream = 'federation' AND event_id = ?2",
                params![inner_raw, event_id],
            )?;
        }
        Ok(report)
    })();

    match result {
        Ok(report) => {
            conn.execute_batch("COMMIT")?;
            if report.normalized > 0 {
                eprintln!(
                    "federation payload repair: candidates={} normalized={} unchanged={}",
                    report.candidates, report.normalized, report.unchanged
                );
            }
            Ok(report)
        }
        Err(err) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(err)
        }
    }
}

fn event_id(event: &Value) -> String {
    if let Some(id) = event.get("event_id").and_then(Value::as_str) {
        return id.to_string();
    }
    crate::core::ulid::new_ulid()
}

/// Import the legacy JSONL streams into `events`, then retire the files.
///
/// Re-running is safe: already-imported files are skipped and retired if still
/// present. Each file is imported inside one SQLite transaction. A partial
/// parse or validation failure rolls back the file and does not mark it imported.
pub fn import_legacy_jsonl(
    data_root: &Path,
    conn: &Connection,
) -> Result<usize, error::DecapodError> {
    let imported = import_legacy_jsonl_without_retire(data_root, conn)?;
    let _ = retire_imported_legacy_jsonl(data_root, conn)?;
    Ok(imported)
}

/// Import only (no filesystem move). Prefer [`import_legacy_jsonl`].
fn import_legacy_jsonl_without_retire(
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
        let mut file_imported = 0usize;

        conn.execute_batch("BEGIN IMMEDIATE")?;
        let file_result = (|| {
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
                let event_id = value
                    .get("event_id")
                    .and_then(Value::as_str)
                    .unwrap()
                    .to_string();
                // Historical promotions ledgers often omit event_type; stamp one so
                // post-migration validation can still classify them as promotions.
                if filename.contains("promotion")
                    && value.get("event_type").and_then(Value::as_str).is_none()
                    && value.get("op").and_then(Value::as_str).is_none()
                {
                    value["event_type"] = Value::String("knowledge.promotion".to_string());
                }
                let event_type = value
                    .get("event_type")
                    .or_else(|| value.get("op"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                // Fail closed on contradictory federation envelopes before insert.
                if stream == FEDERATION {
                    domain_payload_for_storage(stream, &value, &event_id, event_type).map_err(
                        |e| {
                            error::DecapodError::ValidationError(format!(
                                "{}:{}: {e}",
                                path.display(),
                                line_no + 1
                            ))
                        },
                    )?;
                }
                if let Some(existing) = load_canonical_event_shape(conn, stream, &event_id)? {
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
                file_imported += 1;
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
            Ok(file_imported)
        })();

        match file_result {
            Ok(count) => {
                conn.execute_batch("COMMIT")?;
                imported += count;
            }
            Err(err) => {
                let _ = conn.execute_batch("ROLLBACK");
                return Err(err);
            }
        }
    }
    Ok(imported)
}

/// Move imported legacy JSONL files out of the live data directory.
///
/// Only files already recorded in `legacy_event_imports` are retired. Unimported
/// files are left in place so a later import can still consume them.
pub fn retire_imported_legacy_jsonl(
    data_root: &Path,
    conn: &Connection,
) -> Result<usize, error::DecapodError> {
    ensure_tables(conn)?;
    let archive = data_root.join(RETIRED_JSONL_DIR);
    let mut retired = 0usize;
    for &(filename, _) in LEGACY_FILES {
        let path = data_root.join(filename);
        if !path.is_file() {
            continue;
        }
        let marked: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM legacy_event_imports WHERE filename = ?1)",
            [filename],
            |row| row.get(0),
        )?;
        if !marked {
            continue;
        }
        fs::create_dir_all(&archive).map_err(error::DecapodError::IoError)?;
        let dest_name = filename.replace('/', "__");
        let mut dest = archive.join(&dest_name);
        if dest.exists() {
            // Avoid clobbering a previous retirement; append content hash suffix.
            let hash = file_content_hash(&path).unwrap_or_else(|_| "dup".into());
            dest = archive.join(format!("{dest_name}.{hash}"));
        }
        fs::rename(&path, &dest).map_err(error::DecapodError::IoError)?;
        retired += 1;
        eprintln!(
            "legacy jsonl retired: {} → {}",
            path.display(),
            dest.display()
        );
    }
    Ok(retired)
}

/// List any still-live legacy JSONL basenames under `data_root` (not retired).
pub fn live_legacy_jsonl_files(data_root: &Path) -> Vec<String> {
    LEGACY_FILES
        .iter()
        .filter_map(|(filename, _)| {
            let path = data_root.join(filename);
            if path.is_file() {
                Some((*filename).to_string())
            } else {
                None
            }
        })
        .collect()
}

/// A successful single-datastore migration already proved that its legacy
/// inputs were parsed and copied. Record that durable proof so later versions
/// never reinterpret those archives as live authority, then retire the files.
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
    let _ = retire_imported_legacy_jsonl(data_root, conn)?;
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
