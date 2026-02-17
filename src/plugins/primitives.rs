use crate::core::broker::DbBroker;
use crate::core::error;
use crate::core::schemas;
use crate::core::store::Store;
use crate::plugins::todo;
use clap::{Parser, Subcommand};
use rusqlite::params;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[clap(
    name = "primitives",
    about = "Markdown-native primitive export and round-trip validation"
)]
pub struct PrimitivesCli {
    #[clap(subcommand)]
    pub command: PrimitivesCommand,
}

#[derive(Subcommand, Debug)]
pub enum PrimitivesCommand {
    /// Export tasks/projects/decisions/lessons/people as markdown primitives.
    Export {
        #[clap(long)]
        out: Option<PathBuf>,
        #[clap(long, default_value_t = true)]
        views: bool,
    },
    /// Validate markdown <-> control-plane round-trip integrity.
    Validate {
        #[clap(long)]
        path: Option<PathBuf>,
    },
}

#[derive(Debug, Clone)]
struct MemoryNode {
    id: String,
    node_type: String,
    title: String,
    status: String,
    priority: String,
    body: String,
    tags: String,
    created_at: String,
    updated_at: String,
}

pub fn run_primitives_cli(store: &Store, cli: PrimitivesCli) -> Result<(), error::DecapodError> {
    match cli.command {
        PrimitivesCommand::Export { out, views } => {
            let out_dir = out.unwrap_or_else(|| default_export_dir(&store.root));
            let count = export_primitives(store, &out_dir, views)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ts": crate::core::time::now_epoch_z(),
                    "cmd": "primitives.export",
                    "status": "ok",
                    "path": out_dir,
                    "files_written": count
                }))
                .unwrap()
            );
        }
        PrimitivesCommand::Validate { path } => {
            let base = path.unwrap_or_else(|| default_export_dir(&store.root));
            validate_round_trip(store, &base)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ts": crate::core::time::now_epoch_z(),
                    "cmd": "primitives.validate",
                    "status": "ok",
                    "path": base
                }))
                .unwrap()
            );
        }
    }
    Ok(())
}

pub fn default_export_dir(store_root: &Path) -> PathBuf {
    store_root.join("generated").join("primitives")
}

pub fn export_primitives(
    store: &Store,
    out_dir: &Path,
    include_views: bool,
) -> Result<usize, error::DecapodError> {
    fs::create_dir_all(out_dir).map_err(error::DecapodError::IoError)?;
    let tasks_dir = out_dir.join("tasks");
    let projects_dir = out_dir.join("projects");
    let decisions_dir = out_dir.join("decisions");
    let lessons_dir = out_dir.join("lessons");
    let people_dir = out_dir.join("people");
    let views_dir = out_dir.join("views");
    for dir in [
        &tasks_dir,
        &projects_dir,
        &decisions_dir,
        &lessons_dir,
        &people_dir,
    ] {
        fs::create_dir_all(dir).map_err(error::DecapodError::IoError)?;
    }
    if include_views {
        fs::create_dir_all(&views_dir).map_err(error::DecapodError::IoError)?;
    }

    let tasks = todo::list_tasks(&store.root, None, None, None, None, None)?;
    let mut files_written = 0usize;
    for t in &tasks {
        let content = render_task_markdown(t);
        fs::write(tasks_dir.join(format!("{}.md", t.id)), content)
            .map_err(error::DecapodError::IoError)?;
        files_written += 1;
    }

    let memory_nodes =
        list_memory_nodes_by_types(&store.root, &["project", "decision", "lesson", "person"])?;
    for node in &memory_nodes {
        let dir = match node.node_type.as_str() {
            "project" => &projects_dir,
            "decision" => &decisions_dir,
            "lesson" => &lessons_dir,
            "person" => &people_dir,
            _ => continue,
        };
        fs::write(
            dir.join(format!("{}.md", node.id)),
            render_node_markdown(node),
        )
        .map_err(error::DecapodError::IoError)?;
        files_written += 1;
    }

    if include_views {
        files_written += write_views(&views_dir, &tasks)?;
    }

    Ok(files_written)
}

pub fn validate_roundtrip_gate(store: &Store) -> Result<(), error::DecapodError> {
    let tmp = std::env::temp_dir().join(format!(
        "decapod-primitives-{}",
        crate::core::time::new_event_id()
    ));
    export_primitives(store, &tmp, true)?;
    let result = validate_round_trip(store, &tmp);
    let _ = fs::remove_dir_all(&tmp);
    result
}

