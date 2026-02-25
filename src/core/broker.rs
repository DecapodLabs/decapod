//! Database broker for serialized state access (The Thin Waist).
//!
//! This module provides the core state mutation control plane for Decapod.
//! Stateful operations route through this layer to ensure
//! serialization, auditability, and deterministic replay.

use crate::core::error;
use crate::core::pool;
use crate::core::time;
use crate::plugins::policy;
use rusqlite::Connection;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use ulid::Ulid;

/// Database broker providing serialized access to Decapod state.
///
/// The DbBroker is the "Thin Waist" control plane for all state mutations.
/// It provides:
/// - Write queue API for serialized mutation pipeline (future: background thread)
/// - Read/write access with proper locking
///
#[allow(dead_code)]
pub struct DbBroker {
    audit_log_path: PathBuf,
    write_queue: Option<Sender<WriteRequest>>,
}

#[derive(Clone)]
struct CacheEntry {
    value: JsonValue,
    expires_at: Instant,
}

/// Write request for the queue - simplified for Send safety
#[allow(dead_code)]
struct WriteRequest {
    db_path: PathBuf,
    sql: String,
    params: Vec<Box<dyn rusqlite::ToSql + Send>>,
    result_tx: std::sync::mpsc::Sender<Result<u64, error::DecapodError>>,
}

/// Audit event for a brokered database operation.
///
/// Every call to `DbBroker::with_conn` generates a `BrokerEvent` that is
/// appended to `broker.events.jsonl` for full mutation audit trail.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BrokerEvent {
    /// Envelope schema version for machine consumers.
    #[serde(default = "default_broker_schema_version")]
    pub schema_version: String,
    /// Request identifier used by orchestrators and adapters.
    #[serde(default)]
    pub request_id: String,
    /// ISO 8601 timestamp (seconds since epoch + 'Z')
    pub ts: String,
    /// Unique event identifier (ULID)
    pub event_id: String,
    /// Actor who initiated the operation (e.g., "cli", "agent", "watcher")
    pub actor: String,
    /// Canonical actor identifier (same as actor for now; explicit for envelope stability).
    #[serde(default)]
    pub actor_id: String,
    /// Optional runtime session identifier for multi-call workflows.
    #[serde(default)]
    pub session_id: Option<String>,
    /// Correlation ID for grouping related operations.
    #[serde(default)]
    pub correlation_id: Option<String>,
    /// Causation ID that links this event to a parent event/request.
    #[serde(default)]
    pub causation_id: Option<String>,
    /// Optional idempotency key set by orchestrator/runtime.
    #[serde(default)]
    pub idempotency_key: Option<String>,
    /// Optional reference to an intent or session ID
    pub intent_ref: Option<String>,
    /// Operation name (e.g., "todo.add", "health.record")
    pub op: String,
    /// Database identifier (file name, e.g., "todo.db")
    pub db_id: String,
    /// Operation status ("success" or "error")
    pub status: String,
}

impl DbBroker {
    pub fn new(root: &Path) -> Self {
        Self {
            audit_log_path: root.join("broker.events.jsonl"),
            write_queue: None, // Future: spawn background thread
        }
    }

    /// Execute a write through the queue (synchronous for now).
    /// In future, this will queue writes to be processed by background thread.
    pub fn execute_write_sync(
        &self,
        db_path: &Path,
        sql: &str,
        params: &[(&str, i64)],
    ) -> Result<u64, error::DecapodError> {
        let audit_path = self.audit_log_path.clone();
        let sql = sql.to_string();
        let params: Vec<i64> = params.iter().map(|(_, v)| *v).collect();
        let db_path_owned = db_path.to_path_buf();

        pool::global_pool().with_write(db_path, |conn| {
            let mut stmt = conn.prepare(&sql)?;
            let param_vec: Vec<Box<dyn rusqlite::ToSql>> = params
                .iter()
                .map(|v| Box::new(*v) as Box<dyn rusqlite::ToSql>)
                .collect();
            let params_refs: Vec<&dyn rusqlite::ToSql> =
                param_vec.iter().map(|p| p.as_ref()).collect();
            stmt.execute(params_refs.as_slice())?;

            let rowid = conn.last_insert_rowid();

            log_write_event(&audit_path, "queued_write", &db_path_owned)?;

            Ok(rowid as u64)
        })
    }

