use crate::core::broker::DbBroker;
use crate::core::error;
use crate::core::schemas;
use crate::core::store::Store;
use clap::{Parser, Subcommand};
use rusqlite::{Result as SqlResult, types::ToSql};
use serde::{Deserialize, Serialize};
use std::env;
use std::path::{Path, PathBuf};
use std::fs;
use ulid::Ulid;

fn cron_db_path(root: &Path) -> PathBuf {
    root.join(schemas::CRON_DB_NAME)
}

pub fn initialize_cron_db(root: &Path) -> Result<(), error::DecapodError> {
    fs::create_dir_all(root).map_err(error::DecapodError::IoError)?;
    let broker = DbBroker::new(root);
    let db_path = cron_db_path(root);
    broker.with_conn(&db_path, "decapod", None, "cron.init", |conn| {
        conn.execute(schemas::CRON_DB_SCHEMA, [])
            .map_err(error::DecapodError::RusqliteError)?;
        Ok(())
    })?;
    Ok(())
}

fn now_iso() -> String {
    // A simplified equivalent to the Python version. For full RFC3339, a crate like `chrono` would be better.
    let now = std::time::SystemTime::now();
    let now_str = format!("{:?}", now); // Not exactly ISO, but a placeholder
    now_str
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
    if p == "/" {
        "root".to_string()
    } else {
        "root".to_string()
    }
}

fn ulid_like() -> String {
    Ulid::new().to_string()
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CronJob {
    pub id: String,
    pub name: String,
    pub description: String,
    pub schedule: String,
    pub command: String,
    pub status: String,
    pub tags: String,
    pub created_at: String,
    pub updated_at: String,
    pub dir_path: String,
    pub scope: String,
    pub last_run: Option<String>,
    pub next_run: Option<String>,
}

#[derive(Parser, Debug)]
#[clap(
    name = "cron",
    about = "Manage scheduled cron jobs within the Decapod system."
)]
pub struct CronCli {
    #[clap(subcommand)]
    pub command: CronCommand,
}

#[derive(Subcommand, Debug)]
pub enum CronCommand {
    /// Add a new cron job entry.
    Add {
        #[clap(long)]
        name: String,
        #[clap(long, default_value = "")]
        description: String,
        #[clap(long)]
        schedule: String,
        #[clap(long)]
        command: String,
        #[clap(long, default_value = "active")]
        status: String,
        #[clap(long, default_value = "")]
        tags: String,
        #[clap(long)]
        dir: Option<String>,
    },
    /// Update an existing cron job entry.
    Update {
        #[clap(long)]
        id: String,
        #[clap(long)]
        name: Option<String>,
        #[clap(long)]
        description: Option<String>,
        #[clap(long)]
        schedule: Option<String>,
        #[clap(long)]
        command: Option<String>,
        #[clap(long)]
        status: Option<String>,
        #[clap(long)]
        tags: Option<String>,
        #[clap(long)]
        last_run: Option<String>,
        #[clap(long)]
        next_run: Option<String>,
    },
    /// Retrieve a cron job entry by ID.
    Get {
        #[clap(long)]
        id: String,
    },
    /// List cron job entries.
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
    /// Delete a cron job entry.
    Delete {
        #[clap(long)]
        id: String,
    },
}

fn add_cron_job(
    root: &Path,
    name: String,
    description: String,
    schedule: String,
    command: String,
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

    let prefix = match scope.as_str() {
        "application_development" => "ADC",
        "architecture" => "ARC",
        "artificial_intelligence" => "AIC",
        "design_and_style" => "DSC",
        "development_lifecycle" => "DLC",
        "documentation" => "DOC",
        "languages" => "LAC",
        "platform_engineering" => "PEC",
        "project_management" => "PMC",
        "specialized_domains" => "SDC",
        _ => "RC",
    };

    let job_id = format!("{}_{}", prefix, ulid_like());
    let now = now_iso();
    
    let broker = DbBroker::new(root);
    let db_path = cron_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "cron.add", |conn| {
        conn.execute(schemas::CRON_DB_SCHEMA, [])?;
        conn.execute(
            "INSERT INTO cron_jobs(id, name, description, schedule, command, status, tags, created_at, updated_at, dir_path, scope)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![job_id, name, description, schedule, command, status, tags, now, now, dir_abs, scope],
        )?;
        Ok(())
    })?;

    println!(
        "{}",
        serde_json::json!({
            "ts": now_iso(),
            "cmd": "add",
            "id": job_id,
        })
    );
    Ok(())
}

fn list_cron_jobs(
    root: &Path,
    status: Option<String>,
    scope: Option<String>,
    tags: Option<String>,
    name_search: Option<String>,
    dir: Option<String>,
) -> Result<(), error::DecapodError> {
    let broker = DbBroker::new(root);
    let db_path = cron_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "cron.list", |conn| {
        let mut query = "SELECT id, name, description, schedule, command, status, last_run, next_run, tags, created_at, updated_at, dir_path, scope FROM cron_jobs WHERE 1=1".to_string();
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
        let cron_jobs_iter = stmt.query_map(&params_as_dyn[..], |row| {
            Ok(CronJob {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                schedule: row.get(3)?,
                command: row.get(4)?,
                status: row.get(5)?,
                last_run: row.get(6)?, // Correct indices
                next_run: row.get(7)?,
                tags: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
                dir_path: row.get(11)?,
                scope: row.get(12)?,
            })
        })?;

        let jobs: Vec<SqlResult<CronJob>> = cron_jobs_iter.collect();

        if jobs.is_empty() {
            println!("No cron jobs found matching the criteria.");
        } else {
            println!("Cron Jobs:");
            for job_result in jobs {
                match job_result {
                    Ok(job) => {
                        println!("----------------------------------------------------");
                        println!("ID: {}", job.id);
                        println!("Name: {}", job.name);
                        println!("Schedule: {}", job.schedule);
                        println!("Command: {}", job.command);
                        println!("Status: {}", job.status);
                        if let Some(last_run) = job.last_run {
                            println!("Last Run: {}", last_run);
                        }
                        if let Some(next_run) = job.next_run {
                            println!("Next Run: {}", next_run);
                        }
                        if !job.tags.is_empty() {
                            println!("Tags: {}", job.tags);
                        }
                        println!("Scope: {} (Path: {})", job.scope, job.dir_path);
                        println!("Last Updated: {}", job.updated_at);
                    }
                    Err(e) => eprintln!("Error reading job: {}", e),
                }
            }
            println!("----------------------------------------------------");
        }
        Ok(())
    })
}

