use anyhow::Result;
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

#[async_trait]
pub trait TodoStore: Send + Sync {
    async fn list_tasks(&self) -> Result<Vec<Task>>;
    async fn add_task(&self, task: Task, actor: String, intent: String) -> Result<()>;
    async fn claim_task(&self, id: &str, actor: String) -> Result<()>;
    async fn complete_task(&self, id: &str, actor: String, resolution: String) -> Result<()>;
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
