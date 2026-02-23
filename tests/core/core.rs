use decapod::core::assets;
use decapod::core::broker::{self, BrokerEvent, DbBroker};
use decapod::core::db;
use decapod::core::docs_cli::{self, DocsCli, DocsCommand};
use decapod::core::error::DecapodError;
use decapod::core::external_action::{self, ExternalCapability};
use decapod::core::migration;
use decapod::core::repomap;
use decapod::core::scaffold::{ScaffoldOptions, scaffold_project_entrypoints};
use decapod::core::schemas;
use decapod::core::store::{Store, StoreKind};
use decapod::core::validate;
use decapod::core::workspace;
use rusqlite::params;
use std::fs;
use std::process::Command;
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};
use tempfile::tempdir;

#[test]
fn assets_docs_and_templates_resolve() {
    let docs = assets::list_docs();
    assert!(docs.contains(&"core/DECAPOD.md".to_string()));

    for doc in docs {
        let content = assets::get_doc(&doc).expect("listed doc should be readable");
        assert!(!content.trim().is_empty());
    }

    let template_names = [
        "AGENTS.md",
        "CLAUDE.md",
        "GEMINI.md",
        "README.md",
        "OVERRIDE.md",
    ];

    for template in template_names {
        let content = assets::get_template(template).expect("template should exist");
        assert!(!content.trim().is_empty());
    }

    assert!(assets::get_doc("core/DOES_NOT_EXIST.md").is_none());
    assert!(assets::get_template("plugins/DOES_NOT_EXIST.md").is_none());
}

#[test]
fn db_and_broker_round_trip_and_audit() {
    let tmp = tempdir().expect("tempdir");
    let root = tmp.path();

    db::initialize_knowledge_db(root).expect("knowledge init");
    let db_path = db::knowledge_db_path(root);
    assert!(db_path.exists());

    let conn = db::db_connect(&db_path.to_string_lossy()).expect("db connect");
    let fk_on: i64 = conn
        .query_row("PRAGMA foreign_keys;", [], |row| row.get(0))
        .expect("pragma foreign_keys");
    assert_eq!(fk_on, 1);

    let broker = DbBroker::new(root);
    broker
        .with_conn(&db_path, "tester", Some("intent-1"), "knowledge.insert", |conn| {
            conn.execute(
                "INSERT INTO knowledge (id, title, content, provenance, claim_id, tags, created_at, updated_at, dir_path, scope) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    "k1",
                    "Title",
                    "Body",
                    "source://test",
                    Option::<String>::None,
                    "",
                    "2026-01-01T00:00:00Z",
                    Option::<String>::None,
                    ".",
                    "repo"
                ],
            )
            .map_err(DecapodError::RusqliteError)?;
            Ok(())
        })
        .expect("broker success path");

    let result: Result<(), DecapodError> =
        broker.with_conn(&db_path, "tester", None, "knowledge.fail", |_| {
            Err(DecapodError::ValidationError("intentional".to_string()))
        });
    assert!(result.is_err());

    let audit_path = root.join("broker.events.jsonl");
    assert!(audit_path.exists());
    let events: Vec<BrokerEvent> = fs::read_to_string(&audit_path)
        .expect("read audit")
        .lines()
        .map(|line| serde_json::from_str(line).expect("valid broker event json"))
        .collect();
    assert!(events.iter().any(|ev| ev.status == "success"));
    assert!(events.iter().any(|ev| ev.status == "error"));
    assert!(
        events
            .iter()
            .all(|ev| ev.schema_version == "1.0.0" && !ev.request_id.is_empty())
    );
    assert!(events.iter().all(|ev| ev.actor == ev.actor_id));

    let schema = broker::schema();
    assert_eq!(schema["name"], "broker");
    assert_eq!(schema["envelope"]["schema_version"], "1.0.0");
}

