use crate::core::broker::DbBroker;
use crate::core::error;
use crate::core::schemas;
use crate::core::store::Store;
use clap::{Parser, Subcommand};
use rusqlite::{Result, types::ToSql};
use serde::{Deserialize, Serialize};
use std::env;
use std::path::{Path, PathBuf};
use ulid::Ulid;

fn reflex_db_path(root: &Path) -> PathBuf {
    root.join(schemas::REFLEX_DB_NAME)
}

pub fn initialize_reflex_db(root: &Path) -> Result<(), error::DecapodError> {
    std::fs::create_dir_all(root).map_err(error::DecapodError::IoError)?;
    let broker = DbBroker::new(root);
    let db_path = reflex_db_path(root);
    broker.with_conn(&db_path, "decapod", None, "reflex.init", |conn| {
        conn.execute(schemas::REFLEX_DB_SCHEMA, [])
            .map_err(error::DecapodError::RusqliteError)?;
        Ok(())
    })?;
    Ok(())
}

fn now_iso() -> String {
    let now = std::time::SystemTime::now();
    format!("{:?}", now)
}

const COMPONENT_NAMES: &[&str] = &[
    "application_development",
    "architecture",
    "artificial_intelligence",
    "design_and_style",
    "development_lifecycle",
    "documentation",
    "languages",
    "platform_engineering",
    "project_management",
    "specialized_domains",
];

fn scope_from_dir(p: &str) -> String {
    let path = Path::new(p);
    for component_name in COMPONENT_NAMES {
        if path.file_name().map(|s| s.to_string_lossy().to_lowercase())
            == Some(component_name.to_string())
            || path
                .to_string_lossy()
                .to_lowercase()
                .contains(&format!("/{}/", component_name))
        {
            return component_name.to_string();
        }
    }
    "root".to_string()
}

fn ulid_like() -> String {
    Ulid::new().to_string()
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Reflex {
    pub id: String,
    pub name: String,
    pub description: String,
    pub trigger_type: String,
    pub trigger_config: String,
    pub action_type: String,
    pub action_config: String,
    pub status: String,
    pub tags: String,
    pub created_at: String,
    pub updated_at: String,
    pub dir_path: String,
    pub scope: String,
}

#[derive(Parser, Debug)]
#[clap(
    name = "reflex",
    about = "Manage automated responses (reflexes) within the Decapod system."
)]
pub struct ReflexCli {
    #[clap(subcommand)]
    pub command: ReflexCommand,
}

#[derive(Subcommand, Debug)]
pub enum ReflexCommand {
    /// Add a new reflex entry.
    Add {
        #[clap(long)]
        name: String,
        #[clap(long, default_value = "")]
        description: String,
        #[clap(long)]
        trigger_type: String,
        #[clap(long, default_value = "{}")]
        trigger_config: String,
        #[clap(long)]
        action_type: String,
        #[clap(long)]
        action_config: String,
        #[clap(long, default_value = "active")]
        status: String,
        #[clap(long, default_value = "")]
        tags: String,
        #[clap(long)]
        dir: Option<String>,
    },
    /// Update an existing reflex entry.
    Update {
        #[clap(long)]
        id: String,
        #[clap(long)]
        name: Option<String>,
        #[clap(long)]
        description: Option<String>,
        #[clap(long)]
        trigger_type: Option<String>,
        #[clap(long)]
        trigger_config: Option<String>,
        #[clap(long)]
        action_type: Option<String>,
        #[clap(long)]
        action_config: Option<String>,
        #[clap(long)]
        status: Option<String>,
        #[clap(long)]
        tags: Option<String>,
    },
    /// Retrieve a reflex entry by ID.
    Get {
        #[clap(long)]
        id: String,
    },
    /// List reflex entries.
    List {
        #[clap(long)]
        status: Option<String>,
        #[clap(long)]
        scope: Option<String>,
        #[clap(long)]
        tags: Option<String>,
        #[clap(long)]
        name_search: Option<String>,
        #[clap(long)]
        dir: Option<String>,
    },
    /// Delete a reflex entry.
    Delete {
        #[clap(long)]
        id: String,
    },
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "reflex",
        "version": "0.1.0",
        "description": "Manage automated responses",
        "commands": [
            {
                "name": "add",
                "description": "Add a new reflex entry",
                "parameters": [
                    {"name": "name", "required": true, "description": "Unique reflex name identifier"},
                    {"name": "description", "required": false, "description": "Human-readable description of the reflex purpose", "default": ""},
                    {"name": "trigger_type", "required": true, "description": "Type of trigger (e.g., file_change, command_exit, schedule)"},
                    {"name": "trigger_config", "required": true, "description": "JSON configuration for trigger conditions", "default": "{}"},
                    {"name": "action_type", "required": true, "description": "Type of action to perform (e.g., notify, exec, webhook)"},
                    {"name": "action_config", "required": true, "description": "JSON configuration for the action to execute"},
                    {"name": "status", "required": false, "description": "Initial reflex status", "default": "active"},
                    {"name": "tags", "required": false, "description": "Comma-separated tags for categorization", "default": ""}
                ]
            },
            {
                "name": "list",
                "description": "List reflex entries",
                "parameters": [
                    {"name": "status", "required": false, "description": "Filter by status (active, paused, disabled)"},
                    {"name": "scope", "required": false, "description": "Filter by scope directory"},
                    {"name": "tags", "required": false, "description": "Filter by comma-separated tags"}
                ]
            },
            {
                "name": "get",
                "description": "Retrieve a reflex entry by ID",
                "parameters": [
                    {"name": "id", "required": true, "description": "Reflex entry ID to retrieve"}
                ]
            },
            {
                "name": "update",
                "description": "Update an existing reflex entry",
                "parameters": [
                    {"name": "id", "required": true, "description": "Reflex entry ID to update"}
                ]
            },
            {
                "name": "delete",
                "description": "Delete a reflex entry",
                "parameters": [
                    {"name": "id", "required": true, "description": "Reflex entry ID to delete"}
                ]
            }
        ],
        "storage": ["reflex.db"]
    })
}

