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
    )
    .unwrap();
    let meta_before: Vec<(String, String, String)> = conn
        .prepare("SELECT namespace,key,value FROM meta ORDER BY namespace,key")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let tags_before: Vec<(String, String)> = conn
        .prepare("SELECT task_id,tag FROM task_tags ORDER BY task_id,tag")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    compact(temp.path()).unwrap();
    compact(temp.path()).unwrap();
    for table in ["meta", "task_tags"] {
        let sql = "SELECT wr FROM pragma_table_list WHERE name=?1 AND \"schema\"='main'";
        let without_rowid: i64 = conn.query_row(sql, [table], |r| r.get(0)).unwrap();
        assert_eq!(
            without_rowid, 1,
            "{table} still stores duplicate rowid B-tree"
        );
    }
    let meta_after: Vec<(String, String, String)> = conn
        .prepare("SELECT namespace,key,value FROM meta ORDER BY namespace,key")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let tags_after: Vec<(String, String)> = conn
        .prepare("SELECT task_id,tag FROM task_tags ORDER BY task_id,tag")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(meta_after, meta_before);
    assert_eq!(tags_after, tags_before);
    assert!(has_index(&conn, "idx_task_tags_tag"));
    let violations: i64 = conn
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(violations, 0);
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
    )
    .unwrap();
    let before = contents_digest(&conn);
    let error = compact(temp.path()).unwrap_err().to_string();
    assert!(error.contains("RELATIONAL_COMPACTION_UNSAFE"), "{error}");
    assert_eq!(contents_digest(&conn), before);
    assert!(has_index(&conn, "idx_categories_name"));
    assert!(has_index(&conn, "idx_external_meta_value"));
    let wr: i64 = conn
        .query_row(
            "SELECT wr FROM pragma_table_list WHERE name='meta'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(wr, 0);
    assert!(conn.is_autocommit());
}

#[test]
fn cold_summary_migration_compresses_losslessly_and_is_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let conn = fixture(temp.path(), true);
    conn.execute_batch(schemas::LCM_DB_SCHEMA_SUMMARIES)
        .unwrap();
    let original = "proof  and exact historical context\n".repeat(1200);
    let hashes = serde_json::to_string(&vec!["sha256:abc"; 350]).unwrap();
    conn.execute(
        "INSERT INTO summaries(summary_hash,ts,scope,original_hashes,summary_text,token_estimate)
         VALUES('historic','2026-11-10','all',?1,?2,10000)",
        decapod::params![hashes, original],
    )
    .unwrap();
    compact(temp.path()).unwrap();
    compact(temp.path()).unwrap();
    let stored = conn
        .query_row(
            "SELECT original_hashes,summary_text FROM summaries WHERE summary_hash='historic'",
            [],
            |r| Ok(r.parameters()),
        )
        .unwrap();
    assert_eq!(
        decapod::core::storage_codec::decode(&stored[0]).unwrap(),
        hashes
    );
    assert_eq!(
        decapod::core::storage_codec::decode(&stored[1]).unwrap(),
        original
    );
    let physical: String = conn
        .query_row(
            "SELECT typeof(summary_text) FROM summaries WHERE summary_hash='historic'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(physical, "blob");
    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    assert_eq!(integrity, "ok");
}

#[test]
fn broken_summary_blob_fails_closed_and_restores_index_history() {
    let temp = tempfile::tempdir().unwrap();
    let conn = fixture(temp.path(), true);
    conn.execute_batch(schemas::LCM_DB_SCHEMA_SUMMARIES)
        .unwrap();
    conn.execute(
        "INSERT INTO summaries(summary_hash,ts,scope,original_hashes,summary_text,token_estimate)
         VALUES('bad','now','all','[]',?1,3)",
        decapod::params![b"bogus-envelope".to_vec()],
    )
    .unwrap();
    let error = compact(temp.path()).unwrap_err().to_string();
    assert!(error.contains("LCM_STORAGE_CODEC_CORRUPT"), "{error}");
    assert!(has_index(&conn, "idx_categories_name"));
    assert!(conn.is_autocommit());
}

