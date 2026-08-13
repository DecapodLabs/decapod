//! Todo projection over Decapod's Dactyl storage boundary.
//!
//! This adapter deliberately speaks only SQL and Dactyl's normalized result
//! contract. Cloud authentication and repository scope are carried by the
//! versioned storage context; no todo request adds a backend, tenant, or
//! provider-specific query input.

use crate::core::backend::StorageContext;
use crate::core::dactyl::{DactylBridge, OperationResult};
use crate::core::storage::{Task, TodoStore};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use dactyl_db::{Operation, Parameter, Rows};

const TASK_COLUMNS: &str = "repo_id, id, hash, title, description, status, assigned_to AS assignee, scope, dir_path, priority, category, tags, created_at, updated_at, version";

/// Dactyl-backed todo store used by the cloud command path.
pub struct DactylTodoStore {
    context: StorageContext,
    repository: String,
}

impl DactylTodoStore {
    pub fn new(context: StorageContext, repository: impl Into<String>) -> Self {
        Self {
            context,
            repository: repository.into(),
        }
    }

    fn bridge(&self) -> Result<DactylBridge> {
        Ok(DactylBridge::from_storage_context(
            &self.context,
            dactyl_db::AccessMode::ReadWrite,
        )?)
    }

    fn list_sql() -> String {
        format!("SELECT {TASK_COLUMNS} FROM tasks ORDER BY updated_at DESC, id ASC")
    }

    fn get_sql() -> String {
        format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id = $1")
    }

    fn task_from_rows(&self, rows: Rows) -> Result<Task> {
        rows.as_slice()
            .first()
            .ok_or_else(|| anyhow!("Dactyl task mutation committed no observable task row"))
            .and_then(|row| task_from_row(row, &self.repository))
    }

    fn require_write(result: &OperationResult, operation: &str) -> Result<()> {
        match result {
            OperationResult::Write(write) if write.affected_rows == 1 => Ok(()),
            OperationResult::Write(write) => Err(anyhow!(
                "Dactyl {operation} changed {} rows; expected exactly one (state conflict or missing task)",
                write.affected_rows
            )),
            OperationResult::Rows(_) => Err(anyhow!(
                "Dactyl {operation} returned rows where a write result was required"
            )),
        }
    }
}

#[async_trait]
impl TodoStore for DactylTodoStore {
    async fn list_tasks(&self) -> Result<Vec<Task>> {
        let bridge = self.bridge()?;
        let rows = bridge.read(&Self::list_sql(), &[])?;
        rows.as_slice()
            .iter()
            .map(|row| task_from_row(row, &self.repository))
            .collect()
    }

    async fn add_task(&self, mut task: Task, _actor: String, _intent: String) -> Result<Task> {
        if task.id.trim().is_empty() {
            task.id = new_task_id();
        }
        if task.hash.trim().is_empty() {
            task.hash = task_hash(&task.id);
        }
        if task.status.trim().is_empty() {
            task.status = "open".to_string();
        }

        let bridge = self.bridge()?;
        let result = bridge.atomic(&[
            Operation::write(
                "INSERT INTO tasks (id, hash, title, description, tags, owner, status, dir_path, scope, priority, category, assigned_to) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
                vec![
                    task.id.clone().into(),
                    task.hash.clone().into(),
                    task.title.clone().into(),
                    task.description.clone().unwrap_or_default().into(),
                    task.tags.join(",").into(),
                    Parameter::Text(String::new()),
                    task.status.clone().into(),
                    task.dir_path.clone().into(),
                    task.scope.clone().into(),
                    task.priority.clone().into(),
                    task.category.clone().into(),
                    Parameter::Text(String::new()),
                ],
            ),
            Operation::read(Self::get_sql(), vec![task.id.clone().into()]),
        ])?;
        let mut results = result.results;
        let observation = results
            .pop()
            .ok_or_else(|| anyhow!("Dactyl add returned no observation result"))?;
        let write = results
            .first()
            .ok_or_else(|| anyhow!("Dactyl add returned no write result"))?;
        Self::require_write(write, "add")?;
        match observation {
            OperationResult::Rows(rows) => self.task_from_rows(rows),
            OperationResult::Write(_) => Err(anyhow!(
                "Dactyl add returned a write result for its task observation"
            )),
        }
    }