pub fn run_reflex_cli(store: &Store, cli: ReflexCli) {
    let root = &store.root;
    let result = match cli.command {
        ReflexCommand::Add {
            name,
            description,
            trigger_type,
            trigger_config,
            action_type,
            action_config,
            status,
            tags,
            dir,
        } => add_reflex(
            root,
            name,
            description,
            trigger_type,
            trigger_config,
            action_type,
            action_config,
            status,
            tags,
            dir,
        ),
        ReflexCommand::Update {
            id,
            name,
            description,
            trigger_type,
            trigger_config,
            action_type,
            action_config,
            status,
            tags,
        } => update_reflex(
            root,
            id,
            name,
            description,
            trigger_type,
            trigger_config,
            action_type,
            action_config,
            status,
            tags,
        ),
        ReflexCommand::Get { id } => get_reflex(root, id),
        ReflexCommand::List {
            status,
            scope,
            tags,
            name_search,
            dir,
        } => list_reflexes(root, status, scope, tags, name_search, dir),
        ReflexCommand::Delete { id } => delete_reflex(root, id),
    };
    if let Err(e) = result {
        eprintln!("Error: {}", e);
    }
}

#[allow(clippy::too_many_arguments)]
fn add_reflex(
    root: &Path,
    name: String,
    description: String,
    trigger_type: String,
    trigger_config: String,
    action_type: String,
    action_config: String,
    status: String,
    tags: String,
    dir: Option<String>,
) -> Result<(), error::DecapodError> {
    let dir_path = dir.unwrap_or_else(|| env::current_dir().unwrap().to_string_lossy().to_string());
    let dir_abs = Path::new(&dir_path)
        .canonicalize()
        .map_err(error::DecapodError::IoError)?
        .to_string_lossy()
        .to_string();
    let scope = scope_from_dir(&dir_abs);

    let reflex_id = format!("REF_{}", ulid_like());
    let now = now_iso();

    let broker = DbBroker::new(root);
    let db_path = reflex_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "reflex.add", |conn| {
        conn.execute(
            "INSERT INTO reflexes(id, name, description, trigger_type, trigger_config, action_type, action_config, status, tags, created_at, updated_at, dir_path, scope)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            rusqlite::params![reflex_id, name, description, trigger_type, trigger_config, action_type, action_config, status, tags, now, now, dir_abs, scope],
        )?;
        Ok(())
    })?;

    println!(
        "{}",
        serde_json::json!({
            "ts": now_iso(),
            "cmd": "add",
            "id": reflex_id,
        })
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn update_reflex(
    root: &Path,
    id: String,
    name: Option<String>,
    description: Option<String>,
    trigger_type: Option<String>,
    trigger_config: Option<String>,
    action_type: Option<String>,
    action_config: Option<String>,
    status: Option<String>,
    tags: Option<String>,
) -> Result<(), error::DecapodError> {
    let broker = DbBroker::new(root);
    let db_path = reflex_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "reflex.update", |conn| {
        let mut set_clauses = Vec::new();
        let mut params: Vec<Box<dyn ToSql>> = Vec::new();

        if let Some(n) = name {
            set_clauses.push("name = ?");
            params.push(Box::new(n));
        }
        if let Some(d) = description {
            set_clauses.push("description = ?");
            params.push(Box::new(d));
        }
        if let Some(tt) = trigger_type {
            set_clauses.push("trigger_type = ?");
            params.push(Box::new(tt));
        }
        if let Some(tc) = trigger_config {
            set_clauses.push("trigger_config = ?");
            params.push(Box::new(tc));
        }
        if let Some(at) = action_type {
            set_clauses.push("action_type = ?");
            params.push(Box::new(at));
        }
        if let Some(ac) = action_config {
            set_clauses.push("action_config = ?");
            params.push(Box::new(ac));
        }
        if let Some(s) = status {
            set_clauses.push("status = ?");
            params.push(Box::new(s));
        }
        if let Some(t) = tags {
            set_clauses.push("tags = ?");
            params.push(Box::new(t));
        }

        if set_clauses.is_empty() {
            println!(
                "{}",
                serde_json::json!({ "ts": now_iso(), "cmd": "update", "id": id, "status": "no_changes" })
            );
            return Ok(());
        }

        set_clauses.push("updated_at = ?");
        params.push(Box::new(now_iso()));
        params.push(Box::new(id.clone()));

        let update_sql = format!(
            "UPDATE reflexes SET {} WHERE id = ?",
            set_clauses.join(", ")
        );
        let params_as_dyn: Vec<&dyn ToSql> = params.iter().map(|p| p.as_ref()).collect();
        conn.execute(&update_sql, &params_as_dyn[..])?;

        println!(
            "{}",
            serde_json::json!({ "ts": now_iso(), "cmd": "update", "id": id, "status": "ok" })
        );
        Ok(())
    })
}