    /// Execute a closure with a serialized connection to the specified DB.
    pub fn with_conn<F, R>(
        &self,
        db_path: &Path,
        actor: &str,
        intent_ref: Option<&str>,
        op_name: &str,
        f: F,
    ) -> Result<R, error::DecapodError>
    where
        F: FnOnce(&Connection) -> Result<R, error::DecapodError>,
    {
        let is_read = policy::is_read_only_operation(op_name) && !op_name.ends_with(".init"); // .init ops do DDL writes; route through write pool
        let effective_intent = if let Some(i) = intent_ref {
            Some(i.to_string())
        } else if !is_read {
            Some(format!("intent:auto:{}:{}", op_name, Ulid::new()))
        } else {
            None
        };

        if !is_read {
            let store_root = self
                .audit_log_path
                .parent()
                .ok_or_else(|| error::DecapodError::PathError("invalid broker root".to_string()))?;
            policy::enforce_broker_mutation_policy(store_root, actor, op_name)?;
        }

        let db_id = db_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        if is_read {
            // Read path: use pooled read connection (no mutex serialization)
            let result = pool::global_pool().with_read(db_path, f);
            let status = if result.is_ok() { "success" } else { "error" };
            self.log_event(actor, effective_intent.as_deref(), op_name, &db_id, status)?;
            result
        } else {
            // Write path: use pooled write connection with two-phase audit logging
            self.log_event(
                actor,
                effective_intent.as_deref(),
                op_name,
                &db_id,
                "pending",
            )?;

            let result = pool::global_pool().with_write(db_path, f);

            let status = if result.is_ok() { "success" } else { "error" };
            self.log_event(actor, effective_intent.as_deref(), op_name, &db_id, status)?;
            result
        }
    }

    fn log_event(
        &self,
        actor: &str,
        intent_ref: Option<&str>,
        op: &str,
        db_id: &str,
        status: &str,
    ) -> Result<(), error::DecapodError> {
        use std::fs::OpenOptions;
        use std::io::Write;
        let ts = time::now_epoch_z();
        let request_id = time::new_event_id();
        let event_id = time::new_event_id();
        let session_id = env::var("DECAPOD_SESSION_ID").ok();
        let correlation_id = env::var("DECAPOD_CORRELATION_ID")
            .ok()
            .or_else(|| intent_ref.map(|s| s.to_string()));
        let causation_id = env::var("DECAPOD_CAUSATION_ID").ok();
        let idempotency_key = env::var("DECAPOD_IDEMPOTENCY_KEY").ok();

        let ev = BrokerEvent {
            schema_version: default_broker_schema_version(),
            request_id,
            ts,
            event_id,
            actor: actor.to_string(),
            actor_id: actor.to_string(),
            session_id,
            correlation_id,
            causation_id,
            idempotency_key,
            intent_ref: intent_ref.map(|s| s.to_string()),
            op: op.to_string(),
            db_id: db_id.to_string(),
            status: status.to_string(),
        };

        let audit_lock = get_audit_lock();
        let _audit_guard = audit_lock
            .lock()
            .map_err(|_| error::DecapodError::ValidationError("Audit lock poisoned".into()))?;

        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.audit_log_path)
            .map_err(error::DecapodError::IoError)?;