#[test]
fn broker_allows_parallel_ops_on_different_databases() {
    let tmp = tempdir().expect("tempdir");
    let root = tmp.path();
    let broker = Arc::new(DbBroker::new(root));

    let db_a = root.join("a.db");
    let db_b = root.join("b.db");

    let barrier = Arc::new(Barrier::new(3));

    let b1 = Arc::clone(&broker);
    let gate1 = Arc::clone(&barrier);
    let h1 = std::thread::spawn(move || {
        b1.with_conn(&db_a, "tester", None, "parallel.a", |conn| {
            conn.execute("CREATE TABLE IF NOT EXISTS t (id INTEGER)", [])
                .map_err(DecapodError::RusqliteError)?;
            gate1.wait();
            std::thread::sleep(Duration::from_millis(150));
            Ok(())
        })
    });

    let b2 = Arc::clone(&broker);
    let gate2 = Arc::clone(&barrier);
    let h2 = std::thread::spawn(move || {
        b2.with_conn(&db_b, "tester", None, "parallel.b", |conn| {
            conn.execute("CREATE TABLE IF NOT EXISTS t (id INTEGER)", [])
                .map_err(DecapodError::RusqliteError)?;
            gate2.wait();
            std::thread::sleep(Duration::from_millis(150));
            Ok(())
        })
    });

    barrier.wait();
    let started = Instant::now();
    h1.join().expect("thread a joined").expect("thread a ok");
    h2.join().expect("thread b joined").expect("thread b ok");
    let elapsed = started.elapsed();

    assert!(
        elapsed < Duration::from_millis(260),
        "expected per-db concurrency (<260ms), got {:?}",
        elapsed
    );
}

#[test]
fn external_action_broker_enforces_capability_allowlist() {
    let tmp = tempdir().expect("tempdir");
    let root = tmp.path();
    std::fs::create_dir_all(root.join("data")).expect("store root");

    let denied = external_action::execute(
        &root.join("data"),
        ExternalCapability::VcsRead,
        "test.scope",
        "echo",
        &["hello"],
        root,
    );
    assert!(denied.is_err(), "unexpected allow for disallowed binary");

    let allowed = external_action::execute(
        &root.join("data"),
        ExternalCapability::VcsRead,
        "test.scope",
        "git",
        &["status", "--porcelain"],
        root,
    );
    assert!(allowed.is_ok(), "git status should be allowed for vcs_read");
}

#[test]
fn broker_policy_enforces_trust_tier_on_high_risk_mutator_ops() {
    let tmp = tempdir().expect("tempdir");
    let root = tmp.path();
    let broker = DbBroker::new(root);
    let db_path = root.join("policy-test.db");

    let denied = broker.with_conn(
        &db_path,
        "agent-basic",
        None,
        "federation.rebuild",
        |conn| {
            conn.execute("CREATE TABLE IF NOT EXISTS t (id INTEGER)", [])
                .map_err(DecapodError::RusqliteError)?;
            Ok(())
        },
    );
    assert!(
        denied.is_err(),
        "expected trust-tier denial for high-risk op"
    );

    let allowed = broker.with_conn(&db_path, "decapod", None, "federation.rebuild", |conn| {
        conn.execute("CREATE TABLE IF NOT EXISTS t2 (id INTEGER)", [])
            .map_err(DecapodError::RusqliteError)?;
        Ok(())
    });
    assert!(allowed.is_ok(), "core actor should pass policy gate");
}

fn init_git_repo(path: &std::path::Path) {
    let init = Command::new("git")
        .current_dir(path)
        .args(["init", "-b", "master"])
        .output()
        .expect("git init");
    assert!(init.status.success(), "git init failed");

    let email = Command::new("git")
        .current_dir(path)
        .args(["config", "user.email", "alexhraber@gmail.com"])
        .output()
        .expect("git config email");
    assert!(email.status.success());

    let name = Command::new("git")
        .current_dir(path)
        .args(["config", "user.name", "Alex H. Raber"])
        .output()
        .expect("git config name");
    assert!(name.status.success());

    fs::write(path.join("README.md"), "# test\n").expect("write readme");
    let add = Command::new("git")
        .current_dir(path)
        .args(["add", "README.md"])
        .output()
        .expect("git add");
    assert!(add.status.success());

    let commit = Command::new("git")
        .current_dir(path)
        .args(["commit", "-m", "init"])
        .output()
        .expect("git commit");
    assert!(commit.status.success());
}

