//! Lossless Context Management (LCM) — immutable originals ledger + deterministic summary DAG.
//!
//! LCM stores verbatim interaction originals in an append-only JSONL ledger
//! (`lcm.events.jsonl`) and maintains a derived SQLite index (`lcm.db`) that
//! is always rebuildable from the ledger. Summaries reference originals by
//! content hash, forming a deterministic DAG.

use crate::core::broker::DbBroker;
use crate::core::error;
use crate::core::schemas;
use crate::core::store::Store;
use clap::Subcommand;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use ulid::Ulid;

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

fn lcm_db_path(root: &Path) -> PathBuf {
    root.join(schemas::LCM_DB_NAME)
}

fn lcm_events_path(root: &Path) -> PathBuf {
    root.join(schemas::LCM_EVENTS_NAME)
}

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

pub fn initialize_lcm_db(root: &Path) -> Result<(), error::DecapodError> {
    fs::create_dir_all(root).map_err(error::DecapodError::IoError)?;
    let broker = DbBroker::new(root);
    let db_path = lcm_db_path(root);
    broker.with_conn(&db_path, "decapod", None, "lcm.init", |conn| {
        conn.execute(schemas::LCM_DB_SCHEMA_META, [])
            .map_err(error::DecapodError::RusqliteError)?;
        conn.execute(schemas::LCM_DB_SCHEMA_ORIGINALS_INDEX, [])
            .map_err(error::DecapodError::RusqliteError)?;
        conn.execute(schemas::LCM_DB_SCHEMA_SUMMARIES, [])
            .map_err(error::DecapodError::RusqliteError)?;
        conn.execute(schemas::LCM_DB_INDEX_ORIGINALS_KIND, [])
            .map_err(error::DecapodError::RusqliteError)?;
        conn.execute(schemas::LCM_DB_INDEX_ORIGINALS_TS, [])
            .map_err(error::DecapodError::RusqliteError)?;
        conn.execute(schemas::LCM_DB_INDEX_SUMMARIES_SCOPE, [])
            .map_err(error::DecapodError::RusqliteError)?;
        Ok(())
    })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LcmEvent {
    pub event_id: String,
    pub ts: String,
    pub actor: String,
    pub content_hash: String,
    pub kind: String,
    pub content: String,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OriginalEntry {
    pub content_hash: String,
    pub event_id: String,
    pub ts: String,
    pub actor: String,
    pub kind: String,
    pub byte_size: i64,
    pub session_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SummaryEntry {
    pub summary_hash: String,
    pub ts: String,
    pub scope: String,
    pub original_hashes: Vec<String>,
    pub summary_text: String,
    pub token_estimate: i64,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn now_iso() -> String {
    crate::core::time::now_epoch_z()
}

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

fn append_jsonl(path: &Path, value: &serde_json::Value) -> Result<(), error::DecapodError> {
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(error::DecapodError::IoError)?;
    writeln!(f, "{}", serde_json::to_string(value).unwrap())
        .map_err(error::DecapodError::IoError)?;
    Ok(())
}

fn read_all_events(root: &Path) -> Result<Vec<LcmEvent>, error::DecapodError> {
    let path = lcm_events_path(root);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = fs::File::open(&path).map_err(error::DecapodError::IoError)?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(error::DecapodError::IoError)?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let event: LcmEvent = serde_json::from_str(trimmed)
            .map_err(|e| error::DecapodError::ValidationError(e.to_string()))?;
        events.push(event);
    }
    Ok(events)
}

/// Rough token estimate: ~4 chars per token.
fn estimate_tokens(text: &str) -> i64 {
    (text.len() as i64 + 3) / 4
}

// ---------------------------------------------------------------------------
// Core operations
// ---------------------------------------------------------------------------

/// Ingest an immutable original into the append-only ledger + index.
pub fn ingest(
    store: &Store,
    content: &str,
    kind: &str,
    actor: &str,
    session_id: Option<&str>,
    source: Option<&str>,
) -> Result<serde_json::Value, error::DecapodError> {
    let valid_kinds = ["event", "message", "artifact", "tool_result"];
    if !valid_kinds.contains(&kind) {
        return Err(error::DecapodError::ValidationError(format!(
            "Invalid kind '{}'; must be one of: {}",
            kind,
            valid_kinds.join(", ")
        )));
    }

    let content_hash = sha256_hex(content.as_bytes());
    let event_id = Ulid::new().to_string();
    let ts = now_iso();
    let byte_size = content.len() as i64;

    let mut metadata = serde_json::Map::new();
    if let Some(sid) = session_id {
        metadata.insert(
            "session_id".to_string(),
            serde_json::Value::String(sid.to_string()),
        );
    }
    if let Some(src) = source {
        metadata.insert(
            "source".to_string(),
            serde_json::Value::String(src.to_string()),
        );
    }

    let event = serde_json::json!({
        "event_id": event_id,
        "ts": ts,
        "actor": actor,
        "content_hash": content_hash,
        "kind": kind,
        "content": content,
        "metadata": metadata,
    });

    // 1. Append to immutable ledger
    append_jsonl(&lcm_events_path(&store.root), &event)?;

    // 2. Update derived index
    let broker = DbBroker::new(&store.root);
    let db_path = lcm_db_path(&store.root);
    broker.with_conn(&db_path, "decapod", None, "lcm.ingest", |conn| {
        conn.execute(
            "INSERT OR IGNORE INTO originals_index(content_hash, event_id, ts, actor, kind, byte_size, session_id) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![content_hash, event_id, ts, actor, kind, byte_size, session_id.unwrap_or("")],
        ).map_err(error::DecapodError::RusqliteError)?;
        Ok(())
    })?;

    Ok(serde_json::json!({
        "content_hash": content_hash,
        "event_id": event_id,
        "ts": ts,
        "byte_size": byte_size,
    }))
}

/// List stored originals (metadata only).
pub fn list_originals(
    store: &Store,
    kind_filter: Option<&str>,
    last_n: Option<usize>,
) -> Result<Vec<OriginalEntry>, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = lcm_db_path(&store.root);

    broker.with_conn(&db_path, "decapod", None, "lcm.list", |conn| {
        let (sql, bind_values): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match kind_filter {
            Some(k) => {
                let mut sql = "SELECT content_hash, event_id, ts, actor, kind, byte_size, session_id FROM originals_index WHERE kind = ?1 ORDER BY ts DESC".to_string();
                if let Some(n) = last_n {
                    sql.push_str(&format!(" LIMIT {}", n));
                }
                (sql, vec![Box::new(k.to_string())])
            }
            None => {
                let mut sql = "SELECT content_hash, event_id, ts, actor, kind, byte_size, session_id FROM originals_index ORDER BY ts DESC".to_string();
                if let Some(n) = last_n {
                    sql.push_str(&format!(" LIMIT {}", n));
                }
                (sql, vec![])
            }
        };

        let mut stmt = conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> = bind_values.iter().map(|b| b.as_ref()).collect();
        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            let sid: String = row.get(6)?;
            Ok(OriginalEntry {
                content_hash: row.get(0)?,
                event_id: row.get(1)?,
                ts: row.get(2)?,
                actor: row.get(3)?,
                kind: row.get(4)?,
                byte_size: row.get(5)?,
                session_id: if sid.is_empty() { None } else { Some(sid) },
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(error::DecapodError::RusqliteError)?);
        }
        Ok(out)
    })
}

/// Retrieve an original by content hash from the ledger.
pub fn show_original(
    store: &Store,
    content_hash: &str,
) -> Result<Option<LcmEvent>, error::DecapodError> {
    let events = read_all_events(&store.root)?;
    Ok(events.into_iter().find(|e| e.content_hash == content_hash))
}

/// Produce a deterministic summary from originals.
pub fn summarize(store: &Store, scope: &str) -> Result<serde_json::Value, error::DecapodError> {
    let events = read_all_events(&store.root)?;
    if events.is_empty() {
        return Ok(serde_json::json!({
            "summary_hash": null,
            "message": "No originals to summarize",
        }));
    }

    // Sort by timestamp (already chronological in append-only ledger, but ensure)
    let mut sorted = events.clone();
    sorted.sort_by(|a, b| a.ts.cmp(&b.ts));

    // Filter by scope if "session" — use the most recent session_id
    let filtered: Vec<&LcmEvent> = if scope == "session" {
        // Find most recent session_id
        let last_session = sorted.iter().rev().find_map(|e| {
            e.metadata
                .get("session_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        });
        match last_session {
            Some(sid) => sorted
                .iter()
                .filter(|e| {
                    e.metadata
                        .get("session_id")
                        .and_then(|v| v.as_str())
                        .map(|s| s == sid)
                        .unwrap_or(false)
                })
                .collect(),
            None => sorted.iter().collect(),
        }
    } else {
        sorted.iter().collect()
    };

    // Concatenate originals in timestamp order
    let original_hashes: Vec<String> = filtered.iter().map(|e| e.content_hash.clone()).collect();
    let concatenated: String = filtered
        .iter()
        .map(|e| format!("[{}:{}] {}", e.kind, e.ts, e.content))
        .collect::<Vec<_>>()
        .join("\n");

    // Truncate to budget (64KB for summary text)
    let budget = 64 * 1024;
    let summary_text = if concatenated.len() > budget {
        concatenated[..budget].to_string()
    } else {
        concatenated
    };

    // Deterministic hash: hash the concatenation of original hashes + summary text
    let hash_input = format!("{}|{}", original_hashes.join(","), summary_text);
    let summary_hash = sha256_hex(hash_input.as_bytes());
    let ts = now_iso();
    let token_estimate = estimate_tokens(&summary_text);

    // Store in DB
    let broker = DbBroker::new(&store.root);
    let db_path = lcm_db_path(&store.root);
    let hashes_json = serde_json::to_string(&original_hashes).unwrap();

    broker.with_conn(&db_path, "decapod", None, "lcm.summarize", |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO summaries(summary_hash, ts, scope, original_hashes, summary_text, token_estimate) VALUES(?1,?2,?3,?4,?5,?6)",
            params![summary_hash, ts, scope, hashes_json, summary_text, token_estimate],
        ).map_err(error::DecapodError::RusqliteError)?;
        Ok(())
    })?;

    Ok(serde_json::json!({
        "summary_hash": summary_hash,
        "scope": scope,
        "original_count": original_hashes.len(),
        "token_estimate": token_estimate,
        "ts": ts,
    }))
}

/// Rebuild LCM index from events ledger (validates integrity).
pub fn rebuild_index(
    store: &Store,
    validate: bool,
) -> Result<serde_json::Value, error::DecapodError> {
    let events_path = lcm_events_path(&store.root);
    if !events_path.exists() {
        return Ok(serde_json::json!({
            "status": "skipped",
            "reason": "No events ledger found",
        }));
    }

    let file = fs::File::open(&events_path).map_err(error::DecapodError::IoError)?;
    let reader = BufReader::new(file);

    let broker = DbBroker::new(&store.root);
    let db_path = lcm_db_path(&store.root);

    let mut validated_count = 0;
    let mut error_count = 0;
    let mut errors: Vec<String> = vec![];

    broker.with_conn(&db_path, "decapod", None, "lcm.rebuild", |conn| {
        conn.execute("DELETE FROM originals_index", [])?;
        Ok(())
    })?;

    for line in reader.lines() {
        let line = line.map_err(error::DecapodError::IoError)?;
        if line.trim().is_empty() {
            continue;
        }

        let event: LcmEvent = serde_json::from_str(&line).map_err(|e| {
            error::DecapodError::ValidationError(format!("Failed to parse event: {}", e))
        })?;

        if validate {
            let computed_hash = sha256_hex(event.content.as_bytes());
            if computed_hash != event.content_hash {
                error_count += 1;
                errors.push(format!(
                    "Hash mismatch for event {}: expected {}, got {}",
                    event.event_id, event.content_hash, computed_hash
                ));
                continue;
            }
            validated_count += 1;
        }

        broker.with_conn(&db_path, "decapod", None, "lcm.rebuild.insert", |conn| {
            conn.execute(
                "INSERT INTO originals_index (content_hash, event_id, ts, actor, kind, byte_size, session_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    event.content_hash,
                    event.event_id,
                    event.ts,
                    event.actor,
                    event.kind,
                    event.content.len() as i64,
                    event.metadata.get("session_id").and_then(|v| v.as_str()),
                ],
            )?;
            Ok(())
        })?;
    }

    Ok(serde_json::json!({
        "status": if error_count > 0 { "failed" } else { "success" },
        "validated_count": validated_count,
        "error_count": error_count,
        "errors": errors,
    }))
}

/// Show a summary by hash, or the latest if no hash is given.
pub fn show_summary(
    store: &Store,
    summary_hash: Option<&str>,
) -> Result<Option<SummaryEntry>, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = lcm_db_path(&store.root);

    broker.with_conn(&db_path, "decapod", None, "lcm.summary.show", |conn| {
        let (sql, bind_values): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match summary_hash {
            Some(h) => (
                "SELECT summary_hash, ts, scope, original_hashes, summary_text, token_estimate FROM summaries WHERE summary_hash = ?1".to_string(),
                vec![Box::new(h.to_string())],
            ),
            None => (
                "SELECT summary_hash, ts, scope, original_hashes, summary_text, token_estimate FROM summaries ORDER BY ts DESC LIMIT 1".to_string(),
                vec![],
            ),
        };

        let mut stmt = conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> = bind_values.iter().map(|b| b.as_ref()).collect();
        let mut rows = stmt.query(params_refs.as_slice())?;
        if let Some(row) = rows.next()? {
            let hashes_json: String = row.get(3)?;
            let original_hashes: Vec<String> = serde_json::from_str(&hashes_json)
                .unwrap_or_default();
            Ok(Some(SummaryEntry {
                summary_hash: row.get(0)?,
                ts: row.get(1)?,
                scope: row.get(2)?,
                original_hashes,
                summary_text: row.get(4)?,
                token_estimate: row.get(5)?,
            }))
        } else {
            Ok(None)
        }
    })
}

/// Rebuild the originals_index from the JSONL ledger.
pub fn rebuild_index_from_ledger(store: &Store) -> Result<usize, error::DecapodError> {
    let events = read_all_events(&store.root)?;
    let broker = DbBroker::new(&store.root);
    let db_path = lcm_db_path(&store.root);

    broker.with_conn(&db_path, "decapod", None, "lcm.rebuild", |conn| {
        conn.execute("DELETE FROM originals_index", [])
            .map_err(error::DecapodError::RusqliteError)?;
        let mut count = 0usize;
        for event in &events {
            let session_id = event
                .metadata
                .get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            conn.execute(
                "INSERT OR IGNORE INTO originals_index(content_hash, event_id, ts, actor, kind, byte_size, session_id) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![
                    event.content_hash,
                    event.event_id,
                    event.ts,
                    event.actor,
                    event.kind,
                    event.content.len() as i64,
                    session_id,
                ],
            ).map_err(error::DecapodError::RusqliteError)?;
            count += 1;
        }
        Ok(count)
    })
}

