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
        "INSERT INTO events (event_id, ts, seq, stream, subject_kind, subject_id, event_type, payload, actor)
         SELECT CASE WHEN EXISTS (SELECT 1 FROM tasks WHERE id = $3 AND updated_at = $2) THEN $1 ELSE NULL END,
                $2,
                COALESCE((SELECT MAX(seq) FROM events WHERE stream = 'todo'), 0) + 1,
                'todo',
                'task',
                $3,
                $4,
                $5,
                $6"
    }

    fn operation_timestamp() -> String {
        Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
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
                "UPDATE tasks SET status = 'open', assigned_to = '', assigned_at = NULL, updated_at = $2, version = COALESCE(version, 1) + 1 WHERE id = $3 AND status = 'in_progress' AND assigned_to = $1",
                vec![actor.clone().into(), ts.clone().into(), id.into()],
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
                "UPDATE tasks SET status = 'completed', completed_at = $2, updated_at = $2, version = COALESCE(version, 1) + 1 WHERE id = $1 AND status = 'in_progress' AND assigned_to = $3",
                vec![id.into(), ts.clone().into(), actor.clone().into()],
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
mod tests {
    use super::*;
    use crate::core::dactyl::DactylBridge;

    const EVENTS_TABLE_SQL: &str = "CREATE TABLE events (event_id TEXT PRIMARY KEY, ts TEXT NOT NULL, seq INTEGER NOT NULL, stream TEXT NOT NULL, subject_kind TEXT, subject_id TEXT, event_type TEXT NOT NULL DEFAULT '', payload TEXT NOT NULL, actor TEXT NOT NULL DEFAULT 'decapod')";

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
        bridge.write(EVENTS_TABLE_SQL, &[]).expect("event schema");
        bridge
            .write(
                "CREATE UNIQUE INDEX events_stream_seq ON events(stream, seq)",
                &[],
            )
            .expect("event sequence index");
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

    fn event_count(store: &DactylTodoStore) -> i64 {
        store
            .bridge()
            .expect("event bridge")
            .read("SELECT COUNT(*) AS count FROM events", &[])
            .expect("event count")
            .as_slice()
            .first()
            .expect("event count row")
            .get("count")
            .expect("event count value")
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
        assert_eq!(event_count(&store), 1);
        assert_eq!(
            store.get_task(&added.id).await.expect("get").unwrap().id,
            added.id
        );
        assert_eq!(store.list_tasks().await.expect("list").len(), 1);

        let claimed = store
            .claim_task(&added.id, "agent-a".to_string())
            .await
            .expect("claim");
        assert_eq!(claimed.status, "in_progress");
        assert_eq!(claimed.assignee.as_deref(), Some("agent-a"));
        assert_eq!(claimed.version, added.version + 1);
        assert_eq!(event_count(&store), 2);

        let released = store
            .release_task(&added.id, "agent-a".to_string())
            .await
            .expect("release");
        assert_eq!(released.status, "open");
        assert_eq!(released.assignee.as_deref(), Some(""));
        assert_eq!(event_count(&store), 3);

        store
            .claim_task(&added.id, "agent-a".to_string())
            .await
            .expect("re-claim");
        assert_eq!(event_count(&store), 4);

        let completed = store
            .complete_task(&added.id, "agent-a".to_string(), String::new())
            .await
            .expect("complete");
        assert_eq!(completed.status, "completed");
        assert_eq!(event_count(&store), 5);
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
        let events_before_conflict = event_count(&store);
        let error = store
            .claim_task(&added.id, "agent-b".to_string())
            .await
            .expect_err("second claim must conflict");
        assert!(error.to_string().contains("state conflict"));
        assert_eq!(event_count(&store), events_before_conflict);
        assert_eq!(
            store.list_tasks().await.expect("list")[0]
                .assignee
                .as_deref(),
            Some("agent-a")
        );
    }
}
