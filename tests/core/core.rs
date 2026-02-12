use decapod::core::assets;
use decapod::core::broker::{self, BrokerEvent, DbBroker};
use decapod::core::db;
use decapod::core::docs_cli::{self, DocsCli, DocsCommand};
use decapod::core::error::DecapodError;
use decapod::core::repomap;
use decapod::core::scaffold::{ScaffoldOptions, scaffold_project_entrypoints};
use decapod::core::schemas;
use decapod::core::store::{Store, StoreKind};
use decapod::core::validate;
use rusqlite::params;
use std::fs;
use tempfile::tempdir;

#[test]
fn assets_docs_and_templates_resolve() {
    let docs = assets::list_docs();
    assert!(docs.contains(&"embedded/core/DECAPOD.md".to_string()));

    for doc in docs {
        let content = assets::get_doc(&doc).expect("listed doc should be readable");
        assert!(!content.trim().is_empty());
    }

    let template_names = [
        "AGENTS.md",
        "CLAUDE.md",
        "GEMINI.md",
        "README.md",
        "core/CONTROL_PLANE.md",
        "core/DECAPOD.md",
        "core/PLUGINS.md",
        "core/CLAIMS.md",
        "core/DEMANDS.md",
        "core/DEPRECATION.md",
        "core/DOC_RULES.md",
        "core/GLOSSARY.md",
        "core/KNOWLEDGE.md",
        "core/MEMORY.md",
        "core/SOUL.md",
        "core/STORE_MODEL.md",
        "specs/AMENDMENTS.md",
        "specs/ARCHITECTURE.md",
        "specs/INTENT.md",
        "specs/SYSTEM.md",
        "plugins/DB_BROKER.md",
        "plugins/MANIFEST.md",
        "plugins/TODO.md",
        "plugins/TODO_USER.md",
        "plugins/CRON.md",
        "plugins/REFLEX.md",
        "plugins/HEALTH.md",
        "plugins/POLICY.md",
        "plugins/WATCHER.md",
        "plugins/KNOWLEDGE.md",
        "plugins/ARCHIVE.md",
        "plugins/FEEDBACK.md",
        "plugins/TRUST.md",
        "plugins/CONTEXT.md",
        "plugins/HEARTBEAT.md",
    ];

    for template in template_names {
        let content = assets::get_template(template).expect("template should exist");
        assert!(!content.trim().is_empty());
    }

    assert!(assets::get_doc("embedded/core/DOES_NOT_EXIST.md").is_none());
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

    let schema = broker::schema();
    assert_eq!(schema["name"], "broker");
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
    };
    scaffold_project_entrypoints(&dry_run_opts).expect("dry run scaffold");
    assert!(!dry_run_target.join("AGENTS.md").exists());

    let live_target = tmp.path().join("live");
    let live_opts = ScaffoldOptions {
        target_dir: live_target.clone(),
        force: false,
        dry_run: false,
    };
    scaffold_project_entrypoints(&live_opts).expect("live scaffold");
    assert!(live_target.join("AGENTS.md").exists());
    assert!(
        live_target
            .join(".decapod/constitution/specs/INTENT.md")
            .exists()
    );

    let second = scaffold_project_entrypoints(&live_opts);
    assert!(matches!(second, Err(DecapodError::ValidationError(_))));

    let force_opts = ScaffoldOptions {
        target_dir: live_target.clone(),
        force: true,
        dry_run: false,
    };
    scaffold_project_entrypoints(&force_opts).expect("force scaffold");

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
    assert_eq!(schemas::TODO_SCHEMA_VERSION, 1);
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

    let result = validate::run_validation(&store, repo.path(), repo.path());
    assert!(result.is_err());
}
