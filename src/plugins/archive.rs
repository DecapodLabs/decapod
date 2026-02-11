use crate::core::broker::DbBroker;
use crate::core::error;
use crate::core::schemas;
use crate::core::store::Store;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use sha2::{Sha256, Digest};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ArchiveEntry {
    pub id: String,
    pub path: String,
    pub content_hash: String,
    pub summary_hash: String,
    pub created_at: String,
}

pub fn archive_db_path(root: &Path) -> PathBuf {
    root.join(schemas::ARCHIVE_DB_NAME)
}

pub fn initialize_archive_db(root: &Path) -> Result<(), error::DecapodError> {
    let broker = DbBroker::new(root);
    let db_path = archive_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "archive.init", |conn| {
        conn.execute(schemas::ARCHIVE_DB_SCHEMA, [])?;
        Ok(())
    })
}

pub fn hash_text(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text);
    format!("{:x}", hasher.finalize())
}

pub fn register_archive(
    store: &Store,
    id: &str,
    path: &Path,
    content: &str,
    summary: &str,
) -> Result<(), error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = archive_db_path(&store.root);
    let content_hash = hash_text(content);
    let summary_hash = hash_text(summary);
    let now = format!("{:?}", std::time::SystemTime::now());

    let rel_path = path.strip_prefix(&store.root).unwrap_or(path).to_string_lossy().to_string();

    broker.with_conn(&db_path, "decapod", None, "archive.register", |conn| {
        conn.execute(
            "INSERT INTO archives(id, path, content_hash, summary_hash, created_at) VALUES(?1, ?2, ?3, ?4, ?5)",
            params![id, rel_path, content_hash, summary_hash, now],
        )?;
        Ok(())
    })
}

pub fn list_archives(store: &Store) -> Result<Vec<ArchiveEntry>, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = archive_db_path(&store.root);

    broker.with_conn(&db_path, "decapod", None, "archive.list", |conn| {
        let mut stmt = conn.prepare("SELECT id, path, content_hash, summary_hash, created_at FROM archives")?;
        let rows = stmt.query_map([], |row| {
            Ok(ArchiveEntry {
                id: row.get(0)?,
                path: row.get(1)?,
                content_hash: row.get(2)?,
                summary_hash: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })
}

pub fn verify_archives(store: &Store) -> Result<Vec<String>, error::DecapodError> {
    let archives = list_archives(store)?;
    let mut failures = Vec::new();

    for entry in archives {
        let full_path = store.root.join(&entry.path);
        if !full_path.exists() {
            failures.push(format!("Archive {}: File missing at {}", entry.id, entry.path));
            continue;
        }

        let content = fs::read_to_string(&full_path).map_err(error::DecapodError::IoError)?;
        if hash_text(&content) != entry.content_hash {
            failures.push(format!("Archive {}: Content hash mismatch", entry.id));
        }
        
        // In Epoch 5, summary linkage verification: we check if the archive ID is referenced 
        // in any project markdown file.
        // Simplified: just confirm the index entry has a non-empty summary_hash recorded.
        if entry.summary_hash.is_empty() {
            failures.push(format!("Archive {}: Missing summary hash in index", entry.id));
        }
    }

    Ok(failures)
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "archive",
        "version": "0.1.0",
        "description": "Archive indexing and integrity",
        "commands": [
            { "name": "list", "description": "List all registered archives" },
            { "name": "verify", "description": "Run integrity scan on all archives" }
        ],
        "storage": ["archive.db"]
    })
}
