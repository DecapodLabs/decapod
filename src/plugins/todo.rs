use crate::core::broker::DbBroker;
use crate::core::error;
use crate::policy;
use crate::core::schemas; // Import the new schemas module
use crate::core::store::{Store, StoreKind};
use clap::{Parser, Subcommand, ValueEnum};
use rusqlite::{Connection, OptionalExtension, Result as SqlResult, types::ToSql};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use ulid::Ulid;

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Parser, Debug)]
#[clap(name = "todo", about = "Manage TODO tasks within the Decapod system.")]
pub struct TodoCli {
    /// Output format for this command group.
    #[clap(long, global = true, value_enum, default_value = "text")]
    format: OutputFormat,
    #[clap(subcommand)]
    command: TodoCommand,
}

#[derive(Subcommand, Debug)]
enum TodoCommand {
    /// Add a new task.
    Add {
        #[clap(long)]
        title: String,
        #[clap(long, default_value = "")]
        tags: String,
        #[clap(long, default_value = "")]
        owner: String,
        #[clap(long)]
        due: Option<String>,
        #[clap(long, default_value = "")]
        r#ref: String,
        #[clap(long)]
        dir: Option<String>,
        #[clap(long, default_value = "medium")]
        priority: String,
        #[clap(long, default_value = "")]
        depends_on: String,
        #[clap(long, default_value = "")]
        blocks: String,
        #[clap(long)]
        parent: Option<String>,
    },
    /// List tasks.
    List {
        #[clap(long)]
        status: Option<String>,
        #[clap(long)]
        scope: Option<String>,
        #[clap(long)]
        tags: Option<String>,
        #[clap(long)]
        title_search: Option<String>,
        #[clap(long)]
        dir: Option<String>,
    },
    /// Get a task by ID.
    Get {
        #[clap(long)]
        id: String,
    },
    /// Mark a task done.
    Done {
        #[clap(long)]
        id: String,
    },
    /// Archive a task (keeps audit trail).
    Archive {
        #[clap(long)]
        id: String,
    },
    /// Add a comment to a task (audit-only event).
    Comment {
        #[clap(long)]
        id: String,
        #[clap(long)]
        comment: String,
    },
    /// Rebuild the SQLite DB deterministically from the JSONL event log.
    Rebuild,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Task {
    id: String,
    title: String,
    tags: String,
    owner: String,
    due: Option<String>,
    r#ref: String,
    status: String,
    created_at: String,
    updated_at: String,
    completed_at: Option<String>,
    dir_path: String,
    scope: String,
    parent_task_id: Option<String>,
    priority: String,
    depends_on: String,
    blocks: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct TodoEvent {
    ts: String,
    event_id: String,
    event_type: String,
    task_id: Option<String>,
    payload: JsonValue,
    actor: String,
}

fn now_iso() -> String {
    // Good enough for stable ordering and human readability; we can switch to chrono later.
    // Use RFC3339-like UTC seconds with 'Z' suffix.
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{}Z", secs)
}

fn todo_db_path(root: &Path) -> PathBuf {
    root.join(schemas::TODO_DB_NAME)
}

fn events_path(root: &Path) -> PathBuf {
    root.join(schemas::TODO_EVENTS_NAME)
}

fn connect_todo(root: &Path) -> Result<Connection, error::DecapodError> {
    let db_path = todo_db_path(root);
    crate::db::db_connect(&db_path.to_string_lossy())
}

fn ensure_schema(conn: &Connection) -> Result<(), error::DecapodError> {
    conn.execute(schemas::TODO_DB_SCHEMA_META, [])?;

    let current: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(error::DecapodError::RusqliteError)?;

    let current_version: u32 = current
        .as_deref()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);

    if current_version >= schemas::TODO_SCHEMA_VERSION {
        return Ok(());
    }

    conn.execute(schemas::TODO_DB_SCHEMA_TASKS, [])?;
    conn.execute(schemas::TODO_DB_SCHEMA_TASK_EVENTS, [])?;
    conn.execute(schemas::TODO_DB_SCHEMA_INDEX_STATUS, [])?;
    conn.execute(schemas::TODO_DB_SCHEMA_INDEX_SCOPE, [])?;
    conn.execute(schemas::TODO_DB_SCHEMA_INDEX_DIR, [])?;
    conn.execute(schemas::TODO_DB_SCHEMA_INDEX_EVENTS_TASK, [])?;

    conn.execute(
        "INSERT INTO meta(key, value) VALUES('schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        [schemas::TODO_SCHEMA_VERSION.to_string()],
    )?;

    Ok(())
}

pub fn initialize_todo_db(root: &Path) -> Result<(), error::DecapodError> {
    fs::create_dir_all(root).map_err(error::DecapodError::IoError)?;
    let broker = DbBroker::new(root);
    let db_path = todo_db_path(root);
    broker.with_conn(&db_path, "decapod", None, "todo.init", |conn| {
        ensure_schema(conn)?;
        Ok(())
    })?;
    Ok(())
}

fn scope_from_dir(p: &str) -> String {
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

    let path = Path::new(p);
    for component_name in COMPONENT_NAMES {
        if path.file_name().map(|s| s.to_string_lossy().to_lowercase())
            == Some(component_name.to_string())
            || p.to_lowercase().contains(&format!("/{}/", component_name))
        {
            return component_name.to_string();
        }
    }
    "root".to_string()
}

fn task_prefix_from_scope(scope: &str) -> &'static str {
    match scope {
        "application_development" => "AD",
        "architecture" => "AR",
        "artificial_intelligence" => "AI",
        "design_and_style" => "DS",
        "development_lifecycle" => "DL",
        "documentation" => "DO",
        "languages" => "LA",
        "platform_engineering" => "PE",
        "project_management" => "PM",
        "specialized_domains" => "SD",
        _ => "R",
    }
}

