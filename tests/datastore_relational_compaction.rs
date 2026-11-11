//! Disposable SQLite fixtures for prefix-index compaction. No user DB access.
use decapod::core::{db::Connection, error::DecapodError, migration, schemas};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

const MIGRATION: &str = "sqlite.compact_redundant_relational_indexes.v001";
const OBSOLETE: [&str; 5] = [
    "idx_categories_name",
    "idx_risk_zones_name",
    "idx_task_owners_task",
    "idx_task_dependencies_task",
    "idx_preferences_category",
];

fn compact(root: &Path) -> Result<(), DecapodError> {
    (migration::all_migrations()
        .into_iter()
        .find(|migration| migration.id == MIGRATION)
        .expect("migration registered")
        .up)(root)
}

fn fixture(root: &Path, historical: bool) -> Connection {
    fs::create_dir_all(root.join("data")).unwrap();
    let conn = Connection::open(root.join("data/decapod.db")).unwrap();
    for ddl in [
        schemas::TODO_DB_SCHEMA_TASKS,
        schemas::TODO_DB_SCHEMA_CATEGORIES,
        schemas::TODO_DB_SCHEMA_RISK_ZONES,
        schemas::TODO_DB_SCHEMA_TASK_OWNERS,
        schemas::TODO_DB_SCHEMA_TASK_DEPENDENCIES,
        schemas::APTITUDE_DB_SCHEMA_PREFERENCES,
        schemas::TODO_DB_SCHEMA_INDEX_TASK_OWNERS_UNIQUE,
    ] {
        conn.execute_batch(ddl).unwrap();
    }
    if historical {
        conn.execute_batch(
            "CREATE INDEX idx_categories_name ON categories(name);
             CREATE INDEX idx_risk_zones_name ON risk_zones(zone_name);
             CREATE INDEX idx_task_owners_task ON task_owners(task_id);
             CREATE INDEX idx_task_dependencies_task ON task_dependencies(task_id);
             CREATE INDEX idx_preferences_category ON preferences(category);",
        )
        .unwrap();
    }
    conn.execute_batch(
        "INSERT INTO categories(id,name,created_at) VALUES('c1','fast','now');
         INSERT INTO risk_zones(id,zone_name,created_at) VALUES('r1','infra','now');
         INSERT INTO tasks(id,hash,title,created_at,updated_at,dir_path,scope)
           VALUES('t1','hash','task','now','now','','');
         INSERT INTO tasks(id,hash,title,created_at,updated_at,dir_path,scope)
           VALUES('t2','hash2','dependent','now','now','','');
         INSERT INTO task_owners(id,task_id,agent_id,claimed_at)
           VALUES('o1','t1','agent1','now');
         INSERT INTO task_dependencies(id,task_id,depends_on_task_id,created_at)
           VALUES('d1','t1','t2','now');
         INSERT INTO preferences(id,category,key,value,source,created_at)
           VALUES('p1','git','sign','yes','operator','now');",
    )
    .unwrap();
    conn
}

fn has_index(conn: &Connection, index: &str) -> bool {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='index' AND name=?1)",
        [index],
        |row| row.get(0),
    )
    .unwrap()
}

fn contents_digest(conn: &Connection) -> String {
    let mut digest = Sha256::new();
    for table in [
        "categories",
        "risk_zones",
        "tasks",
        "task_owners",
        "task_dependencies",
        "preferences",
    ] {
        digest.update(table.as_bytes());
        let mut stmt = conn
            .prepare(&format!("SELECT * FROM \"{table}\" ORDER BY rowid"))
            .unwrap();
        let mut rows = stmt.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            digest.update(serde_json::to_vec(&row.parameters()).unwrap());
            digest.update(b"\n");
        }
    }
    format!("{:x}", digest.finalize())
}

#[test]
fn historical_compaction_preserves_data_and_unique_semantics() {
    let temp = tempfile::tempdir().unwrap();
    let conn = fixture(temp.path(), true);
    let before = contents_digest(&conn);
    for index in OBSOLETE {
        assert!(has_index(&conn, index));
    }
    compact(temp.path()).unwrap();
    compact(temp.path()).unwrap();
    assert_eq!(contents_digest(&conn), before);
    for index in OBSOLETE {
        assert!(!has_index(&conn, index));
    }
    assert!(has_index(&conn, "idx_task_owners_task_agent"));
    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .unwrap();
    assert_eq!(integrity, "ok");
    assert!(
        conn.execute(
            "INSERT INTO categories(id,name,created_at) VALUES('c2','fast','now')",
            [],
        )
        .is_err()
    );
    assert!(
        conn.execute(
            "INSERT INTO task_owners(id,task_id,agent_id,claimed_at)
             VALUES('o2','t1','agent1','now')",
            [],
        )
        .is_err()
    );
}

#[test]
fn unexpected_index_definition_fails_closed() {
    let temp = tempfile::tempdir().unwrap();
    let conn = fixture(temp.path(), true);
    conn.execute_batch(
        "DROP INDEX idx_categories_name;
         CREATE INDEX idx_categories_name ON categories(id)",
    )
    .unwrap();
    let digest = contents_digest(&conn);
    assert!(
        compact(temp.path())
            .unwrap_err()
            .to_string()
            .contains("RELATIONAL_INDEX_COMPACTION_UNSAFE")
    );
    for index in OBSOLETE {
        assert!(has_index(&conn, index));
    }
    assert_eq!(contents_digest(&conn), digest);
    assert!(conn.is_autocommit());
}

#[test]
fn missing_unique_replacement_rolls_back_every_prior_drop() {
    let temp = tempfile::tempdir().unwrap();
    let conn = fixture(temp.path(), true);
    conn.execute_batch("DROP INDEX idx_task_owners_task_agent")
        .unwrap();
    assert!(
        compact(temp.path())
            .unwrap_err()
            .to_string()
            .contains("RELATIONAL_INDEX_COMPACTION_UNSAFE")
    );
    for index in OBSOLETE {
        assert!(has_index(&conn, index));
    }
    assert!(conn.is_autocommit());
}

#[test]
fn already_compact_schema_remains_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let conn = fixture(temp.path(), false);
    let digest = contents_digest(&conn);
    compact(temp.path()).unwrap();
    assert_eq!(contents_digest(&conn), digest);
    for index in OBSOLETE {
        assert!(!has_index(&conn, index));
    }
}

#[test]
fn repeated_subsystem_initialization_does_not_recreate_obsolete_indexes() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    decapod::core::todo::initialize_todo_db(&data).unwrap();
    decapod::plugins::aptitude::initialize_aptitude_db(&data).unwrap();
    decapod::plugins::policy::initialize_policy_db(&data).unwrap();
    compact(temp.path()).unwrap();

    // These initializers are reachable during normal runtime and cannot undo
    // compaction after it has been recorded in the migration ledger.
    decapod::core::todo::initialize_todo_db(&data).unwrap();
    decapod::plugins::aptitude::initialize_aptitude_db(&data).unwrap();
    decapod::plugins::policy::initialize_policy_db(&data).unwrap();
    let conn = Connection::open(data.join(schemas::LOCAL_DB_NAME)).unwrap();
    for index in OBSOLETE {
        assert!(!has_index(&conn, index), "{index} was recreated");
    }
    assert!(has_index(&conn, "idx_task_owners_task_agent"));
}
