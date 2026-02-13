//! Teammate plugin: remembers user preferences and requested behaviors.
//!
//! This plugin catalogs distinct user expectations like:
//! - SSH key preferences for Git operations
//! - Branch naming conventions
//! - Code style preferences
//! - Commit message formats
//! - Workflow conventions
//!
//! # For AI Agents
//!
//! - **Check preferences before acting**: Use `decapod teammate get <key>` to check user preferences
//! - **Record new preferences**: When user expresses a preference, record it with `decapod teammate add`
//! - **Categories organize preferences**: Use categories like "git", "style", "workflow" for organization

use crate::core::broker::DbBroker;
use crate::core::error;
use crate::core::store::Store;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const TEAMMATE_DB_NAME: &str = "teammate.db";
pub const TEAMMATE_DB_SCHEMA_TABLE: &str = "
    CREATE TABLE IF NOT EXISTS preferences (
        id TEXT PRIMARY KEY,
        category TEXT NOT NULL,
        key TEXT NOT NULL,
        value TEXT NOT NULL,
        context TEXT,
        source TEXT NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT,
        UNIQUE(category, key)
    )
";
pub const TEAMMATE_DB_SCHEMA_INDEX_CATEGORY: &str =
    "CREATE INDEX IF NOT EXISTS idx_preferences_category ON preferences(category)";
pub const TEAMMATE_DB_SCHEMA_INDEX_KEY: &str =
    "CREATE INDEX IF NOT EXISTS idx_preferences_key ON preferences(key)";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Preference {
    pub id: String,
    pub category: String,
    pub key: String,
    pub value: String,
    pub context: Option<String>,
    pub source: String,
    pub created_at: String,
    pub updated_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PreferenceInput {
    pub category: String,
    pub key: String,
    pub value: String,
    pub context: Option<String>,
    pub source: String,
}

pub fn teammate_db_path(root: &Path) -> PathBuf {
    root.join(TEAMMATE_DB_NAME)
}

pub fn initialize_teammate_db(root: &Path) -> Result<(), error::DecapodError> {
    let broker = DbBroker::new(root);
    let db_path = teammate_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "teammate.init", |conn| {
        conn.execute(TEAMMATE_DB_SCHEMA_TABLE, [])?;
        conn.execute(TEAMMATE_DB_SCHEMA_INDEX_CATEGORY, [])?;
        conn.execute(TEAMMATE_DB_SCHEMA_INDEX_KEY, [])?;
        Ok(())
    })
}

pub fn add_preference(
    store: &Store,
    input: PreferenceInput,
) -> Result<String, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = teammate_db_path(&store.root);
    let id = ulid::Ulid::new().to_string();
    let now = format!("{:?}", std::time::SystemTime::now());

    broker.with_conn(&db_path, "decapod", None, "teammate.add", |conn| {
        conn.execute(
            "INSERT INTO preferences(id, category, key, value, context, source, created_at, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)
             ON CONFLICT(category, key) DO UPDATE SET
                value = excluded.value,
                context = excluded.context,
                source = excluded.source,
                updated_at = ?7",
            params![
                id,
                input.category,
                input.key,
                input.value,
                input.context,
                input.source,
                now
            ],
        )?;
        Ok(())
    })?;

    Ok(id)
}

pub fn get_preference(
    store: &Store,
    category: &str,
    key: &str,
) -> Result<Option<Preference>, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = teammate_db_path(&store.root);

    let pref = broker.with_conn(&db_path, "decapod", None, "teammate.get", |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, category, key, value, context, source, created_at, updated_at
             FROM preferences WHERE category = ?1 AND key = ?2",
        )?;
        let result = stmt.query_row(params![category, key], |row| {
            Ok(Preference {
                id: row.get(0)?,
                category: row.get(1)?,
                key: row.get(2)?,
                value: row.get(3)?,
                context: row.get(4)?,
                source: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        });

        match result {
            Ok(p) => Ok(Some(p)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(error::DecapodError::RusqliteError(e)),
        }
    })?;

    Ok(pref)
}

