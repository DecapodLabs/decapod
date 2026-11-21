use anyhow::Result;
use async_trait::async_trait;
use clap::Parser;
use decapod::CloudRuntimeConfig;
use decapod::core::backend::{BackendRoute, StorageContext};
use decapod::core::dactyl_todo::DactylTodoStore;
use decapod::core::error::DecapodError;
use decapod::core::repo_identity::{RepositoryIdentity, resolve_repository_identity};
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
    identities: Arc<Mutex<Vec<String>>>,
}

impl CloudTodoStoreFactory for MockFactory {
    fn build(
        &self,
        config: &CloudRuntimeConfig,
        identity: &RepositoryIdentity,
    ) -> Result<Box<dyn TodoStore>, DecapodError> {
        assert_eq!(config.provider, "dactyl");
        *self.builds.lock().expect("builds lock") += 1;
        self.identities
            .lock()
            .expect("identities lock")
            .push(identity.canonical_name.clone());
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
        _config: &CloudRuntimeConfig,
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
        .args(["init", "--backend", "cloud", "--force", "--proof"])
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
    let machine = tempfile::tempdir().unwrap();
    // This integration binary has one test; set before any worker threads.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", machine.path());
    }
    let project = cloud_project();
    let data_root = project.path().join(".decapod/data");
    let todo_db = data_root.join("decapod.db");
    assert!(!todo_db.exists(), "test starts without local todo SQLite");
    let store = MockPropodusStore::default();
    let factory = MockFactory {
        store: store.clone(),
        builds: Arc::new(Mutex::new(0)),
        identities: Arc::new(Mutex::new(Vec::new())),
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
        parse_todo(&[
            "claim",
            "--id",
            "cloud-cli-1",
            "--agent",
            "cloud-cli-agent",
            "--format",
            "json",
        ]),
        &factory,
    )
    .expect("cloud claim");
    let renewal = run_todo_cli_with_cloud_factory(
        &decapod_store,
        parse_todo(&[
            "renew",
            "--id",
            "cloud-cli-1",
            "--agent",
            "cloud-cli-agent",
            "--lease-seconds",
            "300",
        ]),
        &factory,
    )
    .expect_err("a hosted claim cannot renew without a hosted write");
    assert!(matches!(
        renewal,
        DecapodError::NotImplemented(message)
            if message.starts_with("CLOUD_LEASE_RENEW_UNSUPPORTED:")
    ));
    let route = BackendRoute::Cloud {
        repository: resolve_repository_identity(project.path()).expect("fixture repository"),
        uri: "invalid fixture URI".into(),
    };
    let context =
        StorageContext::from_route(route, Some("fixture-bearer")).expect("fixture context");
    let adapter = DactylTodoStore::new(context, "DecapodLabs/decapod");
    let renewal = adapter
        .renew_task_lease_blocking("cloud-cli-1", "cloud-cli-agent".into(), 300)
        .expect_err("remote adapter cannot renew via its local lease cache");
    assert!(
        renewal
            .to_string()
            .starts_with("CLOUD_LEASE_RENEW_UNSUPPORTED:"),
        "unexpected adapter error: {renewal}"
    );
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
        factory
            .identities
            .lock()
            .expect("identities lock")
            .iter()
            .all(|identity| identity == "DecapodLabs/decapod")
    );
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
            .contains("Dactyl cloud todo operation failed"),
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
    run_todo_cli_with_cloud_factory(
        &decapod_store,
        parse_todo(&["list", "--format", "json"]),
        &factory,
    )
    .expect("fork identity must reach the provider boundary");
    assert!(
        factory
            .identities
            .lock()
            .expect("identities lock")
            .last()
            .is_some_and(|identity| identity == "someone/decapod")
    );
    assert_eq!(*factory.builds.lock().expect("builds lock"), 8);
}

