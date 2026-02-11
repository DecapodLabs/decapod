use crate::core::db;
use crate::core::error;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use ulid::Ulid;

/// The DB Broker is the "Thin Waist" for state access.
/// In Phase 1 (Epoch 1), it is an in-process serialized request layer.
pub struct DbBroker {
    audit_log_path: PathBuf,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BrokerEvent {
    pub ts: String,
    pub event_id: String,
    pub actor: String,
    pub intent_ref: Option<String>,
    pub op: String,
    pub db_id: String,
    pub status: String,
}

impl DbBroker {
    pub fn new(root: &Path) -> Self {
        Self {
            audit_log_path: root.join("broker.events.jsonl"),
        }
    }

    /// Execute a closure with a serialized connection to the specified DB.
    pub fn with_conn<F, R>(&self, db_path: &Path, actor: &str, intent_ref: Option<&str>, op_name: &str, f: F) -> Result<R, error::DecapodError>
    where
        F: FnOnce(&Connection) -> Result<R, error::DecapodError>,
    {
        // Simple global lock for Phase 1 serialization.
        static DB_LOCK: Mutex<()> = Mutex::new(());
        let _lock = DB_LOCK.lock().unwrap();

        let db_id = db_path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let conn = db::db_connect(&db_path.to_string_lossy())?;
        
        let result = f(&conn);

        let status = if result.is_ok() { "success" } else { "error" };
        self.log_event(actor, intent_ref, op_name, &db_id, status)?;

        result
    }

    fn log_event(&self, actor: &str, intent_ref: Option<&str>, op: &str, db_id: &str, status: &str) -> Result<(), error::DecapodError> {
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

        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.audit_log_path)
            .map_err(error::DecapodError::IoError)?;
        
        writeln!(f, "{}", serde_json::to_string(&ev).unwrap()).map_err(error::DecapodError::IoError)?;
        Ok(())
    }
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