        let mut line = serde_json::to_string(&ev).unwrap();
        line.push('\n');
        f.write_all(line.as_bytes())
            .map_err(error::DecapodError::IoError)?;
        Ok(())
    }

    fn cache_compound_key(db_path: &Path, scope: &str, key: &str) -> String {
        format!("{}::{}::{}", db_path.to_string_lossy(), scope, key)
    }

    pub fn cache_get_json(db_path: &Path, scope: &str, key: &str) -> Option<JsonValue> {
        let compound = Self::cache_compound_key(db_path, scope, key);
        let cache = broker_read_cache();
        let mut map = cache.lock().ok()?;
        if let Some(entry) = map.get(&compound)
            && entry.expires_at > Instant::now()
        {
            return Some(entry.value.clone());
        }
        map.remove(&compound);
        None
    }

    pub fn cache_put_json(
        db_path: &Path,
        scope: &str,
        key: &str,
        value: JsonValue,
        ttl_secs: u64,
    ) -> Result<(), error::DecapodError> {
        let compound = Self::cache_compound_key(db_path, scope, key);
        let expires_at = Instant::now()
            .checked_add(Duration::from_secs(ttl_secs.max(1)))
            .unwrap_or_else(Instant::now);
        let mut map = broker_read_cache().lock().map_err(|_| {
            error::DecapodError::ValidationError("broker read cache lock poisoned".to_string())
        })?;
        map.insert(compound, CacheEntry { value, expires_at });
        Ok(())
    }

    pub fn cache_invalidate_scope(db_path: &Path, scope: &str) -> Result<(), error::DecapodError> {
        let prefix = format!("{}::{}::", db_path.to_string_lossy(), scope);
        let mut map = broker_read_cache().lock().map_err(|_| {
            error::DecapodError::ValidationError("broker read cache lock poisoned".to_string())
        })?;
        map.retain(|k, _| !k.starts_with(&prefix));
        Ok(())
    }

    pub fn cache_invalidate_key(
        db_path: &Path,
        scope: &str,
        key: &str,
    ) -> Result<(), error::DecapodError> {
        let compound = Self::cache_compound_key(db_path, scope, key);
        let mut map = broker_read_cache().lock().map_err(|_| {
            error::DecapodError::ValidationError("broker read cache lock poisoned".to_string())
        })?;
        map.remove(&compound);
        Ok(())
    }

    /// Verify log integrity and detect potential crash-induced divergence.
    ///
    /// Scans the audit log for `pending` events that lack a corresponding
    /// terminal `success` or `error` event, which indicates a process crash
    /// between the two-phase commit boundaries.
    pub fn verify_replay(&self) -> Result<ReplayReport, error::DecapodError> {
        use std::io::BufRead;
        if !self.audit_log_path.exists() {
            return Ok(ReplayReport {
                ts: time::now_epoch_z(),
                divergences: vec![],
                total_events: 0,
            });
        }

        let f = std::fs::File::open(&self.audit_log_path).map_err(error::DecapodError::IoError)?;
        let reader = std::io::BufReader::new(f);
        let mut pending_map = FxHashMap::default();
        let mut total_events = 0usize;

        for line in reader.lines() {
            let line = line.map_err(error::DecapodError::IoError)?;
            let ev: BrokerEvent = serde_json::from_str(&line).map_err(|e| {
                error::DecapodError::ValidationError(format!("Invalid audit log entry: {}", e))
            })?;
            total_events += 1;

            match ev.status.as_str() {
                "pending" => {
                    pending_map.insert(ev.event_id.clone(), ev);
                }
                "success" | "error" => {
                    // Match terminal events to pending events by intent_ref + op + db_id.
                    // When both have matching intent_ref (including both None), clear the pending.
                    pending_map.retain(|_, v| {
                        let intent_match = match (&v.intent_ref, &ev.intent_ref) {
                            (Some(a), Some(b)) => a == b,
                            (None, None) => true,
                            _ => false,
                        };
                        !(intent_match && v.op == ev.op && v.db_id == ev.db_id)
                    });
                }
                _ => {}
            }
        }

        let divergences = pending_map
            .into_values()
            .map(|ev| Divergence {
                event_id: ev.event_id,
                op: ev.op,
                db_id: ev.db_id,
                ts: ev.ts,
                intent_ref: ev.intent_ref,
                reason: "Pending event without terminal status (potential crash)".to_string(),
            })
            .collect();

        Ok(ReplayReport {
            ts: time::now_epoch_z(),
            divergences,
            total_events,
        })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ReplayReport {
    pub ts: String,
    pub divergences: Vec<Divergence>,
    pub total_events: usize,
}

fn log_write_event(audit_path: &Path, op: &str, db_path: &Path) -> Result<(), error::DecapodError> {
    use std::fs::OpenOptions;
    use std::io::Write;

    let event = serde_json::json!({
        "op": op,
        "db": db_path.file_name().map(|s| s.to_string_lossy().to_string()),
        "ts": time::now_epoch_z(),
    });

    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(audit_path)
    {
        let _ = writeln!(file, "{}", event);
    }

    Ok(())
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Divergence {
    pub event_id: String,
    pub op: String,
    pub db_id: String,
    pub ts: String,
    pub intent_ref: Option<String>,
    pub reason: String,
}

fn get_audit_lock() -> &'static Mutex<()> {
    static AUDIT_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    AUDIT_LOCK.get_or_init(|| Mutex::new(()))
}

fn broker_read_cache() -> &'static Mutex<FxHashMap<String, CacheEntry>> {
    static READ_CACHE: OnceLock<Mutex<FxHashMap<String, CacheEntry>>> = OnceLock::new();
    READ_CACHE.get_or_init(|| Mutex::new(FxHashMap::default()))
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "broker",
        "version": "0.1.0",
        "description": "State mutation broker (The Thin Waist)",
        "commands": [
            { "name": "audit", "description": "Show the mutation audit log" }
        ],
        "envelope": {
            "schema_version": "1.0.0",
            "fields": [
                "schema_version",
                "request_id",
                "event_id",
                "ts",
                "actor",
                "actor_id",
                "session_id",
                "correlation_id",
                "causation_id",
                "idempotency_key",
                "intent_ref",
                "op",
                "db_id",
                "status"
            ]
        },
        "storage": ["broker.events.jsonl"]
    })
}

fn default_broker_schema_version() -> String {
    "1.0.0".to_string()
}