#[test]
fn default_agent_prompts_do_not_accumulate_on_each_initialization() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    decapod::plugins::aptitude::initialize_aptitude_db(&data).unwrap();
    let conn = Connection::open(data.join("decapod.db")).unwrap();
    let before: i64 = conn
        .query_row("SELECT count(*) FROM agent_prompts", [], |r| r.get(0))
        .unwrap();
    assert!(before > 0);
    for _ in 0..4 {
        decapod::plugins::aptitude::initialize_aptitude_db(&data).unwrap();
    }
    let after: i64 = conn
        .query_row("SELECT count(*) FROM agent_prompts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(after, before);
}

#[test]
fn v2_compaction_migration_1200_is_lossless_idempotent_deduplicates_prompts_and_records_manifest() {
    use decapod::core::{events, storage_compaction, todo};
    use serde_json::{Value, json};

    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    todo::initialize_todo_db(&data).unwrap();
    decapod::plugins::aptitude::initialize_aptitude_db(&data).unwrap();
    decapod::plugins::federation::initialize_federation_db(&data).unwrap();

    let conn = Connection::open(data.join(schemas::LOCAL_DB_NAME)).unwrap();
    // Simulate pre-v2 verbose JSON envelopes stored in events across streams,
    // including an extra top-level field that triggers `_ce_raw` lossless escape,
    // and a federation row that must remain untouched.
    let verbose_todo = json!({
        "event_id": "legacy-verbose-todo-1",
        "ts": "2026-11-10T01:02:03Z",
        "event_type": "task.comment",
        "actor": "legacy-agent",
        "subject_kind": "task",
        "subject_id": "task-legacy-1",
        "payload": {"comment": "Verbose historical comment preserved byte-for-byte"}
    })
    .to_string();
    let verbose_custom_key = json!({
        "event_id": "legacy-custom-top-1",
        "ts": "2026-11-10T01:02:04Z",
        "event_type": "broker.audit",
        "actor": "legacy-broker",
        "custom_top_level_key": ["must", "survive", 42],
        "payload": {"op": "todo.claim"}
    })
    .to_string();
    let federation_signed = json!({
        "event_id": "fed-signed-1",
        "ts": "2026-11-10T01:02:05Z",
        "event_type": "node.add",
        "actor": "fed-peer",
        "payload": {"node_id": "n1"}
    })
    .to_string();

    conn.execute(
        "INSERT INTO events(event_id,ts,seq,stream,subject_kind,subject_id,event_type,payload,actor)
         VALUES('legacy-verbose-todo-1','2026-11-10T01:02:03Z',100,'todo','task','task-legacy-1','task.comment',?1,'legacy-agent')",
        decapod::params![verbose_todo],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO events(event_id,ts,seq,stream,subject_kind,subject_id,event_type,payload,actor)
         VALUES('legacy-custom-top-1','2026-11-10T01:02:04Z',100,'broker',NULL,NULL,'broker.audit',?1,'legacy-broker')",
        decapod::params![verbose_custom_key],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO events(event_id,ts,seq,stream,subject_kind,subject_id,event_type,payload,actor)
         VALUES('fed-signed-1','2026-11-10T01:02:05Z',1,'federation',NULL,NULL,'node.add',?1,'fed-peer')",
        decapod::params![federation_signed],
    )
    .unwrap();
    events::append_on_conn(
        &conn,
        "broker",
        &json!({
            "event_id": "escaped-literal-sentinel-1",
            "ts": "2026-11-10T01:02:06Z",
            "event_type": "broker.audit",
            "actor": "legacy-broker",
            "_ce_v1": "caller-literal-key-collision",
            "payload": {"op": "todo.list"}
        }),
    )
    .unwrap();

    // Insert historical duplicate agent_prompts rows with non-zero usage counts.
    conn.execute(
        "INSERT INTO agent_prompts(id,context,prompt_text,priority,active,usage_count,last_shown_at,created_at,updated_at)
         VALUES('dup-a','custom_ctx','Exact repeated guidance',80,1,3,'2026-11-01T00:00:00Z','2026-11-01T00:00:00Z','2026-11-01T00:00:00Z')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO agent_prompts(id,context,prompt_text,priority,active,usage_count,last_shown_at,created_at,updated_at)
         VALUES('dup-b','custom_ctx','Exact repeated guidance',80,1,5,'2026-11-05T00:00:00Z','2026-11-02T00:00:00Z','2026-11-05T00:00:00Z')",
        [],
    )
    .unwrap();

    let logical_before = |conn: &Connection| -> Vec<(String, Value)> {
        let mut stmt = conn
            .prepare("SELECT stream,event_id,ts,event_type,actor,subject_kind,subject_id,payload FROM events ORDER BY stream,seq")
            .unwrap();
        stmt.query_map([], |r| {
            let stream: String = r.get(0)?;
            let event_id: String = r.get(1)?;
            let ts: String = r.get(2)?;
            let event_type: String = r.get(3)?;
            let actor: String = r.get(4)?;
            let sk: Option<String> = r.get(5)?;
            let sid: Option<String> = r.get(6)?;
            let raw: String = r.get(7)?;
            let val = events::reconstruct_stored_payload(
                &stream,
                &event_id,
                &ts,
                &event_type,
                &actor,
                sk.as_deref(),
                sid.as_deref(),
                &raw,
            )
            .unwrap();
            Ok((event_id, val))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
    };
    let expected_logical = logical_before(&conn);

    let run_v2 = |root: &Path| -> Result<(), DecapodError> {
        (migration::all_migrations()
            .into_iter()
            .find(|m| m.id == storage_compaction::V2_COMPACTION_MIGRATION_ID)
            .expect("migration 1200 registered")
            .up)(root)
    };

    run_v2(temp.path()).unwrap();
    // Second invocation proves idempotency with zero drift.
    run_v2(temp.path()).unwrap();

    assert_eq!(logical_before(&conn), expected_logical);

    // Physical representation checks:
    let raw_todo: String = conn
        .query_row(
            "SELECT payload FROM events WHERE event_id='legacy-verbose-todo-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(raw_todo.contains("\"_ce_v1\""), "{raw_todo}");
    assert!(raw_todo.len() < verbose_todo.len());

    let raw_custom: String = conn
        .query_row(
            "SELECT payload FROM events WHERE event_id='legacy-custom-top-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(raw_custom.contains("\"_ce_v1\""), "{raw_custom}");
    assert!(
        raw_custom.contains("\"custom_top_level_key\""),
        "{raw_custom}"
    );

    let raw_escaped: String = conn
        .query_row(
            "SELECT payload FROM events WHERE event_id='escaped-literal-sentinel-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(raw_escaped.contains("\"_ce_raw\""), "{raw_escaped}");

    let raw_fed: String = conn
        .query_row(
            "SELECT payload FROM events WHERE event_id='fed-signed-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(raw_fed, federation_signed);

    // Duplicate agent_prompts collapsed while summing usage_count (3 + 5 = 8) and keeping earliest created_at.
    let (cnt, usage_sum, created): (i64, i64, String) = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(usage_count),0), MIN(created_at) FROM agent_prompts WHERE context='custom_ctx'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(cnt, 1);
    assert_eq!(usage_sum, 8);
    assert_eq!(created, "2026-11-01T00:00:00Z");

    // Manifest & compatibility stamps recorded:
    let (pre_d, post_d): (String, String) = conn
        .query_row(
            "SELECT pre_digest, post_digest FROM storage_migration_manifests WHERE migration_id=?1",
            [storage_compaction::V2_COMPACTION_MIGRATION_ID],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(pre_d, post_d);
    let fmt_ver: String = conn
        .query_row(
            "SELECT value FROM meta WHERE namespace='storage' AND key='format_version'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(fmt_ver, "2");
}

#[test]
fn storage_format_compatibility_guard_refuses_future_format_or_writer_downgrade() {
    use decapod::core::{storage_compaction, todo};

    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    todo::initialize_todo_db(&data).unwrap();
    let conn = Connection::open(data.join(schemas::LOCAL_DB_NAME)).unwrap();

    // Higher min_writer_version allows read-only compatibility while refusing write initialization.
    conn.execute(
        "UPDATE meta SET value='99.0.0' WHERE namespace='storage' AND key='min_writer_version'",
        [],
    )
    .unwrap();
    storage_compaction::check_read_compatibility(&conn).unwrap();
    let write_err = storage_compaction::check_storage_format_compatibility(&conn).unwrap_err();
    assert!(
        write_err
            .to_string()
            .contains("STORAGE_FORMAT_INCOMPATIBLE")
    );
    let init_err = todo::initialize_todo_db(&data).unwrap_err();
    assert!(init_err.to_string().contains("STORAGE_FORMAT_INCOMPATIBLE"));

    // Future format_version refuses both readers and writers with clear remediation.
    conn.execute(
        "UPDATE meta SET value='99' WHERE namespace='storage' AND key='format_version'",
        [],
    )
    .unwrap();
    let read_err = storage_compaction::check_read_compatibility(&conn).unwrap_err();
    assert!(read_err.to_string().contains("STORAGE_FORMAT_INCOMPATIBLE"));
}

#[test]
fn reference_aware_component_gc_and_explicit_vacuum_preserve_live_and_historical_refs() {
    use decapod::core::{storage_compaction, todo};

    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    todo::initialize_todo_db(&data).unwrap();

    let desc_v1 = "Alpha historical component prose.\n".repeat(48);
    let desc_v2 = "Beta live component prose.\n".repeat(48);
    let added = todo::add_task(
        &data,
        &todo::TodoCommand::Add {
            title: "Component GC task".into(),
            description: desc_v1,
            priority: "high".into(),
            tags: "gc,v2".into(),
            owner: String::new(),
            due: None,
            r#ref: String::new(),
            scope: format!(
                "paths:{}",
                serde_json::to_string(&vec![
                    "src/decapod/core/storage_compaction.rs",
                    "src/decapod/core/task_journal.rs",
                    "src/decapod/core/events.rs",
                    "src/decapod/core/schemas.rs",
                    "src/decapod/core/migration.rs",
                ])
                .unwrap()
            ),
            dir: Some(temp.path().to_string_lossy().into_owned()),
            depends_on: String::new(),
            blocks: String::new(),
            parent: None,
            one_shot: 0,
        },
    )
    .unwrap();
    let task_id = added["id"].as_str().unwrap().to_owned();
    todo::edit_task(&data, &task_id, None, Some(&desc_v2), None, None).unwrap();

    let conn = Connection::open(data.join(schemas::LOCAL_DB_NAME)).unwrap();
    // Insert a large unreferenced orphan blob (64 KiB) to create measurable free pages upon GC.
    let orphan_bytes = vec![b'Z'; 65_536];
    let orphan_sha = format!("{:x}", Sha256::digest(&orphan_bytes));
    conn.execute(
        "INSERT INTO storage_components(component_kind,codec_version,byte_len,content_sha256,stored_bytes,created_at)
         VALUES('task.description',0,?1,?2,?3,'2026-11-12T00:00:00Z')",
        decapod::params![orphan_bytes.len() as i64, orphan_sha, orphan_bytes],
    )
    .unwrap();

    let dry = storage_compaction::gc_unreferenced_components(&conn, true).unwrap();
    assert!(dry.dry_run);
    assert_eq!(dry.unreferenced_components, 1);
    assert_eq!(dry.deleted_components, 0);
    assert!(dry.referenced_components >= 3);

    let live = storage_compaction::gc_unreferenced_components(&conn, false).unwrap();
    assert!(!live.dry_run);
    assert_eq!(live.deleted_components, 1);

    // Journal verification must remain 100% intact across both historical and live checkpoints!
    let verified = todo::task_journal_verify(&data).unwrap();
    assert_eq!(verified["status"], "ok");

    // GC must NOT run an implicit VACUUM: freed pages remain on SQLite's freelist until explicit call.
    let freelist_before: u64 = conn
        .query_row(
            "SELECT freelist_count FROM pragma_freelist_count",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(freelist_before > 0);

    // Explicit VACUUM inside an open transaction must fail closed.
    conn.execute_batch("BEGIN IMMEDIATE").unwrap();
    let tx_vac_err = storage_compaction::reclaim_pages_explicit(&conn).unwrap_err();
    assert!(
        tx_vac_err
            .to_string()
            .contains("EXPLICIT_VACUUM_REQUIRES_AUTOCOMMIT")
    );
    conn.execute_batch("ROLLBACK").unwrap();

    let vac = storage_compaction::reclaim_pages_explicit(&conn).unwrap();
    assert_eq!(vac.freelist_after, 0);
    assert!(vac.pages_after < vac.pages_before);
}