#[test]
fn worktree_config_prune_removes_stale_section() {
    let tmp = tempdir().expect("tempdir");
    let root = tmp.path();
    init_git_repo(root);

    let config_path = root.join(".git").join("config");
    let stale_path = root.join(".decapod").join("workspaces").join("stale-wt");
    let set = Command::new("git")
        .current_dir(root)
        .args([
            "config",
            "--file",
            config_path.to_str().expect("config path"),
            "worktree.stale.path",
            stale_path.to_str().expect("stale path"),
        ])
        .output()
        .expect("write stale worktree key");
    assert!(set.status.success(), "set stale section failed");
    assert!(!stale_path.exists(), "stale worktree path must not exist");

    let removed = workspace::prune_stale_worktree_config(root).expect("prune config");
    assert_eq!(removed, 1, "expected one stale section removed");

    let get = Command::new("git")
        .current_dir(root)
        .args([
            "config",
            "--file",
            config_path.to_str().expect("config path"),
            "--get",
            "worktree.stale.path",
        ])
        .output()
        .expect("check stale key removed");
    assert!(!get.status.success(), "stale section should be removed");
}

#[test]
fn worktree_config_prune_preserves_live_section() {
    let tmp = tempdir().expect("tempdir");
    let root = tmp.path();
    init_git_repo(root);

    let live_path = root.join("live-worktree");
    fs::create_dir_all(&live_path).expect("create live path");

    let add_wt = Command::new("git")
        .current_dir(root)
        .args([
            "worktree",
            "add",
            "-b",
            "agent/test/live",
            live_path.to_str().expect("live worktree path"),
        ])
        .output()
        .expect("create live worktree");
    assert!(
        add_wt.status.success(),
        "create live worktree failed: {}",
        String::from_utf8_lossy(&add_wt.stderr)
    );

    let config_path = root.join(".git").join("config");
    let set = Command::new("git")
        .current_dir(root)
        .args([
            "config",
            "--file",
            config_path.to_str().expect("config path"),
            "worktree.live.path",
            live_path.to_str().expect("live path"),
        ])
        .output()
        .expect("write live worktree key");
    assert!(set.status.success(), "set live section failed");

    let removed = workspace::prune_stale_worktree_config(root).expect("prune config");
    assert_eq!(removed, 0, "live section must not be removed");

    let get = Command::new("git")
        .current_dir(root)
        .args([
            "config",
            "--file",
            config_path.to_str().expect("config path"),
            "--get",
            "worktree.live.path",
        ])
        .output()
        .expect("check live key present");
    assert!(get.status.success(), "live section should remain");
}

#[test]
fn migration_reconstructs_legacy_events_from_fixture() {
    let tmp = tempdir().expect("tempdir");
    let decapod_root = tmp.path();
    let data_dir = decapod_root.join("data");
    fs::create_dir_all(&data_dir).expect("data dir");

    let fixture_sql = fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/migration/legacy_tasks.sql"),
    )
    .expect("read sql fixture");
    let conn = rusqlite::Connection::open(data_dir.join("todo.db")).expect("open db");
    conn.execute_batch(&fixture_sql)
        .expect("apply fixture schema");

    migration::check_and_migrate(decapod_root).expect("migration");

    let expected_lines = fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/migration/expected_todo_events.jsonl"),
    )
    .expect("read expected fixture");
    let actual_lines =
        fs::read_to_string(data_dir.join("todo.events.jsonl")).expect("read migrated events");

    let expected: Vec<serde_json::Value> = expected_lines
        .lines()
        .map(|line| serde_json::from_str(line).expect("expected fixture json"))
        .collect();
    let actual: Vec<serde_json::Value> = actual_lines
        .lines()
        .map(|line| serde_json::from_str(line).expect("actual event json"))
        .collect();

    assert_eq!(actual, expected);
}