/// Validate the integrity of the LCM ledger.
/// Returns a list of failures (empty = valid).
pub fn validate_ledger_integrity(root: &Path) -> Result<Vec<String>, error::DecapodError> {
    let events = read_all_events(root)?;
    let mut failures = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();
    let mut prev_ts: Option<String> = None;

    for (i, event) in events.iter().enumerate() {
        // Check content hash
        let expected = sha256_hex(event.content.as_bytes());
        if event.content_hash != expected {
            failures.push(format!(
                "Line {}: content_hash mismatch (expected={}, got={})",
                i + 1,
                expected,
                event.content_hash
            ));
        }

        // Check duplicate event_ids
        if !seen_ids.insert(&event.event_id) {
            failures.push(format!(
                "Line {}: duplicate event_id '{}'",
                i + 1,
                event.event_id
            ));
        }

        // Check monotonic timestamps
        if let Some(ref prev) = prev_ts
            && event.ts < *prev {
                failures.push(format!(
                    "Line {}: non-monotonic timestamp (prev={}, current={})",
                    i + 1,
                    prev,
                    event.ts
                ));
            }
        prev_ts = Some(event.ts.clone());
    }

    Ok(failures)
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(clap::Args, Debug)]
pub struct LcmCli {
    #[clap(subcommand)]
    pub command: LcmCommand,
}

