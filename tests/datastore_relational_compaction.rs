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

#[test]
fn composite_keys_compact_without_losing_operator_bytes_or_foreign_keys() {
    let temp = tempfile::tempdir().unwrap();
    let conn = fixture(temp.path(), true);
    conn.execute_batch(
        "CREATE TABLE meta(namespace TEXT NOT NULL,key TEXT NOT NULL,value TEXT NOT NULL,
         PRIMARY KEY(namespace,key));
         CREATE TABLE task_tags(task_id TEXT NOT NULL,tag TEXT NOT NULL,
         PRIMARY KEY(task_id,tag),
         FOREIGN KEY(task_id) REFERENCES tasks(id) ON DELETE CASCADE);
         CREATE INDEX idx_task_tags_tag ON task_tags(tag);
         INSERT INTO meta VALUES ('operator','fingerprint','  raw  value\n');
         INSERT INTO meta VALUES ('aptitude','pattern:alpha','{ \"key\": \"value\" }');
         INSERT INTO task_tags VALUES ('t1','size-first');
         INSERT INTO task_tags VALUES ('t1','proof:required');",
    ).unwrap();
    let meta_before: Vec<(String, String, String)> = conn
        .prepare("SELECT namespace,key,value FROM meta ORDER BY namespace,key").unwrap()
        .query_map([], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap()
        .collect::<Result<_, _>>().unwrap();
    let tags_before: Vec<(String, String)> = conn
        .prepare("SELECT task_id,tag FROM task_tags ORDER BY task_id,tag").unwrap()
        .query_map([], |r| Ok((r.get(0)?,r.get(1)?))).unwrap()
        .collect::<Result<_, _>>().unwrap();
    compact(temp.path()).unwrap();
    compact(temp.path()).unwrap();
    for table in ["meta", "task_tags"] {
        let sql = "SELECT wr FROM pragma_table_list WHERE name=?1 AND \"schema\"='main'";
        let without_rowid: i64 = conn.query_row(sql,[table],|r|r.get(0)).unwrap();
        assert_eq!(without_rowid, 1, "{table} still stores duplicate rowid B-tree");
    }
    let meta_after: Vec<(String, String, String)> = conn
        .prepare("SELECT namespace,key,value FROM meta ORDER BY namespace,key").unwrap()
        .query_map([], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap()
        .collect::<Result<_, _>>().unwrap();
    let tags_after: Vec<(String, String)> = conn
        .prepare("SELECT task_id,tag FROM task_tags ORDER BY task_id,tag").unwrap()
        .query_map([], |r| Ok((r.get(0)?,r.get(1)?))).unwrap()
        .collect::<Result<_, _>>().unwrap();
    assert_eq!(meta_after, meta_before);
    assert_eq!(tags_after, tags_before);
    assert!(has_index(&conn,"idx_task_tags_tag"));
    let violations: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pragma_foreign_key_check",[],|r|r.get(0)).unwrap();
    assert_eq!(violations,0);
}

#[test]
fn unexpected_meta_index_rolls_back_every_index_and_row_rewrite() {
    let temp = tempfile::tempdir().unwrap();
    let conn = fixture(temp.path(), true);
    conn.execute_batch(
        "CREATE TABLE meta(namespace TEXT NOT NULL,key TEXT NOT NULL,value TEXT NOT NULL,
         PRIMARY KEY(namespace,key));
         CREATE INDEX idx_external_meta_value ON meta(value);
         INSERT INTO meta VALUES ('operator','authored','must survive');",
    ).unwrap();
    let before=contents_digest(&conn);
    let error=compact(temp.path()).unwrap_err().to_string();
    assert!(error.contains("RELATIONAL_COMPACTION_UNSAFE"),"{error}");
    assert_eq!(contents_digest(&conn),before);
    assert!(has_index(&conn,"idx_categories_name"));
    assert!(has_index(&conn,"idx_external_meta_value"));
    let wr:i64=conn.query_row(
        "SELECT wr FROM pragma_table_list WHERE name='meta'",[],|r|r.get(0)).unwrap();
    assert_eq!(wr,0);
    assert!(conn.is_autocommit());
}

#[test]
fn cold_summary_migration_compresses_losslessly_and_is_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let conn = fixture(temp.path(), true);
    conn.execute_batch(schemas::LCM_DB_SCHEMA_SUMMARIES).unwrap();
    let original = "proof  and exact historical context\n".repeat(1200);
    let hashes = serde_json::to_string(&vec!["sha256:abc"; 350]).unwrap();
    conn.execute(
        "INSERT INTO summaries(summary_hash,ts,scope,original_hashes,summary_text,token_estimate)
         VALUES('historic','2026-11-10','all',?1,?2,10000)",
        decapod::params![hashes,original],
    ).unwrap();
    compact(temp.path()).unwrap();
    compact(temp.path()).unwrap();
    let stored = conn.query_row(
        "SELECT original_hashes,summary_text FROM summaries WHERE summary_hash='historic'",
        [],|r|Ok(r.parameters())).unwrap();
    assert_eq!(decapod::core::storage_codec::decode(&stored[0]).unwrap(),hashes);
    assert_eq!(decapod::core::storage_codec::decode(&stored[1]).unwrap(),original);
    let physical:String=conn.query_row(
        "SELECT typeof(summary_text) FROM summaries WHERE summary_hash='historic'",
        [],|r|r.get(0)).unwrap();
    assert_eq!(physical,"blob");
    let integrity:String=conn.query_row("PRAGMA integrity_check",[],|r|r.get(0)).unwrap();
    assert_eq!(integrity,"ok");
}

#[test]
fn broken_summary_blob_fails_closed_and_restores_index_history() {
    let temp=tempfile::tempdir().unwrap();
    let conn=fixture(temp.path(),true);
    conn.execute_batch(schemas::LCM_DB_SCHEMA_SUMMARIES).unwrap();
    conn.execute(
        "INSERT INTO summaries(summary_hash,ts,scope,original_hashes,summary_text,token_estimate)
         VALUES('bad','now','all','[]',?1,3)",
        decapod::params![b"bogus-envelope".to_vec()],
    ).unwrap();
    let error=compact(temp.path()).unwrap_err().to_string();
    assert!(error.contains("LCM_STORAGE_CODEC_CORRUPT"),"{error}");
    assert!(has_index(&conn,"idx_categories_name"));
    assert!(conn.is_autocommit());
}

#[test]
fn default_agent_prompts_do_not_accumulate_on_each_initialization() {
    let temp=tempfile::tempdir().unwrap();
    let data=temp.path().join("data");
    decapod::plugins::aptitude::initialize_aptitude_db(&data).unwrap();
    let conn=Connection::open(data.join("decapod.db")).unwrap();
    let before:i64=conn.query_row("SELECT count(*) FROM agent_prompts",[],|r|r.get(0)).unwrap();
    assert!(before>0);
    for _ in 0..4 {
        decapod::plugins::aptitude::initialize_aptitude_db(&data).unwrap();
    }
    let after:i64=conn.query_row("SELECT count(*) FROM agent_prompts",[],|r|r.get(0)).unwrap();
    assert_eq!(after,before);
}