fn get_cron_job(root: &Path, id: String) -> Result<(), error::DecapodError> {
    let broker = DbBroker::new(root);
    let db_path = cron_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "cron.get", |conn| {
        let mut stmt = conn.prepare("SELECT id, name, description, schedule, command, status, last_run, next_run, tags, created_at, updated_at, dir_path, scope FROM cron_jobs WHERE id = ?1")?;
        let mut cron_job_iter = stmt.query_map(&[&id], |row| {
            Ok(CronJob {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                schedule: row.get(3)?,
                command: row.get(4)?,
                status: row.get(5)?,
                last_run: row.get(6)?,
                next_run: row.get(7)?,
                tags: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
                dir_path: row.get(11)?,
                scope: row.get(12)?,
            })
        })?;

        if let Some(job_result) = cron_job_iter.next() {
            match job_result {
                Ok(job) => {
                    println!("{}", serde_json::to_string_pretty(&job).unwrap());
                }
                Err(e) => eprintln!("Error reading job: {}", e),
            }
        } else {
            println!(
                "{}",
                serde_json::json!({
                    "ts": now_iso(),
                    "cmd": "get",
                    "id": id,
                    "status": "not_found"
                })
            );
        }
        Ok(())
    })
}

fn delete_cron_job(root: &Path, id: String) -> Result<(), error::DecapodError> {
    let broker = DbBroker::new(root);
    let db_path = cron_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "cron.delete", |conn| {
        conn.execute("DELETE FROM cron_jobs WHERE id = ?1", &[&id])?;
        Ok(())
    })?;

    println!(
        "{}",
        serde_json::json!({ "ts": now_iso(), "cmd": "delete", "id": id, "status": "ok" })
    );
    Ok(())
}

fn update_cron_job(
    root: &Path,
    id: String,
    name: Option<String>,
    description: Option<String>,
    schedule: Option<String>,
    command: Option<String>,
    status: Option<String>,
    tags: Option<String>,
    last_run: Option<String>,
    next_run: Option<String>,
) -> Result<(), error::DecapodError> {
    let broker = DbBroker::new(root);
    let db_path = cron_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "cron.update", |conn| {
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
        if let Some(s) = schedule {
            set_clauses.push("schedule = ?");
            params.push(Box::new(s));
        }
        if let Some(c) = command {
            set_clauses.push("command = ?");
            params.push(Box::new(c));
        }
        if let Some(s) = status {
            set_clauses.push("status = ?");
            params.push(Box::new(s));
        }
        if let Some(t) = tags {
            set_clauses.push("tags = ?");
            params.push(Box::new(t));
        }
        if let Some(lr) = last_run {
            set_clauses.push("last_run = ?");
            params.push(Box::new(lr));
        }
        if let Some(nr) = next_run {
            set_clauses.push("next_run = ?");
            params.push(Box::new(nr));
        }

        if set_clauses.is_empty() {
            println!(
                "{}",
                serde_json::json!({
                    "ts": now_iso(),
                    "cmd": "update",
                    "id": id,
                    "status": "no_changes"
                })
            );
            return Ok(());
        }

        set_clauses.push("updated_at = ?");
        params.push(Box::new(now_iso()));
        params.push(Box::new(id.clone()));

        let update_sql = format!(
            "UPDATE cron_jobs SET {} WHERE id = ?",
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

pub fn run_cron_cli(store: &Store, cli: CronCli) -> Result<(), error::DecapodError> {
    let root = &store.root;
    let result = match cli.command {
        CronCommand::Add {
            name,
            description,
            schedule,
            command,
            status,
            tags,
            dir,
        } => add_cron_job(
            root,
            name,
            description,
            schedule,
            command,
            status,
            tags,
            dir,
        ),
        CronCommand::List {
            status,
            scope,
            tags,
            name_search,
            dir,
        } => list_cron_jobs(root, status, scope, tags, name_search, dir),
        CronCommand::Get { id } => get_cron_job(root, id),
        CronCommand::Delete { id } => delete_cron_job(root, id),
        CronCommand::Update {
            id,
            name,
            description,
            schedule,
            command,
            status,
            tags,
            last_run,
            next_run,
        } => update_cron_job(
            root,
            id,
            name,
            description,
            schedule,
            command,
            status,
            tags,
            last_run,
            next_run,
        ),
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
    }
    Ok(())
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "cron",
        "version": "0.1.0",
        "description": "Manage scheduled cron jobs",
        "commands": [
            { "name": "add", "parameters": ["name", "schedule", "command"] },
            { "name": "list", "parameters": ["status", "scope", "tags"] },
            { "name": "get", "parameters": ["id"] },
            { "name": "update", "parameters": ["id"] },
            { "name": "delete", "parameters": ["id"] }
        ],
        "storage": ["cron.db"]
    })
}