fn row_to_preference(row: &rusqlite::Row) -> Result<Preference, rusqlite::Error> {
    Ok(Preference {
        id: row.get(0)?,
        category: row.get(1)?,
        key: row.get(2)?,
        value: row.get(3)?,
        context: row.get(4)?,
        source: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

pub fn list_preferences(
    store: &Store,
    category: Option<&str>,
) -> Result<Vec<Preference>, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = teammate_db_path(&store.root);

    let entries = broker.with_conn(&db_path, "decapod", None, "teammate.list", |conn| {
        let mut out = Vec::new();

        if let Some(cat) = category {
            let mut stmt = conn.prepare(
                "SELECT id, category, key, value, context, source, created_at, updated_at
                 FROM preferences WHERE category = ?1 ORDER BY key",
            )?;
            let rows = stmt.query_map([cat], row_to_preference)?;
            for r in rows {
                out.push(r?);
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, category, key, value, context, source, created_at, updated_at
                 FROM preferences ORDER BY category, key",
            )?;
            let rows = stmt.query_map([], row_to_preference)?;
            for r in rows {
                out.push(r?);
            }
        }

        Ok(out)
    })?;

    Ok(entries)
}

pub fn get_preferences_by_category(
    store: &Store,
) -> Result<HashMap<String, Vec<Preference>>, error::DecapodError> {
    let all = list_preferences(store, None)?;
    let mut grouped: HashMap<String, Vec<Preference>> = HashMap::new();

    for pref in all {
        grouped.entry(pref.category.clone()).or_default().push(pref);
    }

    Ok(grouped)
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "teammate",
        "version": "0.1.0",
        "description": "User preference and behavior memory system",
        "commands": [
            { "name": "add", "description": "Add or update a preference", "parameters": ["category", "key", "value", "context", "source"] },
            { "name": "get", "description": "Get a specific preference", "parameters": ["category", "key"] },
            { "name": "list", "description": "List all preferences, optionally filtered by category", "parameters": ["category?"] }
        ],
        "storage": ["teammate.db"],
        "categories": [
            "git", "style", "workflow", "communication", "tooling"
        ]
    })
}

// CLI types for clap integration
#[derive(clap::Args, Debug)]
pub struct TeammateCli {
    #[clap(subcommand)]
    pub command: TeammateCommand,
}

#[derive(clap::Subcommand, Debug)]
pub enum TeammateCommand {
    /// Add or update a preference
    Add {
        /// Category (e.g., git, style, workflow)
        #[clap(long)]
        category: String,
        /// Preference key
        #[clap(long)]
        key: String,
        /// Preference value
        #[clap(long)]
        value: String,
        /// Optional context/explanation
        #[clap(long)]
        context: Option<String>,
        /// Source of the preference (e.g., "user_request", "observed_behavior")
        #[clap(long, default_value = "user_request")]
        source: String,
    },
    /// Get a specific preference
    Get {
        /// Category
        #[clap(long)]
        category: String,
        /// Preference key
        #[clap(long)]
        key: String,
    },
    /// List preferences
    List {
        /// Filter by category
        #[clap(long)]
        category: Option<String>,
        /// Output format
        #[clap(long, default_value = "text")]
        format: String,
    },
}

pub fn run_teammate_cli(store: &Store, cli: TeammateCli) -> Result<(), error::DecapodError> {
    initialize_teammate_db(&store.root)?;

    match cli.command {
        TeammateCommand::Add {
            category,
            key,
            value,
            context,
            source,
        } => {
            let input = PreferenceInput {
                category,
                key: key.clone(),
                value: value.clone(),
                context,
                source,
            };
            let id = add_preference(store, input)?;
            println!("✓ Preference recorded: {}={} (id: {})", key, value, id);
        }
        TeammateCommand::Get { category, key } => match get_preference(store, &category, &key)? {
            Some(pref) => {
                println!("{}: {}", pref.key, pref.value);
                if let Some(ctx) = pref.context {
                    println!("  Context: {}", ctx);
                }
                println!("  Source: {} | Created: {}", pref.source, pref.created_at);
            }
            None => {
                println!("No preference found for {}.{}", category, key);
            }
        },
        TeammateCommand::List { category, format } => {
            let prefs = list_preferences(store, category.as_deref())?;

            if format == "json" {
                println!("{}", serde_json::to_string_pretty(&prefs).unwrap());
            } else if prefs.is_empty() {
                println!("No preferences recorded yet.");
            } else {
                let grouped = get_preferences_by_category(store)?;
                for (cat, items) in grouped {
                    println!("\n[{}]", cat);
                    for item in items {
                        println!("  {} = {}", item.key, item.value);
                    }
                }
            }
        }
    }

    Ok(())
}