#[test]
fn migration_preserves_existing_event_log() {
    let tmp = tempdir().expect("tempdir");
    let decapod_root = tmp.path();
    let data_dir = decapod_root.join("data");
    fs::create_dir_all(&data_dir).expect("data dir");

    // Legacy DB exists but events file is already populated: migration should no-op.
    let conn = rusqlite::Connection::open(data_dir.join("todo.db")).expect("open db");
    conn.execute_batch(
        "CREATE TABLE tasks (id TEXT PRIMARY KEY, title TEXT, status TEXT, created_at TEXT);",
    )
    .expect("create tasks table");
    let sentinel = "{\"event_type\":\"task.add\",\"task_id\":\"SENTINEL\"}\n";
    fs::write(data_dir.join("todo.events.jsonl"), sentinel).expect("write sentinel events");

    let generated = decapod_root.join("generated");
    fs::create_dir_all(&generated).expect("generated dir");
    fs::write(generated.join("decapod.version"), "0.8.0").expect("write old version");

    migration::check_and_migrate(decapod_root).expect("migration");

    let after = fs::read_to_string(data_dir.join("todo.events.jsonl")).expect("read events");
    assert_eq!(after, sentinel);
}

#[test]
fn repomap_detects_manifests_entrypoints_and_docs() {
    let tmp = tempdir().expect("tempdir");
    let root = tmp.path();

    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::create_dir_all(root.join("docs")).expect("mkdir docs");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='x'\nversion='0.1.0'\n",
    )
    .expect("write Cargo.toml");
    fs::write(root.join("Makefile"), "all:\n\techo ok\n").expect("write Makefile");
    fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("write main.rs");

    fs::write(
        root.join("docs/a.md"),
        "Link: [B](b.md)\nMention docs/c.md\n",
    )
    .expect("write a.md");
    fs::write(root.join("docs/b.md"), "Backlink ../docs/a.md\n").expect("write b.md");
    fs::write(root.join("docs/c.md"), "Leaf\n").expect("write c.md");

    let map = repomap::generate_map(root);
    assert_eq!(map.manifests.get("Cargo.toml"), Some(&"rust".to_string()));
    assert_eq!(map.manifests.get("Makefile"), Some(&"make".to_string()));
    assert!(map.entry_points.contains(&"src/main.rs".to_string()));
    assert!(map.build_hints.contains(&"cargo build".to_string()));
    assert!(map.build_hints.contains(&"make".to_string()));
    assert!(map.skill_hints.contains(&"rust".to_string()));

    let graph = map.doc_graph.expect("doc graph");
    assert!(graph.nodes.iter().any(|n| n == "docs/a.md"));
    assert!(graph.nodes.iter().any(|n| n == "docs/b.md"));
    assert!(
        graph
            .edges
            .iter()
            .any(|(src, dst)| src == "docs/a.md" && dst == "docs/b.md")
    );

    let schema = repomap::schema();
    assert_eq!(schema["name"], "repomap");
}

