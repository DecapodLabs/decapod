use anyhow::Result;
use async_trait::async_trait;
use clap::Parser;
use decapod::CloudConfigSection;
use decapod::core::error::DecapodError;
use decapod::core::repo_identity::RepositoryIdentity;
use decapod::core::storage::{Task, TodoStore};
use decapod::core::store::{Store, StoreKind};
use decapod::core::todo::{CloudTodoStoreFactory, TodoCli, run_todo_cli_with_cloud_factory};
use std::process::Command;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

#[derive(Clone, Default)]
struct MockPropodusStore {
    calls: Arc<Mutex<Vec<String>>>,
    tasks: Arc<Mutex<Vec<Task>>>,
}

impl MockPropodusStore {
    fn calls(&self) -> Vec<String> {
        self.calls.lock().expect("calls lock").clone()
    }
}

#[async_trait]
impl TodoStore for MockPropodusStore {
    async fn list_tasks(&self) -> Result<Vec<Task>> {
        self.calls.lock().expect("calls lock").push("list".into());
        Ok(self.tasks.lock().expect("tasks lock").clone())
    }

    async fn add_task(&self, mut task: Task, _actor: String, _intent: String) -> Result<Task> {
        self.calls.lock().expect("calls lock").push("add".into());
        task.id = "cloud-cli-1".into();
        self.tasks.lock().expect("tasks lock").push(task.clone());
        Ok(task)
    }

    async fn claim_task(&self, id: &str, _actor: String) -> Result<Task> {
        self.calls.lock().expect("calls lock").push("claim".into());
        let mut tasks = self.tasks.lock().expect("tasks lock");
        let task = tasks.iter_mut().find(|task| task.id == id).expect("task");
        task.assignee = Some("cli-agent".into());
        Ok(task.clone())
    }

    async fn complete_task(&self, id: &str, _actor: String, _resolution: String) -> Result<Task> {
        self.calls
            .lock()
            .expect("calls lock")
            .push("complete".into());
        let mut tasks = self.tasks.lock().expect("tasks lock");
        let task = tasks.iter_mut().find(|task| task.id == id).expect("task");
        task.status = "completed".into();
        Ok(task.clone())
    }
}

#[derive(Clone)]
struct MockFactory {
    store: MockPropodusStore,
    builds: Arc<Mutex<usize>>,
}

impl CloudTodoStoreFactory for MockFactory {
    fn build(
        &self,
        config: &CloudConfigSection,
        identity: &RepositoryIdentity,
    ) -> Result<Box<dyn TodoStore>, DecapodError> {
        assert_eq!(config.provider, "vercel");
        assert_eq!(identity.canonical_name, "DecapodLabs/decapod");
        *self.builds.lock().expect("builds lock") += 1;
        Ok(Box::new(self.store.clone()))
    }
}

struct FailingStore;

#[async_trait]
impl TodoStore for FailingStore {
    async fn list_tasks(&self) -> Result<Vec<Task>> {
        Err(anyhow::anyhow!("transport failure from fake Propodus"))
    }

    async fn add_task(&self, _task: Task, _actor: String, _intent: String) -> Result<Task> {
        Err(anyhow::anyhow!("transport failure from fake Propodus"))
    }

    async fn claim_task(&self, _id: &str, _actor: String) -> Result<Task> {
        Err(anyhow::anyhow!("transport failure from fake Propodus"))
    }

    async fn complete_task(&self, _id: &str, _actor: String, _resolution: String) -> Result<Task> {
        Err(anyhow::anyhow!("transport failure from fake Propodus"))
    }
}

struct FailingFactory;

impl CloudTodoStoreFactory for FailingFactory {
    fn build(
        &self,
        _config: &CloudConfigSection,
        _identity: &RepositoryIdentity,
    ) -> Result<Box<dyn TodoStore>, DecapodError> {
        Ok(Box::new(FailingStore))
    }
}

fn parse_todo(args: &[&str]) -> TodoCli {
    let mut argv = vec!["todo".to_string()];
    argv.extend(args.iter().map(|arg| (*arg).to_string()));
    TodoCli::try_parse_from(argv).expect("parse todo CLI")
}

