use crate::core::broker::DbBroker;
use crate::core::error;
use crate::core::schemas;
use crate::core::store::Store;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FeedbackEntry {
    pub id: String,
    pub source: String,
    pub text: String,
    pub links: Option<String>,
    pub created_at: String,
}

pub fn feedback_db_path(root: &Path) -> PathBuf {
    root.join(schemas::FEEDBACK_DB_NAME)
}

pub fn initialize_feedback_db(root: &Path) -> Result<(), error::DecapodError> {
    let broker = DbBroker::new(root);
    let db_path = feedback_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "feedback.init", |conn| {
        conn.execute(schemas::FEEDBACK_DB_SCHEMA, [])?;
        Ok(())
    })
}

pub fn add_feedback(
    store: &Store,
    source: &str,
    text: &str,
    links: Option<&str>,
) -> Result<String, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = feedback_db_path(&store.root);
    let id = ulid::Ulid::new().to_string();
    let now = format!("{:?}", std::time::SystemTime::now());

    broker.with_conn(&db_path, "decapod", None, "feedback.add", |conn| {
        conn.execute(
            "INSERT INTO feedback(id, source, text, links, created_at) VALUES(?1, ?2, ?3, ?4, ?5)",
            params![id, source, text, links, now],
        )?;
        Ok(())
    })?;

    Ok(id)
}

pub fn propose_prefs(store: &Store) -> Result<String, error::DecapodError> {
    // This generates a proposal text based on feedback.
    // It MUST NOT edit files directly.
    let broker = DbBroker::new(&store.root);
    let db_path = feedback_db_path(&store.root);

    let entries = broker.with_conn(&db_path, "decapod", None, "feedback.list", |conn| {
        let mut stmt = conn.prepare("SELECT id, source, text, links, created_at FROM feedback ORDER BY created_at DESC LIMIT 10")?;
        let rows = stmt.query_map([], |row| {
            Ok(FeedbackEntry {
                id: row.get(0)?,
                source: row.get(1)?,
                text: row.get(2)?,
                links: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })?;

    if entries.is_empty() {
        return Ok("No feedback found to base a proposal on.".to_string());
    }

    let mut proposal = "DECAPOD PREFERENCE PROPOSAL (NON-BINDING)
"
    .to_string();
    proposal.push_str(
        "============================
",
    );
    proposal.push_str(
        "Evidence cited:
",
    );
    for e in &entries {
        proposal.push_str(&format!(
            "- [{}] {}: {}
",
            e.id, e.source, e.text
        ));
    }
    proposal.push_str(
        "
PROPOSED DIFF (against SYSTEM.md):
",
    );
    proposal.push_str(
        "--- SYSTEM.md
+++ SYSTEM.md
",
    );
    proposal.push_str(
        "@@ -10,1 +10,1 @@
",
    );
    proposal.push_str(
        "- [Placeholder rule]
+ [New rule derived from feedback]
",
    );

    Ok(proposal)
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "feedback",
        "version": "0.1.0",
        "description": "Append-only operator feedback ledger",
        "commands": [
            { "name": "add", "parameters": ["source", "text", "links"] },
            { "name": "propose", "description": "Generate a preference proposal based on feedback" }
        ],
        "storage": ["feedback.db"]
    })
}