#[test]
fn mixed_worktree_routing_honors_invoking_workspace_in_both_directions() {
    use decapod::core::store::{Store, StoreKind};
    use decapod::core::todo::run_todo_cli_with_cloud_factory_in_workspace;
    use std::fs;

    // Direction 1: local root checkout with populated local SQLite + cloud worktree.
    let local_root = TempDir::new().expect("local_root tempdir");
    git(local_root.path(), &["init"]);
    git(local_root.path(), &["config", "user.name", "Decapod Test"]);
    git(
        local_root.path(),
        &["config", "user.email", "test@example.invalid"],
    );
    git(
        local_root.path(),
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/DecapodLabs/decapod.git",
        ],
    );
    fs::create_dir_all(local_root.path().join(".decapod")).expect("create .decapod");
    fs::write(
        local_root.path().join(".decapod/config.toml"),
        "schema_version = \"1.0.0\"\n[init]\ndiagram_style = \"mermaid\"\nentrypoints = []\n[repo]\nbackend = \"local\"\n",
    )
    .expect("write local config");
    git(local_root.path(), &["add", "."]);
    git(local_root.path(), &["commit", "-m", "init"]);

    let local_store = Store {
        kind: StoreKind::Repo,
        root: local_root.path().join(".decapod/data"),
    };
    fs::create_dir_all(&local_store.root).expect("create local store");
    run_todo_cli_with_cloud_factory(
        &local_store,
        parse_todo(&["add", "ROOT_LOCAL_SENTINEL"]),
        &FailingFactory,
    )
    .expect("seed local root task");

    let cloud_wt = TempDir::new().expect("cloud worktree tempdir");
    let cloud_wt_path = cloud_wt.path().join("wt");
    git(
        local_root.path(),
        &[
            "worktree",
            "add",
            "-b",
            "cloud-wt",
            cloud_wt_path.to_str().unwrap(),
        ],
    );
    fs::create_dir_all(cloud_wt_path.join(".decapod")).expect("create cloud wt .decapod");
    fs::write(
        cloud_wt_path.join(".decapod/config.toml"),
        "schema_version = \"1.0.0\"\n[init]\ndiagram_style = \"mermaid\"\nentrypoints = []\n[repo]\nbackend = \"cloud\"\n",
    )
    .expect("write cloud wt config");

    let store = MockPropodusStore::default();
    let factory = MockFactory {
        store: store.clone(),
        builds: Arc::new(Mutex::new(0)),
        identities: Arc::new(Mutex::new(Vec::new())),
    };
    run_todo_cli_with_cloud_factory_in_workspace(
        &local_store,
        &cloud_wt_path,
        parse_todo(&["list", "--format", "json"]),
        &factory,
    )
    .expect("cloud worktree must route via cloud factory even with local root");
    assert_eq!(store.calls(), ["list"]);
    assert!(
        !cloud_wt_path.join(".decapod/data/todo.db").exists(),
        "cloud worktree must not create local todo.db"
    );

    // Direction 2: cloud root checkout + local worktree.
    let cloud_root = TempDir::new().expect("cloud_root tempdir");
    git(cloud_root.path(), &["init"]);
    git(cloud_root.path(), &["config", "user.name", "Decapod Test"]);
    git(
        cloud_root.path(),
        &["config", "user.email", "test@example.invalid"],
    );
    git(
        cloud_root.path(),
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/DecapodLabs/decapod.git",
        ],
    );
    fs::create_dir_all(cloud_root.path().join(".decapod")).expect("create .decapod");
    fs::write(
        cloud_root.path().join(".decapod/config.toml"),
        "schema_version = \"1.0.0\"\n[init]\ndiagram_style = \"mermaid\"\nentrypoints = []\n[repo]\nbackend = \"cloud\"\n",
    )
    .expect("write cloud root config");
    git(cloud_root.path(), &["add", "."]);
    git(cloud_root.path(), &["commit", "-m", "init"]);

    let local_wt = TempDir::new().expect("local worktree tempdir");
    let local_wt_path = local_wt.path().join("wt");
    git(
        cloud_root.path(),
        &[
            "worktree",
            "add",
            "-b",
            "local-wt",
            local_wt_path.to_str().unwrap(),
        ],
    );
    fs::create_dir_all(local_wt_path.join(".decapod")).expect("create local wt .decapod");
    fs::write(
        local_wt_path.join(".decapod/config.toml"),
        "schema_version = \"1.0.0\"\n[init]\ndiagram_style = \"mermaid\"\nentrypoints = []\n[repo]\nbackend = \"local\"\n",
    )
    .expect("write local wt config");

    let cloud_root_store = Store {
        kind: StoreKind::Repo,
        root: cloud_root.path().join(".decapod/data"),
    };
    fs::create_dir_all(&cloud_root_store.root).expect("create local data dir");
    let failing_factory = FailingFactory;
    run_todo_cli_with_cloud_factory_in_workspace(
        &cloud_root_store,
        &local_wt_path,
        parse_todo(&["add", "LOCAL_WORKTREE_TASK", "--format", "json"]),
        &failing_factory,
    )
    .expect("local worktree must use local SQLite even with cloud root");
    assert!(
        cloud_root_store.root.join("decapod.db").exists(),
        "local worktree under cloud root must persist in canonical local decapod.db"
    );
    run_todo_cli_with_cloud_factory_in_workspace(
        &cloud_root_store,
        &local_wt_path,
        parse_todo(&["get", "--id", "missing", "--format", "json"]),
        &failing_factory,
    )
    .expect("local worktree get under cloud root must stay local");
}
