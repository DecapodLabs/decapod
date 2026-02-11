use crate::core::broker::DbBroker;
use crate::core::error;
use crate::core::schemas; // Import the new schemas module
use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};

pub fn db_connect(db_path: &str) -> Result<Connection, error::DecapodError> {
    let conn = Connection::open(db_path)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(error::DecapodError::RusqliteError)?;
    conn.query_row("PRAGMA journal_mode=WAL;", [], |_| Ok(()))
        .map_err(error::DecapodError::RusqliteError)?;
    conn.execute("PRAGMA foreign_keys=ON;", [])
        .map_err(error::DecapodError::RusqliteError)?;
    Ok(conn)
}

pub fn knowledge_db_path(root: &Path) -> PathBuf {
    root.join(schemas::KNOWLEDGE_DB_NAME)
}

pub fn initialize_knowledge_db(root: &Path) -> Result<(), error::DecapodError> {
    let db_path = knowledge_db_path(root);
    let parent_dir = db_path.parent().unwrap();
    fs::create_dir_all(parent_dir).map_err(error::DecapodError::IoError)?;

    let broker = DbBroker::new(root);
    broker.with_conn(&db_path, "decapod", None, "knowledge.init", |conn| {
        conn.execute(schemas::KNOWLEDGE_DB_SCHEMA, [])?;
        Ok(())
    })?;

    println!("Knowledge database initialized at {}", db_path.display());
    Ok(())
}

// Subsystems own their schemas and initialization. Avoid generic "plugin DB" APIs until
// a real extension mechanism exists.