fn git(dir: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn cloud_project() -> TempDir {
    let project = tempfile::tempdir().expect("project tempdir");
    let init = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["init", "--mode", "cloud", "--force", "--proof"])
        .current_dir(project.path())
        .output()
        .expect("init cloud project");
    assert!(
        init.status.success(),
        "cloud init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    git(
        project.path(),
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:DecapodLabs/decapod.git",
        ],
    );
    project
}

#[test]
fn production_cli_dispatch_uses_remote_store_and_never_local_sqlite() {
    let project = cloud_project();
    let data_root = project.path().join(".decapod/data");
    let todo_db = data_root.join("todo.db");
    assert!(!todo_db.exists(), "test starts without local todo SQLite");
    // This integration test invokes the same env-gated production resolver as
    // the binary. Tests in this file run serially to keep the process-global
    // dogfood marker isolated.
    unsafe { std::env::set_var("DECAPOD_PROPODUS_DOGFOOD", "1") };

    let store = MockPropodusStore::default();
    let factory = MockFactory {
        store: store.clone(),
        builds: Arc::new(Mutex::new(0)),
    };
    let decapod_store = Store {
        kind: StoreKind::Repo,
        root: data_root,
    };

    run_todo_cli_with_cloud_factory(
        &decapod_store,
        parse_todo(&["list", "--format", "json"]),
        &factory,
    )
    .expect("cloud list");
    run_todo_cli_with_cloud_factory(
        &decapod_store,
        parse_todo(&["add", "CLI boundary task", "--format", "json"]),
        &factory,
    )
    .expect("cloud add");
    run_todo_cli_with_cloud_factory(
        &decapod_store,
        parse_todo(&["get", "--id", "cloud-cli-1", "--format", "json"]),
        &factory,
    )
    .expect("cloud get");
    run_todo_cli_with_cloud_factory(
        &decapod_store,
        parse_todo(&["show", "cloud-cli-1", "--format", "json"]),
        &factory,
    )
    .expect("cloud show");
    run_todo_cli_with_cloud_factory(
        &decapod_store,
        parse_todo(&["claim", "--id", "cloud-cli-1", "--format", "json"]),
        &factory,
    )
    .expect("cloud claim");
    run_todo_cli_with_cloud_factory(
        &decapod_store,
        parse_todo(&["done", "--id", "cloud-cli-1", "--format", "json"]),
        &factory,
    )
    .expect("cloud done");
    let missing = run_todo_cli_with_cloud_factory(
        &decapod_store,
        parse_todo(&["get", "--id", "missing", "--format", "json"]),
        &factory,
    );
    assert!(
        missing.is_ok(),
        "v1 get must report not_found in its output"
    );

    assert_eq!(
        store.calls(),
        ["list", "add", "list", "list", "claim", "complete", "list"]
    );
    assert_eq!(*factory.builds.lock().expect("builds lock"), 7);
    assert!(
        !todo_db.exists(),
        "cloud CLI must not initialize local todo SQLite"
    );

    let transport_error = run_todo_cli_with_cloud_factory(
        &decapod_store,
        parse_todo(&["list", "--format", "json"]),
        &FailingFactory,
    )
    .expect_err("transport failure must remain an error");
    assert!(
        transport_error
            .to_string()
            .contains("Propodus cloud todo operation failed"),
        "unexpected transport error: {transport_error}"
    );
    assert!(
        !todo_db.exists(),
        "transport failure must not fall back to SQLite"
    );

    git(
        project.path(),
        &[
            "remote",
            "set-url",
            "origin",
            "git@github.com:someone/decapod.git",
        ],
    );
    let fork_error = run_todo_cli_with_cloud_factory(
        &decapod_store,
        parse_todo(&["list", "--format", "json"]),
        &factory,
    )
    .expect_err("fork must fail before factory/network");
    assert!(
        fork_error
            .to_string()
            .contains("restricted to DecapodLabs/decapod")
    );
    assert_eq!(*factory.builds.lock().expect("builds lock"), 7);
    unsafe { std::env::remove_var("DECAPOD_PROPODUS_DOGFOOD") };
}