#[derive(Subcommand, Debug)]
pub enum LcmCommand {
    /// Store an immutable original in the append-only ledger
    Ingest {
        /// Source path (reads from stdin if not provided)
        #[clap(long)]
        source: Option<PathBuf>,
        /// Kind of content: event, message, artifact, tool_result
        #[clap(long)]
        kind: String,
        /// Actor identifier
        #[clap(long, default_value = "decapod")]
        actor: String,
        /// Session identifier
        #[clap(long)]
        session_id: Option<String>,
    },
    /// List stored originals (metadata only)
    List {
        /// Filter by kind
        #[clap(long)]
        kind: Option<String>,
        /// Show only last N entries
        #[clap(long)]
        last: Option<usize>,
    },
    /// Retrieve an original by content hash
    Show {
        /// Content hash of the original
        #[clap(long)]
        id: String,
    },
    /// Produce a deterministic summary from originals
    Summarize {
        /// Scope: 'session' or 'all'
        #[clap(long, default_value = "all")]
        scope: String,
    },
    /// Show a summary with pointers to originals
    #[clap(name = "summary")]
    SummaryShow {
        /// Summary hash (shows latest if not provided)
        #[clap(long)]
        id: Option<String>,
    },
    /// Rebuild LCM index from events ledger (validates integrity)
    Rebuild {
        /// Validate content hashes during rebuild
        #[clap(long)]
        validate: bool,
    },
    /// Emit subsystem schema JSON
    Schema,
}