    async fn claim_task(&self, id: &str, actor: String) -> Result<Task> {
        let bridge = self.bridge()?;
        let result = bridge.atomic(&[
            Operation::write(
                "UPDATE tasks SET status = 'in_progress', assigned_to = $1, assigned_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP WHERE id = $2 AND status IN ('open', 'pending') AND (assigned_to = '' OR assigned_to IS NULL)",
                vec![actor.into(), id.into()],
            ),
            Operation::read(Self::get_sql(), vec![id.into()]),
        ])?;
        let mut results = result.results;
        let observation = results
            .pop()
            .ok_or_else(|| anyhow!("Dactyl claim returned no observation result"))?;
        let write = results
            .first()
            .ok_or_else(|| anyhow!("Dactyl claim returned no write result"))?;
        Self::require_write(write, "claim")?;
        match observation {
            OperationResult::Rows(rows) => self.task_from_rows(rows),
            OperationResult::Write(_) => Err(anyhow!(
                "Dactyl claim returned a write result for its task observation"
            )),
        }
    }

    async fn complete_task(&self, id: &str, actor: String, _resolution: String) -> Result<Task> {
        let bridge = self.bridge()?;
        let result = bridge.atomic(&[
            Operation::write(
                "UPDATE tasks SET status = 'completed', completed_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP WHERE id = $1 AND status = 'in_progress' AND assigned_to = $2",
                vec![id.into(), actor.into()],
            ),
            Operation::read(Self::get_sql(), vec![id.into()]),
        ])?;
        let mut results = result.results;
        let observation = results
            .pop()
            .ok_or_else(|| anyhow!("Dactyl complete returned no observation result"))?;
        let write = results
            .first()
            .ok_or_else(|| anyhow!("Dactyl complete returned no write result"))?;
        Self::require_write(write, "complete")?;
        match observation {
            OperationResult::Rows(rows) => self.task_from_rows(rows),
            OperationResult::Write(_) => Err(anyhow!(
                "Dactyl complete returned a write result for its task observation"
            )),
        }
    }
}

fn task_from_row(row: &dactyl_db::Row, repository: &str) -> Result<Task> {
    let created_at = parse_timestamp(row.get::<_, Option<String>>("created_at")?.as_deref());
    let updated_at = parse_timestamp(row.get::<_, Option<String>>("updated_at")?.as_deref());
    let tags = row
        .get::<_, Option<String>>("tags")?
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(str::to_string)
        .collect();

    Ok(Task {
        id: row.get("id")?,
        repo_id: row
            .get::<_, Option<String>>("repo_id")?
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| repository.to_string()),
        hash: row.get("hash")?,
        title: row.get("title")?,
        description: row.get::<_, Option<String>>("description")?,
        status: row.get("status")?,
        assignee: row.get::<_, Option<String>>("assignee")?,
        scope: row.get("scope")?,
        dir_path: row.get("dir_path")?,
        priority: row.get("priority")?,
        category: row.get("category")?,
        tags,
        created_at: created_at.unwrap_or_else(Utc::now),
        updated_at: updated_at.unwrap_or_else(Utc::now),
        version: row.get::<_, Option<i64>>("version")?.unwrap_or(1) as i32,
    })
}

fn parse_timestamp(value: Option<&str>) -> Option<DateTime<Utc>> {
    value
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc))
}

fn new_task_id() -> String {
    format!(
        "todo_{}",
        crate::core::ulid::new_ulid()
            .to_string()
            .to_ascii_lowercase()
    )
}

