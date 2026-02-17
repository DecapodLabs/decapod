//! Database broker for serialized state access (The Thin Waist).
//!
//! This module provides the core state mutation control plane for Decapod.
//! All agent interactions with state MUST go through the broker to ensure
//! serialization, auditability, and deterministic replay.
//!
//! # For AI Agents
//!
//! - **Never bypass the broker**: Use `decapod` CLI commands, not direct DB access
//! - **All mutations are audited**: Every broker call logs to `broker.events.jsonl`
//! - **Serialization guarantee**: In-process mutex ensures no race conditions
//! - **Intent tracking**: Operations can reference intent IDs for traceability

use crate::core::db;
use crate::core::error;
use crate::core::time;
use crate::plugins::policy;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use ulid::Ulid;

/// Database broker providing serialized access to Decapod state.
///
/// The DbBroker is the "Thin Waist" control plane for all state mutations.
/// In Phase 1 (Epoch 1), it uses an in-process global lock to serialize access.
/// Future phases may support multi-process coordination.
///
/// # Agent Contract
///
/// Agents MUST use the broker for ALL state access. Direct database manipulation
/// bypasses audit trails and violates the control plane contract.
pub struct DbBroker {
    audit_log_path: PathBuf,
}

#[derive(Clone)]
struct CacheEntry {
    value: JsonValue,
    expires_at: Instant,
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
        }
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
        let is_read = policy::is_read_only_operation(op_name);
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

        // Serialize operations per database path instead of globally.
        // This preserves same-DB safety while allowing cross-DB parallelism.
        let db_lock = get_db_lock(db_path)?;
        let _lock = db_lock
            .lock()
            .map_err(|_| error::DecapodError::ValidationError("DbBroker lock poisoned".into()))?;

        let db_id = db_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let conn = db::db_connect(&db_path.to_string_lossy())?;

        let result = f(&conn);

        let status = if result.is_ok() { "success" } else { "error" };
        self.log_event(actor, effective_intent.as_deref(), op_name, &db_id, status)?;

        result
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

        writeln!(f, "{}", serde_json::to_string(&ev).unwrap())
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
        if let Some(entry) = map.get(&compound) {
            if entry.expires_at > Instant::now() {
                return Some(entry.value.clone());
            }
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
}

fn db_lock_map() -> &'static Mutex<HashMap<PathBuf, Arc<Mutex<()>>>> {
    static DB_LOCKS: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> = OnceLock::new();
    DB_LOCKS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get_db_lock(db_path: &Path) -> Result<Arc<Mutex<()>>, error::DecapodError> {
    let key = db_path.to_path_buf();
    let mut map = db_lock_map()
        .lock()
        .map_err(|_| error::DecapodError::ValidationError("Db lock map poisoned".into()))?;
    Ok(map
        .entry(key)
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone())
}

fn get_audit_lock() -> &'static Mutex<()> {
    static AUDIT_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    AUDIT_LOCK.get_or_init(|| Mutex::new(()))
}

fn broker_read_cache() -> &'static Mutex<HashMap<String, CacheEntry>> {
    static READ_CACHE: OnceLock<Mutex<HashMap<String, CacheEntry>>> = OnceLock::new();
    READ_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
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
