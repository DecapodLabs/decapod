use crate::core::storage as ds; // decapod storage
use anyhow::Result;
use async_trait::async_trait;
use std::path::Path;

pub struct PropodusStorage {
    inner: Box<dyn propodus::traits::StorageProvider>,
}

unsafe impl Send for PropodusStorage {}
unsafe impl Sync for PropodusStorage {}

impl PropodusStorage {
    pub async fn load(root: &Path) -> Result<Self> {
        let inner = propodus::load_storage(root).await?;
        Ok(Self { inner })
    }
}

#[async_trait]
impl ds::TodoStore for PropodusStorage {
    async fn list_tasks(&self) -> Result<Vec<ds::Task>> {
        let tasks = self.inner.todo_store().list_tasks().await?;
        Ok(tasks.into_iter().map(map_task_to_ds).collect())
    }

    async fn add_task(&self, task: ds::Task, actor: String, intent: String) -> Result<()> {
        self.inner
            .todo_store()
            .add_task(map_task_from_ds(task), actor, intent)
            .await
    }

    async fn claim_task(&self, id: &str, actor: String) -> Result<()> {
        self.inner.todo_store().claim_task(id, actor).await
    }

    async fn complete_task(&self, id: &str, actor: String, resolution: String) -> Result<()> {
        self.inner
            .todo_store()
            .complete_task(id, actor, resolution)
            .await
    }
}

#[async_trait]
impl ds::KnowledgeStore for PropodusStorage {
    async fn get_knowledge(&self, id: &str) -> Result<Option<ds::KnowledgeEntry>> {
        let entry = self.inner.knowledge_store().get_knowledge(id).await?;
        Ok(entry.map(map_knowledge_to_ds))
    }

    async fn list_knowledge(&self) -> Result<Vec<ds::KnowledgeEntry>> {
        let entries = self.inner.knowledge_store().list_knowledge().await?;
        Ok(entries.into_iter().map(map_knowledge_to_ds).collect())
    }

    async fn upsert_knowledge(
        &self,
        item: ds::KnowledgeEntry,
        actor: String,
        intent: String,
    ) -> Result<()> {
        self.inner
            .knowledge_store()
            .upsert_knowledge(map_knowledge_from_ds(item), actor, intent)
            .await
    }
}

#[async_trait]
impl ds::DecisionStore for PropodusStorage {
    async fn list_decisions(&self) -> Result<Vec<ds::Decision>> {
        let decisions = self.inner.decision_store().list_decisions().await?;
        Ok(decisions.into_iter().map(map_decision_to_ds).collect())
    }

    async fn add_decision(
        &self,
        decision: ds::Decision,
        actor: String,
        intent: String,
    ) -> Result<()> {
        self.inner
            .decision_store()
            .add_decision(map_decision_from_ds(decision), actor, intent)
            .await
    }
}

impl ds::StorageProvider for PropodusStorage {
    fn todo_store(&self) -> &dyn ds::TodoStore {
        self
    }
    fn knowledge_store(&self) -> &dyn ds::KnowledgeStore {
        self
    }
    fn decision_store(&self) -> &dyn ds::DecisionStore {
        self
    }
}

fn map_task_to_ds(t: propodus::types::Task) -> ds::Task {
    ds::Task {
        id: t.id,
        repo_id: t.repo_id,
        hash: t.hash,
        title: t.title,
        description: Some(t.description),
        status: t.status,
        assignee: t.assignee,
        scope: t.scope,
        dir_path: t.dir_path,
        priority: t.priority,
        category: t.category,
        tags: t.tags,
        created_at: t.created_at,
        updated_at: t.updated_at,
        version: t.version,
    }
}

fn map_task_from_ds(t: ds::Task) -> propodus::types::Task {
    propodus::types::Task {
        id: t.id,
        repo_id: t.repo_id,
        hash: t.hash,
        title: t.title,
        description: t.description.unwrap_or_default(),
        status: t.status,
        assignee: t.assignee,
        scope: t.scope,
        dir_path: t.dir_path,
        priority: t.priority,
        category: t.category,
        tags: t.tags,
        created_at: t.created_at,
        updated_at: t.updated_at,
        version: t.version,
    }
}

fn map_knowledge_to_ds(t: propodus::types::KnowledgeEntry) -> ds::KnowledgeEntry {
    ds::KnowledgeEntry {
        id: t.id,
        repo_id: t.repo_id,
        title: t.title,
        content: t.content,
        provenance: t.provenance,
        scope: t.scope,
        dir_path: t.dir_path,
        tags: t.tags,
        created_at: t.created_at,
        updated_at: t.updated_at,
        version: t.version,
    }
}

fn map_knowledge_from_ds(t: ds::KnowledgeEntry) -> propodus::types::KnowledgeEntry {
    propodus::types::KnowledgeEntry {
        id: t.id,
        repo_id: t.repo_id,
        title: t.title,
        content: t.content,
        provenance: t.provenance,
        scope: t.scope,
        dir_path: t.dir_path,
        tags: t.tags,
        created_at: t.created_at,
        updated_at: t.updated_at,
        version: t.version,
    }
}

fn map_decision_to_ds(t: propodus::types::Decision) -> ds::Decision {
    ds::Decision {
        id: t.id,
        repo_id: t.repo_id,
        session_id: t.session_id,
        question_text: t.question_text,
        chosen_value: t.chosen_value,
        rationale: t.rationale,
        actor: t.actor,
        created_at: t.created_at,
        version: t.version,
    }
}

fn map_decision_from_ds(t: ds::Decision) -> propodus::types::Decision {
    propodus::types::Decision {
        id: t.id,
        repo_id: t.repo_id,
        session_id: t.session_id,
        question_text: t.question_text,
        chosen_value: t.chosen_value,
        rationale: t.rationale,
        actor: t.actor,
        created_at: t.created_at,
        version: t.version,
    }
}