pub fn validate_round_trip(store: &Store, base: &Path) -> Result<(), error::DecapodError> {
    for (dir, expected_type) in [
        ("tasks", "task"),
        ("projects", "project"),
        ("decisions", "decision"),
        ("lessons", "lesson"),
        ("people", "person"),
    ] {
        let folder = base.join(dir);
        if !folder.exists() {
            return Err(error::DecapodError::ValidationError(format!(
                "missing primitive directory: {}",
                folder.display()
            )));
        }
        for entry in fs::read_dir(&folder).map_err(error::DecapodError::IoError)? {
            let entry = entry.map_err(error::DecapodError::IoError)?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let raw = fs::read_to_string(&path).map_err(error::DecapodError::IoError)?;
            let parsed_id = extract_kv(&raw, "id").ok_or_else(|| {
                error::DecapodError::ValidationError(format!(
                    "primitive missing id field: {}",
                    path.display()
                ))
            })?;
            if expected_type == "task" {
                let found = todo::get_task(&store.root, &parsed_id)?;
                if found.is_none() {
                    return Err(error::DecapodError::ValidationError(format!(
                        "task primitive not found in todo db: {}",
                        parsed_id
                    )));
                }
            } else if !memory_node_exists(&store.root, &parsed_id, expected_type)? {
                return Err(error::DecapodError::ValidationError(format!(
                    "{} primitive not found in federation db: {}",
                    expected_type, parsed_id
                )));
            }
        }
    }
    Ok(())
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "primitives",
        "version": "0.1.0",
        "description": "Markdown-only primitive layer and round-trip validation",
        "commands": [
            { "name": "export", "parameters": ["out", "views"] },
            { "name": "validate", "parameters": ["path"] }
        ],
        "storage": [
            "generated/primitives/tasks/",
            "generated/primitives/projects/",
            "generated/primitives/decisions/",
            "generated/primitives/lessons/",
            "generated/primitives/people/",
            "generated/primitives/views/"
        ]
    })
}

fn write_views(views_dir: &Path, tasks: &[todo::Task]) -> Result<usize, error::DecapodError> {
    let mut count = 0usize;
    fs::write(views_dir.join("all-tasks.md"), render_all_tasks_view(tasks))
        .map_err(error::DecapodError::IoError)?;
    count += 1;
    fs::write(
        views_dir.join("blocked-items.md"),
        render_blocked_view(tasks),
    )
    .map_err(error::DecapodError::IoError)?;
    count += 1;
    fs::write(
        views_dir.join("by-project.md"),
        render_by_project_view(tasks),
    )
    .map_err(error::DecapodError::IoError)?;
    count += 1;
    fs::write(views_dir.join("by-owner.md"), render_by_owner_view(tasks))
        .map_err(error::DecapodError::IoError)?;
    count += 1;
    fs::write(views_dir.join("backlog.md"), render_backlog_view(tasks))
        .map_err(error::DecapodError::IoError)?;
    count += 1;
    Ok(count)
}

fn render_task_markdown(t: &todo::Task) -> String {
    let owners = if t.owners.is_empty() {
        "-".to_string()
    } else {
        t.owners
            .iter()
            .map(|o| format!("{} ({})", o.agent_id, o.claim_type))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "# Task: {}\n\n- id: {}\n- status: {}\n- priority: {}\n- owner: {}\n- assigned_to: {}\n- owners: {}\n- depends_on: {}\n- blocks: {}\n- category: {}\n- scope: {}\n- created_at: {}\n- updated_at: {}\n\n## Description\n{}\n\n## Tags\n{}\n",
        t.title,
        t.id,
        t.status,
        t.priority,
        empty_dash(&t.owner),
        empty_dash(&t.assigned_to),
        owners,
        empty_dash(&t.depends_on),
        empty_dash(&t.blocks),
        empty_dash(&t.category),
        t.scope,
        t.created_at,
        t.updated_at,
        if t.description.trim().is_empty() {
            "-"
        } else {
            t.description.trim()
        },
        if t.tags.trim().is_empty() {
            "-"
        } else {
            t.tags.trim()
        },
    )
}

fn render_node_markdown(n: &MemoryNode) -> String {
    format!(
        "# {}: {}\n\n- id: {}\n- type: {}\n- status: {}\n- priority: {}\n- tags: {}\n- created_at: {}\n- updated_at: {}\n\n## Body\n{}\n",
        capitalize(&n.node_type),
        n.title,
        n.id,
        n.node_type,
        n.status,
        n.priority,
        empty_dash(&n.tags),
        n.created_at,
        n.updated_at,
        if n.body.trim().is_empty() {
            "-"
        } else {
            n.body.trim()
        },
    )
}

fn render_all_tasks_view(tasks: &[todo::Task]) -> String {
    let mut out = String::from(
        "# All Tasks\n\n| ID | Title | Status | Priority | Owner |\n|---|---|---|---|---|\n",
    );
    for t in tasks {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            t.id,
            escape_table(&t.title),
            t.status,
            t.priority,
            escape_table(if t.owner.trim().is_empty() {
                &t.assigned_to
            } else {
                &t.owner
            }),
        ));
    }
    out
}