fn get_reflex(root: &Path, id: String) -> Result<(), error::DecapodError> {
    let broker = DbBroker::new(root);
    let db_path = reflex_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "reflex.get", |conn| {
        let mut stmt = conn.prepare("SELECT * FROM reflexes WHERE id = ?1")?;
        let mut rows = stmt.query_map([&id], |row| {
            Ok(Reflex {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                trigger_type: row.get(3)?,
                trigger_config: row.get(4)?,
                action_type: row.get(5)?,
                action_config: row.get(6)?,
                status: row.get(7)?,
                tags: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
                dir_path: row.get(11)?,
                scope: row.get(12)?,
            })
        })?;

        if let Some(reflex_result) = rows.next() {
            match reflex_result {
                Ok(reflex) => println!("{}", serde_json::to_string_pretty(&reflex).unwrap()),
                Err(e) => eprintln!("Error reading reflex: {}", e),
            }
        } else {
            println!(
                "{}",
                serde_json::json!({ "ts": now_iso(), "cmd": "get", "id": id, "status": "not_found" })
            );
        }
        Ok(())
    })
}

fn list_reflexes(
    root: &Path,
    status: Option<String>,
    scope: Option<String>,
    tags: Option<String>,
    name_search: Option<String>,
    dir: Option<String>,
) -> Result<(), error::DecapodError> {
    let broker = DbBroker::new(root);
    let db_path = reflex_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "reflex.list", |conn| {
        let mut query = "SELECT * FROM reflexes WHERE 1=1".to_string();
        let mut params: Vec<Box<dyn ToSql>> = Vec::new();

        if let Some(s) = status {
            query.push_str(" AND status = ?");
            params.push(Box::new(s));
        }
        if let Some(s) = scope {
            query.push_str(" AND scope = ?");
            params.push(Box::new(s));
        }
        if let Some(t) = tags {
            query.push_str(" AND tags LIKE ?");
            params.push(Box::new(format!("%{}%", t)));
        }
        if let Some(n) = name_search {
            query.push_str(" AND name LIKE ?");
            params.push(Box::new(format!("%{}%", n)));
        }
        if let Some(d) = dir {
            query.push_str(" AND dir_path = ?");
            params.push(Box::new(d));
        }

        query.push_str(" ORDER BY updated_at DESC");

        let mut stmt = conn.prepare(&query)?;
        let params_as_dyn: Vec<&dyn ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(&params_as_dyn[..], |row| {
            Ok(Reflex {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                trigger_type: row.get(3)?,
                trigger_config: row.get(4)?,
                action_type: row.get(5)?,
                action_config: row.get(6)?,
                status: row.get(7)?,
                tags: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
                dir_path: row.get(11)?,
                scope: row.get(12)?,
            })
        })?;

        println!("Reflexes:");
        for reflex in rows {
            let r = reflex?;
            println!("----------------------------------------------------");
            println!(
                "ID: {}\nName: {}\nTrigger: {} ({})\nAction: {} ({})\nStatus: {}\nScope: {} (Path: {})\nUpdated: {}",
                r.id,
                r.name,
                r.trigger_type,
                r.trigger_config,
                r.action_type,
                r.action_config,
                r.status,
                r.scope,
                r.dir_path,
                r.updated_at
            );
        }
        println!("----------------------------------------------------");
        Ok(())
    })
}

fn delete_reflex(root: &Path, id: String) -> Result<(), error::DecapodError> {
    let broker = DbBroker::new(root);
    let db_path = reflex_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "reflex.delete", |conn| {
        conn.execute("DELETE FROM reflexes WHERE id = ?1", [&id])?;
        Ok(())
    })?;

    println!(
        "{}",
        serde_json::json!({ "ts": now_iso(), "cmd": "delete", "id": id, "status": "ok" })
    );
    Ok(())
}