#[test]
fn scaffold_store_and_docs_cli_behaviors() {
    let tmp = tempdir().expect("tempdir");

    let dry_run_target = tmp.path().join("dry");
    let dry_run_opts = ScaffoldOptions {
        target_dir: dry_run_target.clone(),
        force: false,
        dry_run: true,
        agent_files: vec![],
        created_backups: false,
        all: false,
        generate_specs: true,
        diagram_style: decapod::core::scaffold::DiagramStyle::Ascii,
        specs_seed: None,
    };
    scaffold_project_entrypoints(&dry_run_opts).expect("dry run scaffold");
    assert!(!dry_run_target.join("AGENTS.md").exists());

    let live_target = tmp.path().join("live");
    let live_opts = ScaffoldOptions {
        target_dir: live_target.clone(),
        force: false,
        dry_run: false,
        agent_files: vec![],
        created_backups: false,
        all: false,
        generate_specs: true,
        diagram_style: decapod::core::scaffold::DiagramStyle::Ascii,
        specs_seed: None,
    };
    scaffold_project_entrypoints(&live_opts).expect("live scaffold");
    assert!(live_target.join("AGENTS.md").exists());
    assert!(live_target.join(".decapod/OVERRIDE.md").exists());
    let gitignore = fs::read_to_string(live_target.join(".gitignore")).expect("read .gitignore");
    assert!(
        gitignore.contains(".decapod/generated/*"),
        "decapod init must enforce generated wildcard ignore in .gitignore"
    );
    assert!(
        gitignore.contains("!.decapod/generated/Dockerfile"),
        "decapod init must allowlist generated Dockerfile in .gitignore"
    );
    assert!(
        gitignore.contains("!.decapod/generated/context/*.json"),
        "decapod init must allowlist generated context capsule artifacts in .gitignore"
    );
    assert!(
        gitignore.contains("!.decapod/data/knowledge.promotions.jsonl"),
        "decapod init must allowlist knowledge promotion ledger in .gitignore"
    );
    let generated_dockerfile = live_target.join(".decapod/generated/Dockerfile");
    assert!(
        generated_dockerfile.exists(),
        "decapod init must generate .decapod/generated/Dockerfile"
    );
    let dockerfile_content = fs::read_to_string(&generated_dockerfile).expect("read Dockerfile");
    assert!(
        dockerfile_content.contains("Generated by decapod container profile"),
        "generated Dockerfile must come from Rust template component"
    );
    assert!(
        live_target.join("specs/architecture.md").exists(),
        "decapod init must scaffold specs/architecture.md"
    );
    assert!(
        live_target.join("specs/intent.md").exists(),
        "decapod init must scaffold specs/intent.md"
    );
    let architecture = fs::read_to_string(live_target.join("specs/architecture.md"))
        .expect("read specs/architecture.md");
    assert!(
        architecture.contains("```text"),
        "default diagram style should scaffold ascii topology block"
    );

    // Second run should succeed with checksum verification (files unchanged)
    scaffold_project_entrypoints(&live_opts).expect("second scaffold succeeds when files match");

    let force_opts = ScaffoldOptions {
        target_dir: live_target.clone(),
        force: true,
        dry_run: false,
        agent_files: vec![],
        created_backups: false,
        all: false,
        generate_specs: true,
        diagram_style: decapod::core::scaffold::DiagramStyle::Ascii,
        specs_seed: None,
    };
    scaffold_project_entrypoints(&force_opts).expect("force scaffold");

    let mermaid_target = tmp.path().join("mermaid");
    let mermaid_opts = ScaffoldOptions {
        target_dir: mermaid_target.clone(),
        force: false,
        dry_run: false,
        agent_files: vec![],
        created_backups: false,
        all: false,
        generate_specs: true,
        diagram_style: decapod::core::scaffold::DiagramStyle::Mermaid,
        specs_seed: None,
    };
    scaffold_project_entrypoints(&mermaid_opts).expect("mermaid scaffold");
    let mermaid_arch = fs::read_to_string(mermaid_target.join("specs/architecture.md"))
        .expect("read mermaid architecture");
    assert!(
        mermaid_arch.contains("```mermaid"),
        "mermaid diagram style should scaffold mermaid topology block"
    );

    let store = Store {
        kind: StoreKind::Repo,
        root: live_target,
    };
    let cloned = store.clone();
    assert_eq!(cloned.kind, StoreKind::Repo);
    assert!(cloned.root.exists());

    let docs_schema = docs_cli::schema();
    assert_eq!(docs_schema["name"], "docs");
    docs_cli::run_docs_cli(DocsCli {
        command: DocsCommand::List,
    })
    .expect("docs list");

    // Change to the scaffolded directory for Show commands (which need find_repo_root)
    let original_dir = std::env::current_dir().expect("get current dir");
    std::env::set_current_dir(&store.root).expect("change to scaffolded dir");

    docs_cli::run_docs_cli(DocsCli {
        command: DocsCommand::Show {
            path: "core/DECAPOD.md".to_string(),
            source: docs_cli::DocumentSource::Merged,
        },
    })
    .expect("docs show existing");
    let missing = docs_cli::run_docs_cli(DocsCli {
        command: DocsCommand::Show {
            path: "core/NOPE.md".to_string(),
            source: docs_cli::DocumentSource::Merged,
        },
    });
    assert!(matches!(missing, Err(DecapodError::NotFound(_))));

    // Restore original directory
    std::env::set_current_dir(original_dir).expect("restore original dir");
}

