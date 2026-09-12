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
use chrono::{DateTime, NaiveDateTime, SecondsFormat, Utc};
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

    fn event_insert_sql() -> &'static str {
        "WITH event_params(event_id, event_ts, task_id, event_type, event_payload, event_actor) AS (
             VALUES ($1, $2, $3, $4, $5, $6)
         )
         INSERT INTO events (event_id, ts, seq, stream, subject_kind, subject_id, event_type, payload, actor)
         SELECT event_params.event_id,
                event_params.event_ts,
                COALESCE((SELECT MAX(seq) FROM events WHERE stream = 'todo'), 0) + 1,
                'todo',
                'task',
                event_params.task_id,
                event_params.event_type,
                event_params.event_payload,
                event_params.event_actor
         FROM event_params
         WHERE EXISTS (
             SELECT 1 FROM tasks
             WHERE tasks.id = event_params.task_id
               AND tasks.updated_at = event_params.event_ts
         )"
    }

    fn operation_timestamp() -> String {
        // The timestamp is also the transaction marker used by the portable
        // state-plus-event batch. Millisecond precision allowed a fast stale
        // mutation to observe the winning mutation's marker after Dactyl's
        // positional SQLite binding was tightened in v0.10.0.
        Utc::now().to_rfc3339_opts(SecondsFormat::Nanos, true)
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

    async fn get_task(&self, id: &str) -> Result<Option<Task>> {
        let bridge = self.bridge()?;
        let rows = bridge.read(&Self::get_sql(), &[id.into()])?;
        rows.as_slice()
            .first()
            .map(|row| task_from_row(row, &self.repository))
            .transpose()
    }

    async fn add_task(&self, mut task: Task, actor: String, _intent: String) -> Result<Task> {
        if task.id.trim().is_empty() {
            task.id = new_task_id();
        }
        if task.hash.trim().is_empty() {
            task.hash = task_hash(&task.id);
        }
        if task.status.trim().is_empty() {
            task.status = "open".to_string();
        }
        if task.repo_id.trim().is_empty() {
            task.repo_id = self.repository.clone();
        }

        let ts = Self::operation_timestamp();
        let event_id = crate::core::ulid::new_ulid().to_string();
        let payload = serde_json::json!({
            "title": task.title.clone(),
            "status": task.status.clone(),
        })
        .to_string();
        let bridge = self.bridge()?;
        let result = bridge.atomic(&[
            Operation::write(
                "INSERT INTO tasks (repo_id, id, hash, title, description, tags, owner, status, dir_path, scope, priority, category, assigned_to, created_at, updated_at, version) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $14, 1)",
                vec![
                    task.repo_id.clone().into(),
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
                    ts.clone().into(),
                ],
            ),
            Operation::write(
                Self::event_insert_sql(),
                vec![
                    event_id.into(),
                    ts.into(),
                    task.id.clone().into(),
                    "task.add".into(),
                    payload.into(),
                    actor.into(),
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
        let event = results
            .get(1)
            .ok_or_else(|| anyhow!("Dactyl add returned no event result"))?;
        Self::require_write(event, "add event")?;
        match observation {
            OperationResult::Rows(rows) => self.task_from_rows(rows),
            OperationResult::Write(_) => Err(anyhow!(
                "Dactyl add returned a write result for its task observation"
            )),
        }
    }

    async fn claim_task(&self, id: &str, actor: String) -> Result<Task> {
        let ts = Self::operation_timestamp();
        let event_id = crate::core::ulid::new_ulid().to_string();
        let payload = serde_json::json!({ "assigned_to": actor }).to_string();
        let bridge = self.bridge()?;
        let result = bridge.atomic(&[
            Operation::write(
                "UPDATE tasks SET status = 'in_progress', assigned_to = $1, assigned_at = $2, updated_at = $2, version = COALESCE(version, 1) + 1 WHERE id = $3 AND status IN ('open', 'pending') AND (assigned_to = '' OR assigned_to IS NULL)",
                vec![actor.clone().into(), ts.clone().into(), id.into()],
            ),
            Operation::write(
                Self::event_insert_sql(),
                vec![
                    event_id.into(),
                    ts.into(),
                    id.into(),
                    "task.claim".into(),
                    payload.into(),
                    actor.into(),
                ],
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
        let event = results
            .get(1)
            .ok_or_else(|| anyhow!("Dactyl claim returned no event result"))?;
        Self::require_write(event, "claim event")?;
        match observation {
            OperationResult::Rows(rows) => self.task_from_rows(rows),
            OperationResult::Write(_) => Err(anyhow!(
                "Dactyl claim returned a write result for its task observation"
            )),
        }
    }

    async fn release_task(&self, id: &str, actor: String) -> Result<Task> {
        let ts = Self::operation_timestamp();
        let event_id = crate::core::ulid::new_ulid().to_string();
        let payload = serde_json::json!({ "released_by": actor }).to_string();
        let bridge = self.bridge()?;
        let result = bridge.atomic(&[
            Operation::write(
                "UPDATE tasks SET status = 'open', assigned_to = '', assigned_at = NULL, updated_at = $1, version = COALESCE(version, 1) + 1 WHERE id = $2 AND status = 'in_progress' AND assigned_to = $3",
                vec![ts.clone().into(), id.into(), actor.clone().into()],
            ),
            Operation::write(
                Self::event_insert_sql(),
                vec![
                    event_id.into(),
                    ts.into(),
                    id.into(),
                    "task.release".into(),
                    payload.into(),
                    actor.into(),
                ],
            ),
            Operation::read(Self::get_sql(), vec![id.into()]),
        ])?;
        let mut results = result.results;
        let observation = results
            .pop()
            .ok_or_else(|| anyhow!("Dactyl release returned no observation result"))?;
        let write = results
            .first()
            .ok_or_else(|| anyhow!("Dactyl release returned no write result"))?;
        Self::require_write(write, "release")?;
        let event = results
            .get(1)
            .ok_or_else(|| anyhow!("Dactyl release returned no event result"))?;
        Self::require_write(event, "release event")?;
        match observation {
            OperationResult::Rows(rows) => self.task_from_rows(rows),
            OperationResult::Write(_) => Err(anyhow!(
                "Dactyl release returned a write result for its task observation"
            )),
        }
    }

    async fn complete_task(&self, id: &str, actor: String, resolution: String) -> Result<Task> {
        let ts = Self::operation_timestamp();
        let event_id = crate::core::ulid::new_ulid().to_string();
        let payload = serde_json::json!({ "resolution": resolution }).to_string();
        let bridge = self.bridge()?;
        let result = bridge.atomic(&[
            Operation::write(
                "UPDATE tasks SET status = 'completed', completed_at = $1, updated_at = $1, version = COALESCE(version, 1) + 1 WHERE id = $2 AND status = 'in_progress' AND assigned_to = $3",
                vec![ts.clone().into(), id.into(), actor.clone().into()],
            ),
            Operation::write(
                Self::event_insert_sql(),
                vec![
                    event_id.into(),
                    ts.into(),
                    id.into(),
                    "task.done".into(),
                    payload.into(),
                    actor.into(),
                ],
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
        let event = results
            .get(1)
            .ok_or_else(|| anyhow!("Dactyl complete returned no event result"))?;
        Self::require_write(event, "complete event")?;
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
    value.and_then(|value| {
        DateTime::parse_from_rfc3339(value)
            .map(|value| value.with_timezone(&Utc))
            .ok()
            .or_else(|| {
                NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f")
                    .ok()
                    .map(|value| DateTime::from_naive_utc_and_offset(value, Utc))
            })
    })
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
#[path = "../../../tests/unit/core/dactyl_todo_tests.rs"]
mod tests;