fn append_event(root: &Path, ev: &TodoEvent) -> Result<(), error::DecapodError> {
    let path = events_path(root);
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(error::DecapodError::IoError)?;
    writeln!(f, "{}", serde_json::to_string(ev).unwrap()).map_err(error::DecapodError::IoError)?;
    Ok(())
}

fn insert_event(conn: &Connection, ev: &TodoEvent) -> SqlResult<()> {
    conn.execute(
        "INSERT INTO task_events(event_id, ts, event_type, task_id, payload, actor)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            ev.event_id,
            ev.ts,
            ev.event_type,
            ev.task_id,
            serde_json::to_string(&ev.payload).unwrap(),
            ev.actor
        ],
    )?;
    Ok(())
}

fn add_task(root: &Path, args: &TodoCommand) -> Result<serde_json::Value, error::DecapodError> {
    let TodoCommand::Add {
        title,
        tags,
        owner,
        due,
        r#ref,
        dir,
        priority,
        depends_on,
        blocks,
        parent,
    } = args
    else {
        return Err(error::DecapodError::ValidationError(
            "invalid command".into(),
        ));
    };

    let dir_path = dir
        .clone()
        .unwrap_or_else(|| env::current_dir().unwrap().to_string_lossy().to_string());
    let dir_abs = Path::new(&dir_path)
        .canonicalize()
        .map_err(error::DecapodError::IoError)?
        .to_string_lossy()
        .to_string();
    let scope = scope_from_dir(&dir_abs);
    let prefix = task_prefix_from_scope(&scope);
    let task_id = format!("{}_{}", prefix, Ulid::new());
    let ts = now_iso();

    let broker = DbBroker::new(root);
    let db_path = todo_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "todo.add", |conn| {
        ensure_schema(conn)?;
        conn.execute(
            "INSERT INTO tasks(id, title, tags, owner, due, ref, status, created_at, updated_at, completed_at, dir_path, scope, parent_task_id, priority, depends_on, blocks)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, 'open', ?7, ?8, NULL, ?9, ?10, ?11, ?12, ?13, ?14)",
            rusqlite::params![
                task_id,
                title,
                tags,
                owner,
                due,
                r#ref,
                ts,
                ts,
                dir_abs,
                scope,
                parent,
                priority,
                depends_on,
                blocks
            ],
        )?;

        let ev = TodoEvent {
            ts: ts.clone(),
            event_id: Ulid::new().to_string(),
            event_type: "task.add".to_string(),
            task_id: Some(task_id.clone()),
            payload: serde_json::json!({
                "title": title,
                "tags": tags,
                "owner": owner,
                "due": due,
                "ref": r#ref,
                "dir_path": dir_abs,
                "scope": scope,
                "parent_task_id": parent,
                "priority": priority,
                "depends_on": depends_on,
                "blocks": blocks,
            }),
            actor: "decapod".to_string(),
        };
        append_event(root, &ev)?;
        insert_event(conn, &ev).map_err(error::DecapodError::RusqliteError)?;
        Ok(())
    })?;

    Ok(serde_json::json!({
        "ts": ts,
        "cmd": "todo.add",
        "status": "ok",
        "root": root.to_string_lossy(),
        "id": task_id,
    }))
}

