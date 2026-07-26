use anyhow::Result;
use async_trait::async_trait;
use decapod::core::storage::{Task, TodoStore};
use decapod::core::todo::{ClaimMode, TodoCommand, run_cloud_todo_command_with_store};
use std::sync::{Arc, Mutex};
use tempfile::tempdir;

#[derive(Clone, Default)]
struct RecordingStore {
    calls: Arc<Mutex<Vec<String>>>,
    tasks: Arc<Mutex<Vec<Task>>>,
}

impl RecordingStore {
    fn calls(&self) -> Vec<String> {
        self.calls.lock().expect("calls lock").clone()
    }
}

#[async_trait]
impl TodoStore for RecordingStore {
    async fn list_tasks(&self) -> Result<Vec<Task>> {
        self.calls.lock().expect("calls lock").push("list".into());
        Ok(self.tasks.lock().expect("tasks lock").clone())
    }

    async fn add_task(&self, mut task: Task, _actor: String, _intent: String) -> Result<Task> {
        self.calls.lock().expect("calls lock").push("add".into());
        task.id = "cloud-1".to_string();
        self.tasks.lock().expect("tasks lock").push(task.clone());
        Ok(task)
    }

    async fn claim_task(&self, id: &str, _actor: String) -> Result<Task> {
        self.calls.lock().expect("calls lock").push("claim".into());
        let mut tasks = self.tasks.lock().expect("tasks lock");
        let task = tasks.iter_mut().find(|task| task.id == id).expect("task");
        task.assignee = Some("agent-two".to_string());
        Ok(task.clone())
    }

    async fn complete_task(&self, id: &str, _actor: String, _resolution: String) -> Result<Task> {
        self.calls
            .lock()
            .expect("calls lock")
            .push("complete".into());
        let mut tasks = self.tasks.lock().expect("tasks lock");
        let task = tasks.iter_mut().find(|task| task.id == id).expect("task");
        task.status = "completed".to_string();
        Ok(task.clone())
    }
}

#[test]
fn active_cloud_commands_use_the_storage_adapter_without_local_fallback() {
    let root = tempdir().expect("tempdir");
    let store = RecordingStore::default();

    run_cloud_todo_command_with_store(
        root.path(),
        &TodoCommand::Add {
            title: "shared cloud task".into(),
            description: String::new(),
            priority: "medium".into(),
            tags: String::new(),
            owner: String::new(),
            due: None,
            r#ref: String::new(),
            scope: String::new(),
            dir: None,
            depends_on: String::new(),
            blocks: String::new(),
            parent: None,
            one_shot: 0,
        },
        &store,
    )
    .expect("cloud add");

    run_cloud_todo_command_with_store(
        root.path(),
        &TodoCommand::List {
            status: "open".into(),
            scope: None,
            tags: None,
            title_search: None,
            dir: None,
        },
        &store,
    )
    .expect("cloud list");
    run_cloud_todo_command_with_store(
        root.path(),
        &TodoCommand::Claim {
            id: "cloud-1".into(),
            agent: Some("agent-two".into()),
            mode: ClaimMode::Shared,
        },
        &store,
    )
    .expect("cloud claim");
    run_cloud_todo_command_with_store(
        root.path(),
        &TodoCommand::Done {
            id: Some("cloud-1".into()),
            id_positional: None,
            validated: false,
            artifact: vec![],
        },
        &store,
    )
    .expect("cloud complete");

    assert_eq!(store.calls(), ["add", "list", "claim", "complete"]);
}
