use crate::core::error;
use crate::core::store::Store;
use crate::plugins::todo;
use clap::{Parser, Subcommand};
use std::path::Path;
use std::process::Command;

#[derive(Parser, Debug)]
#[clap(
    name = "workflow",
    about = "Workflow automation and discovery helpers for agent loops"
)]
pub struct WorkflowCli {
    #[clap(subcommand)]
    pub command: WorkflowCommand,
}

#[derive(Subcommand, Debug)]
pub enum WorkflowCommand {
    /// Execute one automation loop (trigger -> task -> context -> execution -> lesson).
    Run {
        #[clap(long)]
        agent: String,
        #[clap(long)]
        title: String,
        #[clap(long, default_value = "medium")]
        priority: String,
        #[clap(long, default_value = "")]
        tags: String,
        #[clap(long, default_value_t = 1)]
        max_tasks: usize,
        #[clap(long)]
        lesson: Option<String>,
        #[clap(long, default_value_t = true)]
        autoclose: bool,
    },
    /// Suggest discovery opportunities from open work and stale ownership.
    Discover {
        #[clap(long, default_value_t = 10)]
        limit: usize,
    },
}

pub fn run_workflow_cli(store: &Store, cli: WorkflowCli) -> Result<(), error::DecapodError> {
    match cli.command {
        WorkflowCommand::Run {
            agent,
            title,
            priority,
            tags,
            max_tasks,
            lesson,
            autoclose,
        } => run_workflow(
            store, &agent, &title, &priority, &tags, max_tasks, lesson, autoclose,
        ),
        WorkflowCommand::Discover { limit } => discover(store, limit),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_workflow(
    store: &Store,
    agent: &str,
    title: &str,
    priority: &str,
    tags: &str,
    max_tasks: usize,
    lesson: Option<String>,
    autoclose: bool,
) -> Result<(), error::DecapodError> {
    let mut add_args = vec![
        "todo",
        "--format",
        "json",
        "add",
        title,
        "--priority",
        priority,
        "--owner",
        agent,
    ];
    if !tags.trim().is_empty() {
        add_args.push("--tags");
        add_args.push(tags);
    }
    let add = run_decapod_json(&store.root, &add_args)?;
    let task_id = add
        .get("task")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            error::DecapodError::ValidationError("workflow run failed: missing task id".to_string())
        })?
        .to_string();

    let max_tasks_s = max_tasks.to_string();
    let mut worker_args = vec![
        "todo",
        "--format",
        "json",
        "worker-run",
        "--agent",
        agent,
        "--task-id",
        &task_id,
        "--max-tasks",
        &max_tasks_s,
    ];
    if autoclose {
        worker_args.push("--autoclose");
    }
    if let Some(ref lesson_text) = lesson {
        if !lesson_text.trim().is_empty() {
            worker_args.push("--lesson");
            worker_args.push(lesson_text);
        }
    }
    let _worker = run_decapod_json(&store.root, &worker_args)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "ts": crate::core::time::now_epoch_z(),
            "cmd": "workflow.run",
            "status": "ok",
            "task_id": task_id,
            "agent": agent
        }))
        .unwrap()
    );
    Ok(())
}

fn discover(store: &Store, limit: usize) -> Result<(), error::DecapodError> {
    let tasks = todo::list_tasks(
        &store.root,
        Some("open".to_string()),
        None,
        None,
        None,
        None,
    )?;
    let mut suggestions = Vec::new();
    for t in tasks.iter().take(limit) {
        let opportunity = if t.priority == "high" {
            "promote to heartbeat worker loop"
        } else if t.category == "docs" {
            "batch with documentation reflex"
        } else if t.category == "ci" {
            "attach cron suggestion for recurring validation"
        } else {
            "queue for autonomous backlog sweep"
        };
        suggestions.push(serde_json::json!({
            "task_id": t.id,
            "title": t.title,
            "priority": t.priority,
            "suggestion": opportunity
        }));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "ts": crate::core::time::now_epoch_z(),
            "cmd": "workflow.discover",
            "status": "ok",
            "suggestions": suggestions
        }))
        .unwrap()
    );
    Ok(())
}

fn run_decapod_json(
    store_root: &Path,
    args: &[&str],
) -> Result<serde_json::Value, error::DecapodError> {
    let exe = std::env::current_exe().map_err(error::DecapodError::IoError)?;
    let output = Command::new(exe)
        .current_dir(
            store_root
                .parent()
                .and_then(|p| p.parent())
                .unwrap_or_else(|| Path::new(".")),
        )
        .args(args)
        .output()
        .map_err(error::DecapodError::IoError)?;
    if !output.status.success() {
        return Err(error::DecapodError::ValidationError(format!(
            "workflow command failed: decapod {} -> {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    serde_json::from_slice::<serde_json::Value>(&output.stdout).map_err(|err| {
        error::DecapodError::ValidationError(format!(
            "workflow command did not return JSON: {}",
            err
        ))
    })
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "workflow",
        "version": "0.1.0",
        "description": "Workflow automation and discovery command group",
        "commands": [
            { "name": "run", "parameters": ["agent", "title", "priority", "tags", "max_tasks", "lesson", "autoclose"] },
            { "name": "discover", "parameters": ["limit"] }
        ],
        "storage": ["todo.db", "todo.events.jsonl", "knowledge.db"]
    })
}