fn update_status(
    store: &Store,
    id: &str,
    new_status: &str,
    event_type: &str,
    payload: JsonValue,
) -> Result<serde_json::Value, error::DecapodError> {
    let ts = now_iso();
    let root = &store.root;
    let broker = DbBroker::new(root);
    let db_path = todo_db_path(root);

    // Risk Check
    let risk_map_path = root.join("RISKMAP.json");
    let risk_map = if risk_map_path.exists() {
        let content = std::fs::read_to_string(risk_map_path)?;
        serde_json::from_str(&content).unwrap_or(policy::RiskMap { zones: vec![] })
    } else {
        policy::RiskMap { zones: vec![] }
    };
    let (level, _) = policy::eval_risk(event_type, None, &risk_map);
    // Modified policy::check_approval to accept &Store
    if policy::is_high_risk(level) && !policy::check_approval(store, event_type, None, "global")? {
        return Err(error::DecapodError::ValidationError(format!("Action '{}' on '{}' is high risk and lacks approval.", event_type, id)));
    }

    let changed = broker.with_conn(&db_path, "decapod", None, event_type, |conn| {
        ensure_schema(conn)?;
        let changed = conn.execute(
            "UPDATE tasks SET status = ?1, updated_at = ?2, completed_at = CASE WHEN ?1 = 'done' THEN ?2 ELSE completed_at END WHERE id = ?3",
            rusqlite::params![new_status, ts, id],
        )?;

        let ev = TodoEvent {
            ts: ts.clone(),
            event_id: Ulid::new().to_string(),
            event_type: event_type.to_string(),
            task_id: Some(id.to_string()),
            payload,
            actor: "decapod".to_string(),
        };
        append_event(root, &ev)?;
        insert_event(conn, &ev).map_err(error::DecapodError::RusqliteError)?;
        Ok(changed)
    })?;

    Ok(serde_json::json!({
        "ts": ts,
        "cmd": event_type,
        "status": if changed > 0 { "ok" } else { "not_found" },
        "root": root.to_string_lossy(),
        "id": id,
    }))
}

fn comment_task(
    root: &Path,
    id: &str,
    comment: &str,
) -> Result<serde_json::Value, error::DecapodError> {
    let ts = now_iso();
    let broker = DbBroker::new(root);
    let db_path = todo_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "todo.comment", |conn| {
        ensure_schema(conn)?;
        // Event-only; does not mutate task row.
        let ev = TodoEvent {
            ts: ts.clone(),
            event_id: Ulid::new().to_string(),
            event_type: "task.comment".to_string(),
            task_id: Some(id.to_string()),
            payload: serde_json::json!({ "comment": comment }),
            actor: "decapod".to_string(),
        };
        append_event(root, &ev)?;
        insert_event(conn, &ev).map_err(error::DecapodError::RusqliteError)?;
        Ok(())
    })?;

    Ok(serde_json::json!({
        "ts": ts,
        "cmd": "todo.comment",
        "status": "ok",
        "root": root.to_string_lossy(),
        "id": id,
    }))
}