#[test]
fn schemas_errors_and_validate_entrypoint_are_exercised() {
    assert_eq!(schemas::KNOWLEDGE_DB_NAME, "knowledge.db");
    assert_eq!(schemas::TODO_DB_NAME, "todo.db");
    assert_eq!(schemas::TODO_EVENTS_NAME, "todo.events.jsonl");
    assert!(!schemas::TODO_DB_SCHEMA_META.trim().is_empty());
    assert!(!schemas::TODO_DB_SCHEMA_TASKS.trim().is_empty());
    assert!(!schemas::TODO_DB_SCHEMA_TASK_EVENTS.trim().is_empty());
    assert!(!schemas::TODO_DB_SCHEMA_INDEX_STATUS.trim().is_empty());
    assert!(!schemas::TODO_DB_SCHEMA_INDEX_SCOPE.trim().is_empty());
    assert!(!schemas::TODO_DB_SCHEMA_INDEX_DIR.trim().is_empty());
    assert!(!schemas::TODO_DB_SCHEMA_INDEX_EVENTS_TASK.trim().is_empty());
    assert_eq!(schemas::CRON_DB_NAME, "cron.db");
    assert!(!schemas::CRON_DB_SCHEMA.trim().is_empty());
    assert_eq!(schemas::REFLEX_DB_NAME, "reflex.db");
    assert!(!schemas::REFLEX_DB_SCHEMA.trim().is_empty());
    assert_eq!(schemas::HEALTH_DB_NAME, "health.db");
    assert!(!schemas::HEALTH_DB_SCHEMA_CLAIMS.trim().is_empty());
    assert!(!schemas::HEALTH_DB_SCHEMA_PROOF_EVENTS.trim().is_empty());
    assert!(!schemas::HEALTH_DB_SCHEMA_HEALTH_CACHE.trim().is_empty());
    assert_eq!(schemas::POLICY_DB_NAME, "policy.db");
    assert!(!schemas::POLICY_DB_SCHEMA_APPROVALS.trim().is_empty());
    assert!(!schemas::POLICY_DB_SCHEMA_INDEX.trim().is_empty());
    assert_eq!(schemas::ARCHIVE_DB_NAME, "archive.db");
    assert!(!schemas::ARCHIVE_DB_SCHEMA.trim().is_empty());
    assert_eq!(schemas::FEEDBACK_DB_NAME, "feedback.db");
    assert!(!schemas::FEEDBACK_DB_SCHEMA.trim().is_empty());

    let io_err = std::io::Error::other("io boom");
    let from_io: DecapodError = io_err.into();
    assert!(matches!(from_io, DecapodError::IoError(_)));

    let env_err = std::env::var("__DECAPOD_MISSING_ENV_FOR_TEST__").unwrap_err();
    let from_env: DecapodError = env_err.into();
    assert!(matches!(from_env, DecapodError::EnvVarError(_)));

    let tmp = tempdir().expect("tempdir");
    let sqlite_err = rusqlite::Connection::open(tmp.path()).expect_err("opening a directory fails");
    let from_sqlite: DecapodError = sqlite_err.into();
    assert!(matches!(from_sqlite, DecapodError::RusqliteError(_)));

    let repo = tempdir().expect("tempdir");
    fs::create_dir_all(repo.path().join(".decapod/constitution/specs")).expect("mkdir specs");
    fs::write(repo.path().join("AGENTS.md"), "entrypoint\n").expect("write agents");
    fs::write(repo.path().join("CLAUDE.md"), "entrypoint\n").expect("write claude");
    fs::write(repo.path().join("GEMINI.md"), "entrypoint\n").expect("write gemini");
    fs::create_dir_all(repo.path().join(".decapod")).expect("mkdir .decapod");
    fs::write(repo.path().join(".decapod/README.md"), "decapod readme\n").expect("write readme");
    fs::write(
        repo.path().join(".decapod/constitution/specs/INTENT.md"),
        "**Version:** 0.0.1\n",
    )
    .expect("write intent");
    fs::write(
        repo.path()
            .join(".decapod/constitution/specs/ARCHITECTURE.md"),
        "architecture\n",
    )
    .expect("write architecture");
    fs::write(
        repo.path().join(".decapod/constitution/specs/SYSTEM.md"),
        "system\n",
    )
    .expect("write system");

    let store_root = tempdir().expect("store root");
    let store = Store {
        kind: StoreKind::User,
        root: store_root.path().to_path_buf(),
    };

    let result = validate::run_validation(&store, repo.path(), repo.path(), false);
    assert!(result.is_err());
}