pub fn run_lcm_cli(store: &Store, cli: LcmCli) -> Result<(), error::DecapodError> {
    match cli.command {
        LcmCommand::Ingest {
            source,
            kind,
            actor,
            session_id,
        } => {
            let source_str = source.as_ref().map(|p| p.to_string_lossy().to_string());
            let content = match source {
                Some(path) => fs::read_to_string(&path).map_err(error::DecapodError::IoError)?,
                None => {
                    let mut buf = String::new();
                    std::io::stdin()
                        .read_line(&mut buf)
                        .map_err(error::DecapodError::IoError)?;
                    buf
                }
            };
            let result = ingest(
                store,
                &content,
                &kind,
                &actor,
                session_id.as_deref(),
                source_str.as_deref(),
            )?;
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        LcmCommand::List { kind, last } => {
            let entries = list_originals(store, kind.as_deref(), last)?;
            println!("{}", serde_json::to_string_pretty(&entries).unwrap());
        }
        LcmCommand::Show { id } => {
            let event = show_original(store, &id)?;
            match event {
                Some(e) => println!("{}", serde_json::to_string_pretty(&e).unwrap()),
                None => println!("{{\"error\": \"not found\"}}"),
            }
        }
        LcmCommand::Summarize { scope } => {
            let result = summarize(store, &scope)?;
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        LcmCommand::SummaryShow { id } => {
            let entry = show_summary(store, id.as_deref())?;
            match entry {
                Some(e) => println!("{}", serde_json::to_string_pretty(&e).unwrap()),
                None => println!("{{\"error\": \"no summary found\"}}"),
            }
        }
        LcmCommand::Rebuild { validate } => {
            let result = rebuild_index(store, validate)?;
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        LcmCommand::Schema => {
            println!("{}", serde_json::to_string_pretty(&schema()).unwrap());
        }
    }
    Ok(())
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "lcm",
        "version": "0.1.0",
        "description": "Lossless Context Management — immutable originals + deterministic summaries",
        "commands": [
            { "name": "ingest", "description": "Store an immutable original in the append-only ledger" },
            { "name": "list", "description": "List stored originals (metadata only)" },
            { "name": "show", "description": "Retrieve an original by content hash" },
            { "name": "summarize", "description": "Produce deterministic summary DAG from originals" },
            { "name": "summary", "description": "Show a summary with pointers to originals" },
            { "name": "rebuild", "description": "Rebuild LCM index from events ledger (validates integrity)" },
            { "name": "schema", "description": "Emit subsystem schema JSON" },
        ],
        "storage": [schemas::LCM_DB_NAME, schemas::LCM_EVENTS_NAME],
        "invariants": [
            "Originals are NEVER mutated or deleted (append-only JSONL)",
            "Content hash is SHA256 of raw content bytes — deterministic",
            "Summaries are deterministic: same originals → same summary hash",
            "lcm.db is always rebuildable from lcm.events.jsonl",
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_store() -> (tempfile::TempDir, Store) {
        let tmp = tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        initialize_lcm_db(&root).unwrap();
        let store = Store {
            kind: crate::core::store::StoreKind::Repo,
            root,
        };
        (tmp, store)
    }

    #[test]
    fn test_ingest_produces_correct_hash() {
        let (_tmp, store) = test_store();
        let content = "Hello, world!";
        let result = ingest(&store, content, "message", "test-agent", None, None).unwrap();
        let expected_hash = sha256_hex(content.as_bytes());
        assert_eq!(result["content_hash"].as_str().unwrap(), expected_hash);
    }

    #[test]
    fn test_ingest_rejects_invalid_kind() {
        let (_tmp, store) = test_store();
        let result = ingest(&store, "test", "bogus", "agent", None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_list_returns_ingested() {
        let (_tmp, store) = test_store();
        ingest(&store, "alpha", "message", "agent", None, None).unwrap();
        ingest(&store, "beta", "event", "agent", None, None).unwrap();

        let all = list_originals(&store, None, None).unwrap();
        assert_eq!(all.len(), 2);

        let msgs = list_originals(&store, Some("message"), None).unwrap();
        assert_eq!(msgs.len(), 1);
    }

    #[test]
    fn test_show_original_found() {
        let (_tmp, store) = test_store();
        let result = ingest(&store, "find me", "artifact", "agent", None, None).unwrap();
        let hash = result["content_hash"].as_str().unwrap();
        let event = show_original(&store, hash).unwrap().unwrap();
        assert_eq!(event.content, "find me");
    }

    #[test]
    fn test_validate_catches_tamper() {
        let (_tmp, store) = test_store();
        ingest(&store, "good content", "message", "agent", None, None).unwrap();

        // Tamper with the ledger
        let path = lcm_events_path(&store.root);
        let mut contents = fs::read_to_string(&path).unwrap();
        contents = contents.replace("good content", "bad content");
        fs::write(&path, &contents).unwrap();

        let failures = validate_ledger_integrity(&store.root).unwrap();
        assert!(!failures.is_empty());
    }
}