fn get_task(root: &Path, id: &str) -> Result<Option<Task>, error::DecapodError> {
    let broker = DbBroker::new(root);
    let db_path = todo_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "todo.get", |conn| {
        ensure_schema(conn)?;
        let mut stmt = conn.prepare("SELECT id,title,tags,owner,due,ref,status,created_at,updated_at,completed_at,dir_path,scope,parent_task_id,priority,depends_on,blocks FROM tasks WHERE id = ?1")?;
        let mut rows = stmt.query(rusqlite::params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Task {
                id: row.get(0)?,
                title: row.get(1)?,
                tags: row.get(2)?,
                owner: row.get(3)?,
                due: row.get(4)?,
                r#ref: row.get(5)?,
                status: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
                completed_at: row.get(9)?,
                dir_path: row.get(10)?,
                scope: row.get(11)?,
                parent_task_id: row.get(12)?,
                priority: row.get(13)?,
                depends_on: row.get(14)?,
                blocks: row.get(15)?,
            }))
        } else {
            Ok(None)
        }
    })
}

fn list_tasks(
    root: &Path,
    status: Option<String>,
    scope: Option<String>,
    tags: Option<String>,
    title_search: Option<String>,
    dir: Option<String>,
) -> Result<Vec<Task>, error::DecapodError> {
    let broker = DbBroker::new(root);
    let db_path = todo_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "todo.list", |conn| {
        ensure_schema(conn)?;

        let mut query = "SELECT id,title,tags,owner,due,ref,status,created_at,updated_at,completed_at,dir_path,scope,parent_task_id,priority,depends_on,blocks FROM tasks WHERE 1=1".to_string();
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
        if let Some(ts) = title_search {
            query.push_str(" AND title LIKE ?");
            params.push(Box::new(format!("%{}%", ts)));
        }
        if let Some(d) = dir {
            let abs = Path::new(&d)
                .canonicalize()
                .map_err(error::DecapodError::IoError)?
                .to_string_lossy()
                .to_string();
            query.push_str(" AND dir_path = ?");
            params.push(Box::new(abs));
        }

        query.push_str(" ORDER BY updated_at DESC");

        let mut stmt = conn.prepare(&query)?;
        let params_as_dyn: Vec<&dyn ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(&params_as_dyn[..], |row| {
            Ok(Task {
                id: row.get(0)?,
                title: row.get(1)?,
                tags: row.get(2)?,
                owner: row.get(3)?,
                due: row.get(4)?,
                r#ref: row.get(5)?,
                status: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
                completed_at: row.get(9)?,
                dir_path: row.get(10)?,
                scope: row.get(11)?,
                parent_task_id: row.get(12)?,
                priority: row.get(13)?,
                depends_on: row.get(14)?,
                blocks: row.get(15)?,
            })
        })?;

        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })
}

fn rebuild_from_events(root: &Path) -> Result<serde_json::Value, error::DecapodError> {
    let ev_path = events_path(root);
    if !ev_path.is_file() {
        // Empty store is valid; create empty DB with schema.
        let conn = connect_todo(root)?;
        ensure_schema(&conn)?;
        return Ok(serde_json::json!({
            "ts": now_iso(),
            "cmd": "todo.rebuild",
            "status": "ok",
            "root": root.to_string_lossy(),
            "events": 0,
            "note": "no events file; created empty DB"
        }));
    }

    // Rebuild into a temp DB then swap into place for atomicity.
    let tmp_db = root.join(format!(".{}.tmp", schemas::TODO_DB_NAME));
    if tmp_db.exists() {
        fs::remove_file(&tmp_db).map_err(error::DecapodError::IoError)?;
    }

    let count = rebuild_db_from_events(&ev_path, &tmp_db)?;

    // Swap
    let final_db = todo_db_path(root);
    if final_db.exists() {
        fs::remove_file(&final_db).map_err(error::DecapodError::IoError)?;
    }
    fs::rename(&tmp_db, &final_db).map_err(error::DecapodError::IoError)?;

    Ok(serde_json::json!({
        "ts": now_iso(),
        "cmd": "todo.rebuild",
        "status": "ok",
        "root": root.to_string_lossy(),
        "events": count,
    }))
}