#[test]
fn override_md_extraction_and_merging() {
    let tmp = tempdir().expect("tempdir");
    let root = tmp.path();

    // Create .decapod directory
    fs::create_dir_all(root.join(".decapod")).expect("mkdir .decapod");

    // Create OVERRIDE.md with test overrides
    let override_content = r#"# OVERRIDE.md - Project-Specific Decapod Overrides

---

<!-- CHANGES ARE NOT PERMITTED ABOVE THIS LINE -->

## Core Overrides

### core/DECAPOD.md

## Custom Navigation

This is a test override for DECAPOD.md

### core/CONTROL_PLANE.md

## Custom Control Plane

This is a test override for CONTROL_PLANE.md

---

## Plugin Overrides

### plugins/TODO.md

## Custom TODO Priorities

- critical
- high
- medium
"#;

    fs::write(root.join(".decapod/OVERRIDE.md"), override_content).expect("write OVERRIDE.md");

    // Test override extraction for specific components
    let decapod_override = assets::get_override_doc(root, "core/DECAPOD.md");
    assert!(decapod_override.is_some());
    assert!(decapod_override.unwrap().contains("Custom Navigation"));

    let control_plane_override = assets::get_override_doc(root, "core/CONTROL_PLANE.md");
    assert!(control_plane_override.is_some());
    assert!(
        control_plane_override
            .unwrap()
            .contains("Custom Control Plane")
    );

    let todo_override = assets::get_override_doc(root, "plugins/TODO.md");
    assert!(todo_override.is_some());
    assert!(todo_override.unwrap().contains("Custom TODO Priorities"));

    // Test that non-existent override returns None
    let missing_override = assets::get_override_doc(root, "plugins/NONEXISTENT.md");
    assert!(missing_override.is_none());

    // Test merged document (embedded + override)
    let merged_todo = assets::get_merged_doc(root, "plugins/TODO.md");
    assert!(merged_todo.is_some());
    let merged_content = merged_todo.unwrap();
    // Should contain both embedded content and override
    assert!(merged_content.contains("## Project Overrides"));
    assert!(merged_content.contains("Custom TODO Priorities"));
}

