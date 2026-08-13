use anyhow::{Result, anyhow};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Task {
    pub id: String,
    pub repo_id: String,
    pub hash: String,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub assignee: Option<String>,
    pub scope: String,
    pub dir_path: String,
    pub priority: String,
    pub category: String,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KnowledgeEntry {
    pub id: String,
    pub repo_id: String,
    pub title: String,
    pub content: String,
    pub provenance: String,
    pub scope: String,
    pub dir_path: String,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Decision {
    pub id: String,
    pub repo_id: String,
    pub session_id: String,
    pub question_text: String,
    pub chosen_value: String,
    pub rationale: String,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub version: i32,
}

/// Backend-neutral repo-scoped todo boundary.
///
/// Local SQLite is the default implementation. Remote adapters implement this
/// seam without leaking HTTP or authentication concerns into the local storage
/// kernel. In cloud mode Decapod authenticates at the Propodus boundary and
/// sends these operations through Dactyl; it does not call a Propodus todo
/// resource route directly.
#[async_trait]
pub trait TodoStore: Send + Sync {
    async fn list_tasks(&self) -> Result<Vec<Task>>;
    /// Read one task without requiring callers to scan the complete projection.
    /// Implementations may override this with a keyed backend query.
    async fn get_task(&self, id: &str) -> Result<Option<Task>> {
        Ok(self
            .list_tasks()
            .await?
            .into_iter()
            .find(|task| task.id == id))
    }
    async fn add_task(&self, task: Task, actor: String, intent: String) -> Result<Task>;
    async fn claim_task(&self, id: &str, actor: String) -> Result<Task>;
    /// Release an active claim back to the open state.
    ///
    /// The default keeps older adapters source-compatible while making an
    /// unsupported cloud capability explicit instead of falling back locally.
    async fn release_task(&self, _id: &str, _actor: String) -> Result<Task> {
        Err(anyhow!("todo release is not supported by this storage adapter"))
    }
    async fn complete_task(&self, id: &str, actor: String, resolution: String) -> Result<Task>;
}

#[async_trait]
pub trait KnowledgeStore: Send + Sync {
    async fn get_knowledge(&self, id: &str) -> Result<Option<KnowledgeEntry>>;
    async fn list_knowledge(&self) -> Result<Vec<KnowledgeEntry>>;
    async fn upsert_knowledge(
        &self,
        item: KnowledgeEntry,
        actor: String,
        intent: String,
    ) -> Result<()>;
}

#[async_trait]
pub trait DecisionStore: Send + Sync {
    async fn list_decisions(&self) -> Result<Vec<Decision>>;
    async fn add_decision(&self, decision: Decision, actor: String, intent: String) -> Result<()>;
}

pub trait StorageProvider {
    fn todo_store(&self) -> &dyn TodoStore;
    fn knowledge_store(&self) -> &dyn KnowledgeStore;
    fn decision_store(&self) -> &dyn DecisionStore;
}