pub fn rebuild_db_from_events(events: &Path, out_db: &Path) -> Result<u64, error::DecapodError> {
    let broker = DbBroker::new(out_db.parent().unwrap());
    
    broker.with_conn(out_db, "decapod", None, "todo.rebuild_internal", |conn| {
        ensure_schema(conn)?;

        let f = OpenOptions::new()
            .read(true)
            .open(events)
            .map_err(error::DecapodError::IoError)?;
        let reader = BufReader::new(f);

        let mut count = 0u64;
        for line in reader.lines() {
            let line = line.map_err(error::DecapodError::IoError)?;
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let ev: TodoEvent = serde_json::from_str(line).map_err(|e| {
                error::DecapodError::ValidationError(format!("Invalid JSONL event: {}", e))
            })?;
            count += 1;

            insert_event(conn, &ev).map_err(error::DecapodError::RusqliteError)?;

            match ev.event_type.as_str() {
                "task.add" => {
                    let id = ev.task_id.clone().ok_or_else(|| {
                        error::DecapodError::ValidationError("task.add missing task_id".into())
                    })?;
                    let title = ev
                        .payload
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let tags = ev
                        .payload
                        .get("tags")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let owner = ev
                        .payload
                        .get("owner")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let due = ev
                        .payload
                        .get("due")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let r#ref = ev
                        .payload
                        .get("ref")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let dir_path = ev
                        .payload
                        .get("dir_path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let scope = ev
                        .payload
                        .get("scope")
                        .and_then(|v| v.as_str())
                        .unwrap_or("root")
                        .to_string();
                    let parent_task_id = ev
                        .payload
                        .get("parent_task_id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let priority = ev
                        .payload
                        .get("priority")
                        .and_then(|v| v.as_str())
                        .unwrap_or("medium")
                        .to_string();
                    let depends_on = ev
                        .payload
                        .get("depends_on")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let blocks = ev
                        .payload
                        .get("blocks")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    conn.execute(
                        "INSERT OR REPLACE INTO tasks(id,title,tags,owner,due,ref,status,created_at,updated_at,completed_at,dir_path,scope,parent_task_id,priority,depends_on,blocks)
                         VALUES(?1,?2,?3,?4,?5,?6,'open',?7,?8,NULL,?9,?10,?11,?12,?13,?14)",
                        rusqlite::params![id, title, tags, owner, due, r#ref, ev.ts, ev.ts, dir_path, scope, parent_task_id, priority, depends_on, blocks],
                    )?;
                }
                "task.done" => {
                    let id = ev.task_id.clone().unwrap_or_default();
                    conn.execute(
                        "UPDATE tasks SET status='done', updated_at=?1, completed_at=?1 WHERE id=?2",
                        rusqlite::params![ev.ts, id],
                    )?;
                }
                "task.archive" => {
                    let id = ev.task_id.clone().unwrap_or_default();
                    conn.execute(
                        "UPDATE tasks SET status='archived', updated_at=?1 WHERE id=?2",
                        rusqlite::params![ev.ts, id],
                    )?;
                }
                "task.comment" => {}
                _ => {
                    return Err(error::DecapodError::ValidationError(format!(
                        "Unknown event_type '{}'",
                        ev.event_type
                    )));
                }
            }
        }
        Ok(count)
    })
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "todo",
        "version": "0.1.0",
        "description": "Manage TODO tasks",
        "commands": [
            { "name": "add", "parameters": ["title", "tags", "owner", "due", "ref", "dir", "priority", "depends_on", "blocks", "parent"] },
            { "name": "list", "parameters": ["status", "scope", "tags", "title_search", "dir"] },
            { "name": "get", "parameters": ["id"] },
            { "name": "done", "parameters": ["id"] },
            { "name": "archive", "parameters": ["id"] },
            { "name": "comment", "parameters": ["id", "comment"] },
            { "name": "rebuild", "parameters": [] }
        ],
        "storage": ["todo.db", "todo.events.jsonl"]
    })
}

pub fn run_todo_cli(store: &Store, cli: TodoCli) -> Result<(), error::DecapodError> {
    let root = &store.root;
    let out = match &cli.command {
        TodoCommand::Add { .. } => add_task(root, &cli.command)?,
        TodoCommand::List {
            status,
            scope,
            tags,
            title_search,
            dir,
        } => {
            let items = list_tasks(
                root,
                status.clone(),
                scope.clone(),
                tags.clone(),
                title_search.clone(),
                dir.clone(),
            )?;
            serde_json::json!({
                "ts": now_iso(),
                "cmd": "todo.list",
                "status": "ok",
                "root": root.to_string_lossy(),
                "items": items,
            })
        }
        TodoCommand::Get { id } => {
            let t = get_task(root, id)?;
            serde_json::json!({
                "ts": now_iso(),
                "cmd": "todo.get",
                "status": if t.is_some() { "ok" } else { "not_found" },
                "root": root.to_string_lossy(),
                "item": t,
            })
        }
        TodoCommand::Done { id } => {
            update_status(store, id, "done", "task.done", serde_json::json!({}))?
        }
        TodoCommand::Archive { id } => {
            update_status(store, id, "archived", "task.archive", serde_json::json!({}))?
        }
        TodoCommand::Comment { id, comment } => comment_task(root, id, comment)?,
        TodoCommand::Rebuild => rebuild_from_events(root)?,
    };

    match cli.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
        }
        OutputFormat::Text => match &cli.command {
            TodoCommand::List { .. } => {
                let items = out.get("items").cloned().unwrap_or(JsonValue::Null);
                if let Some(arr) = items.as_array() {
                    if arr.is_empty() {
                        println!("No tasks found.");
                        return Ok(());
                    }
                    println!("Tasks (root: {}):", root.display());
                    for v in arr {
                        let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("?");
                        let status = v.get("status").and_then(|x| x.as_str()).unwrap_or("?");
                        let prio = v.get("priority").and_then(|x| x.as_str()).unwrap_or("?");
                        let title = v.get("title").and_then(|x| x.as_str()).unwrap_or("");
                        let scope = v.get("scope").and_then(|x| x.as_str()).unwrap_or("root");
                        println!("- {} [{}|{}|{}] {}", id, status, prio, scope, title);
                    }
                } else {
                    println!("No tasks found.");
                }
            }
            _ => {
                // For non-list commands, text mode prints the minimal envelope.
                println!("{}", serde_json::to_string(&out).unwrap());
            }
        },
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_todo_lifecycle() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        initialize_todo_db(&root).unwrap();

        // 1. Add task
        let add_args = TodoCommand::Add {
            title: "Test task".to_string(),
            tags: "tag1".to_string(),
            owner: "arx".to_string(),
            due: None,
            r#ref: "".to_string(),
            dir: Some(tmp.path().to_string_lossy().to_string()),
            priority: "high".to_string(),
            depends_on: "".to_string(),
            blocks: "".to_string(),
            parent: None,
        };
        let res = add_task(&root, &add_args).unwrap();
        let task_id = res.get("id").unwrap().as_str().unwrap();
        assert!(task_id.contains("_"));

        // 2. Get task
        let task = get_task(&root, task_id).unwrap().expect("Task not found");
        assert_eq!(task.title, "Test task");
        assert_eq!(task.status, "open");

        // 3. Mark done
        update_status(&root, task_id, "done", "task.done", serde_json::json!({})).unwrap();
        let task = get_task(&root, task_id).unwrap().unwrap();
        assert_eq!(task.status, "done");

        // 4. List tasks
        let tasks = list_tasks(&root, Some("done".to_string()), None, None, None, None).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, task_id);
    }

    #[test]
    fn test_todo_rebuild() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        initialize_todo_db(&root).unwrap();

        // Add some tasks
        for i in 0..3 {
            let add_args = TodoCommand::Add {
                title: format!("Task {}", i),
                tags: "".to_string(),
                owner: "".to_string(),
                due: None,
                r#ref: "".to_string(),
                dir: Some(tmp.path().to_string_lossy().to_string()),
                priority: "medium".to_string(),
                depends_on: "".to_string(),
                blocks: "".to_string(),
                parent: None,
            };
            add_task(&root, &add_args).unwrap();
        }

        // Corrupt/Delete DB
        let db_path = todo_db_path(&root);
        fs::remove_file(&db_path).unwrap();

        // Rebuild
        rebuild_from_events(&root).unwrap();

        // Verify
        let tasks = list_tasks(&root, None, None, None, None, None).unwrap();
        assert_eq!(tasks.len(), 3);
    }
}