fn task_hash(id: &str) -> String {
    id.split_once('_')
        .map(|(_, suffix)| suffix)
        .unwrap_or(id)
        .chars()
        .take(6)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::dactyl::DactylBridge;

    fn local_store() -> Option<(DactylTodoStore, tempfile::TempDir)> {
        let tempdir = tempfile::tempdir().expect("Dactyl tempdir");
        let path = tempdir.path().join("tasks.db");
        std::fs::File::create(&path).expect("Dactyl SQLite file");
        let bridge = match DactylBridge::open_local(&path, dactyl_db::AccessMode::ReadWrite) {
            Ok(bridge) => bridge,
            Err(crate::core::error::DecapodError::DactylError(error))
                if error.adapter_code() == Some("sqlite_runtime_unavailable") =>
            {
                return None;
            }
            Err(error) => panic!("Dactyl file bridge: {error}"),
        };
        bridge
            .write(
                "CREATE TABLE tasks (repo_id TEXT NOT NULL DEFAULT 'DecapodLabs/decapod', id TEXT PRIMARY KEY, hash TEXT NOT NULL, title TEXT NOT NULL, description TEXT, status TEXT NOT NULL, assigned_to TEXT, scope TEXT NOT NULL, dir_path TEXT NOT NULL, priority TEXT NOT NULL, category TEXT NOT NULL, tags TEXT, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP, updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP, version INTEGER NOT NULL DEFAULT 1, owner TEXT, assigned_at TEXT, completed_at TEXT)",
                &[],
            )
            .expect("task schema");
        drop(bridge);
        let context =
            StorageContext::from_route(crate::core::backend::BackendRoute::Local { path }, None)
                .expect("local context");
        Some((
            DactylTodoStore::new(context, "DecapodLabs/decapod"),
            tempdir,
        ))
    }

    fn task(title: &str) -> Task {
        let now = Utc::now();
        Task {
            id: String::new(),
            repo_id: "DecapodLabs/decapod".to_string(),
            hash: String::new(),
            title: title.to_string(),
            description: Some("description".to_string()),
            status: "open".to_string(),
            assignee: None,
            scope: "repo".to_string(),
            dir_path: "".to_string(),
            priority: "medium".to_string(),
            category: "bugs".to_string(),
            tags: vec!["dactyl".to_string()],
            created_at: now,
            updated_at: now,
            version: 1,
        }
    }

    #[tokio::test]
    async fn mutations_use_dactyl_atomic_write_and_observation() {
        let Some((store, _tempdir)) = local_store() else {
            eprintln!("skipping Dactyl todo adapter test: host SQLite runtime unavailable");
            return;
        };
        let added = store
            .add_task(
                task("cloud task"),
                "agent-a".to_string(),
                "intent".to_string(),
            )
            .await
            .expect("add");
        assert_eq!(added.status, "open");
        assert_eq!(store.list_tasks().await.expect("list").len(), 1);

        let claimed = store
            .claim_task(&added.id, "agent-a".to_string())
            .await
            .expect("claim");
        assert_eq!(claimed.status, "in_progress");
        assert_eq!(claimed.assignee.as_deref(), Some("agent-a"));

        let completed = store
            .complete_task(&added.id, "agent-a".to_string(), String::new())
            .await
            .expect("complete");
        assert_eq!(completed.status, "completed");
    }

    #[tokio::test]
    async fn stale_claim_is_a_conflict_and_does_not_change_the_task() {
        let Some((store, _tempdir)) = local_store() else {
            eprintln!("skipping Dactyl todo adapter test: host SQLite runtime unavailable");
            return;
        };
        let added = store
            .add_task(
                task("only one winner"),
                "agent-a".to_string(),
                "intent".to_string(),
            )
            .await
            .expect("add");
        store
            .claim_task(&added.id, "agent-a".to_string())
            .await
            .expect("first claim");
        let error = store
            .claim_task(&added.id, "agent-b".to_string())
            .await
            .expect_err("second claim must conflict");
        assert!(error.to_string().contains("state conflict"));
        assert_eq!(
            store.list_tasks().await.expect("list")[0]
                .assignee
                .as_deref(),
            Some("agent-a")
        );
    }
}
