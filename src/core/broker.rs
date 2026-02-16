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
use crate::plugins::policy;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
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

/// Audit event for a brokered database operation.
///
/// Every call to `DbBroker::with_conn` generates a `BrokerEvent` that is
/// appended to `broker.events.jsonl` for full mutation audit trail.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BrokerEvent {
    /// ISO 8601 timestamp (seconds since epoch + 'Z')
    pub ts: String,
    /// Unique event identifier (ULID)
    pub event_id: String,
    /// Actor who initiated the operation (e.g., "cli", "agent", "watcher")
    pub actor: String,
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
        use std::time::{SystemTime, UNIX_EPOCH};

        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let ts = format!("{}Z", secs);

        let ev = BrokerEvent {
            ts,
            event_id: Ulid::new().to_string(),
            actor: actor.to_string(),
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

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "broker",
        "version": "0.1.0",
        "description": "State mutation broker (The Thin Waist)",
        "commands": [
            { "name": "audit", "description": "Show the mutation audit log" }
        ],
        "storage": ["broker.events.jsonl"]
    })
}
