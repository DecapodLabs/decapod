use crate::core::broker::DbBroker;
use crate::core::error;
use crate::core::store::Store;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KnowledgeEntry {
    pub id: String,
    pub title: String,
    pub content: String,
    pub provenance: String,
    pub claim_id: Option<String>,
    pub created_at: String,
}

pub fn knowledge_db_path(root: &Path) -> PathBuf {
    root.join("knowledge.db")
}

pub fn add_knowledge(
    store: &Store,
    id: &str,
    title: &str,
    content: &str,
    provenance: &str,
    claim_id: Option<&str>,
) -> Result<(), error::DecapodError> {
    use regex::Regex;
    let prov_re = Regex::new(
        r"^(file:[^#]+(#L\d+(-L\d+)?)?|url:[^ ]+|cmd:[^ ]+|commit:[a-f0-9]+|event:[A-Z0-9]+)$",
    )
    .unwrap();

    if !prov_re.is_match(provenance) {
        return Err(error::DecapodError::ValidationError(format!(
            "Invalid provenance format: '{}'. Must match scheme (file:|url:|cmd:|commit:|event:)",
            provenance
        )));
    }

    let broker = DbBroker::new(&store.root);
    let db_path = knowledge_db_path(&store.root);
    let now = now_iso();

    broker.with_conn(&db_path, "decapod", None, "knowledge.add", |conn| {
        conn.execute(
            "INSERT INTO knowledge(id, title, content, provenance, claim_id, created_at, dir_path, scope)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                title,
                content,
                provenance,
                claim_id,
                now,
                store.root.to_string_lossy(),
                "root"
            ],
        )?;
        Ok(())
    })
}

pub fn search_knowledge(
    store: &Store,
    query: &str,
) -> Result<Vec<KnowledgeEntry>, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = knowledge_db_path(&store.root);

    broker.with_conn(&db_path, "decapod", None, "knowledge.search", |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, title, content, provenance, claim_id, created_at FROM knowledge
             WHERE title LIKE ?1 OR content LIKE ?1 OR provenance LIKE ?1",
        )?;
        let q = format!("%{}%", query);
        let rows = stmt.query_map(params![q], |row| {
            Ok(KnowledgeEntry {
                id: row.get(0)?,
                title: row.get(1)?,
                content: row.get(2)?,
                provenance: row.get(3)?,
                claim_id: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;

        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }
        Ok(results)
    })
}

fn now_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    format!("{}Z", secs)
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "knowledge",
        "version": "0.1.0",
        "description": "Repository context and rationale (minimal)",
        "commands": [
            { "name": "add", "parameters": ["id", "title", "text", "provenance", "claim_id"] },
            { "name": "search", "parameters": ["query"] }
        ],
        "storage": ["knowledge.db"]
    })
}