fn render_blocked_view(tasks: &[todo::Task]) -> String {
    let mut out = String::from("# Blocked Items\n\n");
    for t in tasks.iter().filter(|t| t.status == "blocked") {
        out.push_str(&format!("- [{}] {} ({})\n", t.id, t.title, t.priority));
    }
    if !out.contains("- [") {
        out.push_str("- none\n");
    }
    out
}

fn render_by_project_view(tasks: &[todo::Task]) -> String {
    let mut groups: BTreeMap<String, Vec<&todo::Task>> = BTreeMap::new();
    for t in tasks {
        let key = if !t.component.trim().is_empty() {
            t.component.clone()
        } else if !t.category.trim().is_empty() {
            t.category.clone()
        } else {
            "uncategorized".to_string()
        };
        groups.entry(key).or_default().push(t);
    }
    let mut out = String::from("# By Project\n\n");
    for (project, items) in groups {
        out.push_str(&format!("## {}\n", project));
        for t in items {
            out.push_str(&format!("- [{}] {} ({})\n", t.id, t.title, t.status));
        }
        out.push('\n');
    }
    out
}

fn render_by_owner_view(tasks: &[todo::Task]) -> String {
    let mut groups: BTreeMap<String, Vec<&todo::Task>> = BTreeMap::new();
    for t in tasks {
        let owner = if !t.assigned_to.trim().is_empty() {
            t.assigned_to.clone()
        } else if !t.owner.trim().is_empty() {
            t.owner.clone()
        } else {
            "unassigned".to_string()
        };
        groups.entry(owner).or_default().push(t);
    }
    let mut out = String::from("# By Owner\n\n");
    for (owner, items) in groups {
        out.push_str(&format!("## {}\n", owner));
        for t in items {
            out.push_str(&format!("- [{}] {} ({})\n", t.id, t.title, t.status));
        }
        out.push('\n');
    }
    out
}

fn render_backlog_view(tasks: &[todo::Task]) -> String {
    let mut out = String::from("# Backlog\n\n");
    for t in tasks.iter().filter(|t| t.status == "open") {
        out.push_str(&format!("- [{}] {} ({})\n", t.id, t.title, t.priority));
    }
    if !out.contains("- [") {
        out.push_str("- none\n");
    }
    out
}

fn list_memory_nodes_by_types(
    root: &Path,
    types: &[&str],
) -> Result<Vec<MemoryNode>, error::DecapodError> {
    let db_path = root.join(schemas::FEDERATION_DB_NAME);
    if !db_path.exists() {
        return Ok(Vec::new());
    }
    let broker = DbBroker::new(root);
    broker.with_conn(
        &db_path,
        "primitives",
        None,
        "primitives.list_nodes",
        |conn| {
            let placeholders = std::iter::repeat_n("?", types.len())
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!(
                "SELECT id, node_type, title, status, priority, body, tags, created_at, updated_at
             FROM nodes WHERE node_type IN ({}) ORDER BY updated_at DESC",
                placeholders
            );
            let mut stmt = conn.prepare(&sql)?;
            let params: Vec<&dyn rusqlite::ToSql> =
                types.iter().map(|t| t as &dyn rusqlite::ToSql).collect();
            let rows = stmt.query_map(rusqlite::params_from_iter(params), |row| {
                Ok(MemoryNode {
                    id: row.get(0)?,
                    node_type: row.get(1)?,
                    title: row.get(2)?,
                    status: row.get(3)?,
                    priority: row.get(4)?,
                    body: row.get(5)?,
                    tags: row.get(6)?,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            })?;

            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        },
    )
}

fn memory_node_exists(root: &Path, id: &str, node_type: &str) -> Result<bool, error::DecapodError> {
    let db_path = root.join(schemas::FEDERATION_DB_NAME);
    if !db_path.exists() {
        return Ok(false);
    }
    let broker = DbBroker::new(root);
    broker.with_conn(
        &db_path,
        "primitives",
        None,
        "primitives.node_exists",
        |conn| {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM nodes WHERE id = ?1 AND node_type = ?2",
                params![id, node_type],
                |row| row.get(0),
            )?;
            Ok(count > 0)
        },
    )
}

fn extract_kv(raw: &str, key: &str) -> Option<String> {
    let prefix = format!("- {}:", key);
    raw.lines().find_map(|line| {
        line.trim()
            .strip_prefix(&prefix)
            .map(|v| v.trim().to_string())
    })
}

fn empty_dash(v: &str) -> &str {
    if v.trim().is_empty() { "-" } else { v.trim() }
}

fn escape_table(v: &str) -> String {
    v.replace('|', "\\|")
}

fn capitalize(v: &str) -> String {
    let mut chars = v.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    format!("{}{}", first.to_ascii_uppercase(), chars.as_str())
}