#[test]
fn override_md_checksum_caching() {
    let tmp = tempdir().expect("tempdir");
    let root = tmp.path();

    // Create .decapod/generated directory
    fs::create_dir_all(root.join(".decapod/generated")).expect("mkdir generated");

    // Create OVERRIDE.md
    let override_content_v1 = "# Test OVERRIDE.md v1\n\nContent version 1";
    fs::write(root.join(".decapod/OVERRIDE.md"), override_content_v1)
        .expect("write OVERRIDE.md v1");

    // Calculate checksum manually
    use sha2::{Digest, Sha256};
    let hash_v1 = Sha256::digest(override_content_v1.as_bytes());
    let checksum_v1 = format!("{:x}", hash_v1);

    // Cache the checksum
    fs::write(
        root.join(".decapod/generated/override.checksum"),
        &checksum_v1,
    )
    .expect("write checksum v1");

    // Read cached checksum
    let cached = fs::read_to_string(root.join(".decapod/generated/override.checksum"))
        .expect("read cached checksum");
    assert_eq!(cached, checksum_v1);

    // Modify OVERRIDE.md
    let override_content_v2 = "# Test OVERRIDE.md v2\n\nContent version 2 (changed)";
    fs::write(root.join(".decapod/OVERRIDE.md"), override_content_v2)
        .expect("write OVERRIDE.md v2");

    // Calculate new checksum
    let hash_v2 = Sha256::digest(override_content_v2.as_bytes());
    let checksum_v2 = format!("{:x}", hash_v2);

    // Verify checksums are different
    assert_ne!(checksum_v1, checksum_v2);

    // Update cache
    fs::write(
        root.join(".decapod/generated/override.checksum"),
        &checksum_v2,
    )
    .expect("write checksum v2");

    // Verify cache updated
    let cached_v2 = fs::read_to_string(root.join(".decapod/generated/override.checksum"))
        .expect("read cached checksum v2");
    assert_eq!(cached_v2, checksum_v2);
}

#[test]
fn override_md_empty_sections_return_none() {
    let tmp = tempdir().expect("tempdir");
    let root = tmp.path();

    fs::create_dir_all(root.join(".decapod")).expect("mkdir .decapod");

    // Create OVERRIDE.md with empty sections
    let override_content = r#"# OVERRIDE.md

<!-- CHANGES ARE NOT PERMITTED ABOVE THIS LINE -->

## Core Overrides

### core/DECAPOD.md

### core/CONTROL_PLANE.md

Some content here

### core/PLUGINS.md
"#;

    fs::write(root.join(".decapod/OVERRIDE.md"), override_content).expect("write OVERRIDE.md");

    // Empty section should return None
    let empty_override = assets::get_override_doc(root, "core/DECAPOD.md");
    assert!(empty_override.is_none());

    // Non-empty section should return Some
    let non_empty_override = assets::get_override_doc(root, "core/CONTROL_PLANE.md");
    assert!(non_empty_override.is_some());
    assert!(non_empty_override.unwrap().contains("Some content here"));

    // Empty section at end should return None
    let end_empty_override = assets::get_override_doc(root, "core/PLUGINS.md");
    assert!(end_empty_override.is_none());
}

#[test]
fn override_md_ignores_template_examples() {
    let tmp = tempdir().expect("tempdir");
    let root = tmp.path();

    fs::create_dir_all(root.join(".decapod")).expect("mkdir .decapod");

    // Create OVERRIDE.md with examples in the header (before the marker)
    let override_content = r#"# OVERRIDE.md

## How to Use

Example:

```markdown
### plugins/TODO.md

This is just an example in the instructions
```

<!-- CHANGES ARE NOT PERMITTED ABOVE THIS LINE -->

## Plugin Overrides

### plugins/TODO.md

This is the ACTUAL override content
"#;

    fs::write(root.join(".decapod/OVERRIDE.md"), override_content).expect("write OVERRIDE.md");

    // Should extract the actual override, not the example
    let override_doc = assets::get_override_doc(root, "plugins/TODO.md");
    assert!(override_doc.is_some());
    let content = override_doc.unwrap();
    assert!(content.contains("ACTUAL override content"));
    assert!(!content.contains("just an example"));
}
