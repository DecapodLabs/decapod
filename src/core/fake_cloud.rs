use async_trait::async_trait;
use crate::core::storage::{Decision, DecisionStore, KnowledgeEntry, KnowledgeStore, StorageProvider, Task, TodoStore};
use anyhow::Result;
use std::sync::Mutex;

pub struct FakeCloudStorage {
    tasks: Mutex<Vec<Task>>,
}

impl FakeCloudStorage {
    pub fn new() -> Self {
        Self {
            tasks: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl TodoStore for FakeCloudStorage {
    async fn list_tasks(&self) -> Result<Vec<Task>> {
        let tasks = self.tasks.lock().unwrap();
        Ok(tasks.clone())
    }

    async fn add_task(&self, task: Task, _actor: String, _intent: String) -> Result<()> {
        let mut tasks = self.tasks.lock().unwrap();
        tasks.push(task);
        Ok(())
    }

    async fn claim_task(&self, id: &str, actor: String) -> Result<()> {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(task) = tasks.iter_mut().find(|t| t.id == id) {
            task.assignee = Some(actor);
            task.status = "in_progress".to_string();
        }
        Ok(())
    }

    async fn complete_task(&self, id: &str, _actor: String, _resolution: String) -> Result<()> {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(task) = tasks.iter_mut().find(|t| t.id == id) {
            task.status = "done".to_string();
        }
        Ok(())
    }
}

#[async_trait]
impl KnowledgeStore for FakeCloudStorage {
    async fn get_knowledge(&self, _id: &str) -> Result<Option<KnowledgeEntry>> { Ok(None) }
    async fn list_knowledge(&self) -> Result<Vec<KnowledgeEntry>> { Ok(Vec::new()) }
    async fn upsert_knowledge(&self, _item: KnowledgeEntry, _actor: String, _intent: String) -> Result<()> { Ok(()) }
}

#[async_trait]
impl DecisionStore for FakeCloudStorage {
    async fn list_decisions(&self) -> Result<Vec<Decision>> { Ok(Vec::new()) }
    async fn add_decision(&self, _decision: Decision, _actor: String, _intent: String) -> Result<()> { Ok(()) }
}

impl StorageProvider for FakeCloudStorage {
    fn todo_store(&self) -> &dyn TodoStore { self }
    fn knowledge_store(&self) -> &dyn KnowledgeStore { self }
    fn decision_store(&self) -> &dyn DecisionStore { self }
}

pub fn load_storage() -> Box<dyn StorageProvider> {
    Box::new(FakeCloudStorage::new())
}
