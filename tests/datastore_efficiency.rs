//! Synthetic, disposable datastore evidence. Never opens a user's datastore.
use decapod::core::{db::Connection, events, migration, schemas};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, time::Instant};

const COMPACTION: &str = "events.compact_sequence_index.v002";
const OPERATION_SAMPLES: usize = 512;

fn compact(root: &Path) -> Result<(), decapod::core::error::DecapodError> {
    (migration::all_migrations()
        .into_iter()
        .find(|m| m.id == COMPACTION)
        .unwrap()
        .up)(root)
}

fn fixture(root: &Path) -> Connection {
    fs::create_dir_all(root.join("data")).unwrap();
    let conn = Connection::open(root.join("data/decapod.db")).unwrap();
    // Exact historical DDL: keep this fixture independent of current init.
    conn.execute_batch(schemas::EVENTS_TABLE_SCHEMA).unwrap();
    conn.execute_batch(schemas::EVENTS_TABLE_UNIQUE_STREAM_SEQUENCE_INDEX)
        .unwrap();
    conn
}

fn snapshot(conn: &Connection) -> String {
    let mut digest = Sha256::new();
    let mut stmt = conn.prepare("SELECT event_id, ts, seq, stream, subject_kind, subject_id, event_type, payload, actor FROM events ORDER BY event_id").unwrap();
    for row in stmt
        .query_map([], |r| {
            Ok(json!([
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, String>(8)?
            ]))
        })
        .unwrap()
    {
        digest.update(serde_json::to_vec(&row.unwrap()).unwrap());
        digest.update(b"\n");
    }
    format!("{:x}", digest.finalize())
}

fn operational_snapshot(conn: &Connection) -> Value {
    let mut hashes = serde_json::Map::new();
    for table in conn.inspect_schema().unwrap().tables {
        if table.name == "events" {
            continue;
        }
        let name = table.name.replace('"', "\"\"");
        let mut digest = Sha256::new();
        let order = match table.name.as_str() {
            "meta" => "namespace,key",
            "task_tags" => "task_id,tag",
            _ => "rowid",
        };
        let mut stmt = conn
            .prepare(&format!("SELECT * FROM \"{name}\" ORDER BY {order}"))
            .unwrap();
        let mut rows = stmt.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            digest.update(serde_json::to_vec(&row.parameters()).unwrap());
            digest.update(b"\n");
        }
        hashes.insert(table.name, json!(format!("{:x}", digest.finalize())));
    }
    Value::Object(hashes)
}

fn has_index(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='index' AND name=?1)",
        [name],
        |r| r.get(0),
    )
    .unwrap()
}

#[test]
fn index_upgrade_preserves_nonempty_evidence_and_reinitialization() {
    let root = tempfile::tempdir().unwrap();
    let conn = fixture(root.path());
    for (stream, id, payload) in [
        (
            "todo",
            "task-proof",
            json!({"task_id":"feat_0000000000000001", "task_revision":7,"proof_hash":"sha256:abc"}),
        ),
        (
            "workunit",
            "workunit-proof",
            json!({"generation":4,"predecessor":"sha256:abc"}),
        ),
        (
            "assurance",
            "assurance-proof",
            json!({"state_commit_root":"sha256:def","encoding_version":1}),
        ),
    ] {
        events::append_on_conn(
            &conn,
            stream,
            &json!({"event_id":id,"payload":payload,"actor":"agent-a"}),
        )
        .unwrap();
    }
    let before = snapshot(&conn);
    compact(root.path()).unwrap();
    assert!(!has_index(&conn, "idx_events_stream_seq"));
    for _ in 0..3 {
        events::ensure_tables(&conn).unwrap();
        compact(root.path()).unwrap();
    }
    assert!(!has_index(&conn, "idx_events_stream_seq"));
    assert!(has_index(&conn, "idx_events_stream_seq_unique"));
    assert_eq!(before, snapshot(&conn));
    assert!(conn.execute("INSERT INTO events(event_id,ts,seq,stream,payload) VALUES('duplicate','now',1,'todo','{}')", []).is_err());
    // A different stream retains its independent sequence space.
    conn.execute("INSERT INTO events(event_id,ts,seq,stream,payload) VALUES('independent','now',1,'broker','{}')", []).unwrap();
}

#[test]
fn compaction_refuses_missing_partial_or_wrong_unique_replacement() {
    for replacement in [
        "CREATE UNIQUE INDEX idx_events_stream_seq_unique ON events(stream,seq) WHERE stream='todo'",
        "CREATE UNIQUE INDEX idx_events_stream_seq_unique ON events(event_id,seq)",
        "CREATE UNIQUE INDEX idx_events_stream_seq_unique ON events(stream COLLATE NOCASE,seq)",
        "CREATE UNIQUE INDEX idx_events_stream_seq_unique ON events(stream,seq,(seq+1))",
    ] {
        let root = tempfile::tempdir().unwrap();
        let conn = fixture(root.path());
        conn.execute_batch("DROP INDEX idx_events_stream_seq_unique")
            .unwrap();
        if !replacement.is_empty() {
            conn.execute_batch(replacement).unwrap();
        }
        let error = compact(root.path()).unwrap_err();
        assert!(error.to_string().contains("EVENT_INDEX_COMPACTION_UNSAFE"));
        assert!(has_index(&conn, "idx_events_stream_seq"));
        assert!(conn.is_autocommit());
    }
}

#[test]
fn old_nonunique_history_gets_an_additive_unique_index_without_renumbering() {
    for duplicate in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let conn = fixture(root.path());
        conn.execute_batch("DROP INDEX idx_events_stream_seq_unique; INSERT INTO events(event_id,ts,seq,stream,payload) VALUES('historic','old',17,'todo','{}')").unwrap();
        if duplicate {
            conn.execute_batch("INSERT INTO events(event_id,ts,seq,stream,payload) VALUES('duplicate','old',17,'todo','{}')").unwrap();
        }
        let before = snapshot(&conn);
        let result = compact(root.path());
        assert_eq!(before, snapshot(&conn));
        if duplicate {
            assert!(result.is_err());
            assert!(has_index(&conn, "idx_events_stream_seq"));
            assert!(!has_index(&conn, "idx_events_stream_seq_unique"));
        } else {
            result.unwrap();
            assert!(has_index(&conn, "idx_events_stream_seq_unique"));
            assert!(!has_index(&conn, "idx_events_stream_seq"));
            assert_eq!(scalar(&conn, "SELECT seq FROM events"), 17);
        }
    }
}

#[test]
fn index_ddl_is_transactional_and_fresh_schema_is_compact() {
    let root = tempfile::tempdir().unwrap();
    let conn = fixture(root.path());
    conn.execute_batch("BEGIN IMMEDIATE; DROP INDEX idx_events_stream_seq; ROLLBACK")
        .unwrap();
    assert!(has_index(&conn, "idx_events_stream_seq"));
    let fresh = Connection::open(":memory:").unwrap();
    events::ensure_tables(&fresh).unwrap();
    assert!(!has_index(&fresh, "idx_events_stream_seq"));
    assert!(has_index(&fresh, "idx_events_stream_seq_unique"));
    assert_eq!(schemas::EVENTS_STORAGE_VERSION, 2);
}

#[test]
fn compaction_preserves_unexpected_index_named_like_the_historical_tree() {
    let root = tempfile::tempdir().unwrap();
    let conn = fixture(root.path());
    conn.execute_batch(
        "DROP INDEX idx_events_stream_seq; CREATE INDEX idx_events_stream_seq ON events(actor)",
    )
    .unwrap();
    assert!(
        compact(root.path())
            .unwrap_err()
            .to_string()
            .contains("EVENT_INDEX_COMPACTION_UNSAFE")
    );
    assert!(has_index(&conn, "idx_events_stream_seq"));
}

#[test]
fn compact_event_sequence_serializes_64_competing_writers_and_replays() {
    use std::sync::{Arc, Barrier};
    let root = tempfile::tempdir().unwrap();
    let conn = fixture(root.path());
    compact(root.path()).unwrap();
    let barrier = Arc::new(Barrier::new(64));
    std::thread::scope(|scope| {
        let mut workers = Vec::new();
        for i in 0..64 {
            let path = root.path().join("data/decapod.db");
            let barrier = Arc::clone(&barrier);
            workers.push(scope.spawn(move || {
                let conn = Connection::open(path).unwrap();
                barrier.wait();
                let event = json!({"event_id":format!("concurrent-{i}"),"event_type":"task.claim","actor":format!("agent-{i}")});
                let sequence = events::append_on_conn(&conn, "todo", &event).unwrap();
                assert_eq!(sequence, events::append_on_conn(&conn, "todo", &event).unwrap());
                sequence
            }));
        }
        let mut sequences = workers
            .into_iter()
            .map(|w| w.join().unwrap())
            .collect::<Vec<_>>();
        sequences.sort_unstable();
        assert_eq!(sequences, (1..=64).collect::<Vec<_>>());
    });
    assert_eq!(scalar(&conn, "SELECT COUNT(*) FROM events"), 64);
}

fn scalar(conn: &Connection, sql: &str) -> u64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

fn storage(conn: &Connection, path: &Path) -> Value {
    let objects = conn.prepare("SELECT name, SUM(pgsize), SUM(payload), SUM(unused) FROM dbstat GROUP BY name ORDER BY name")
        .and_then(|mut s| s.query_map([], |r| Ok(json!({"name":r.get::<_, String>(0)?,"bytes":r.get::<_, u64>(1)?,"payload_bytes":r.get::<_, u64>(2)?,"unused_bytes":r.get::<_, u64>(3)?})))?.collect::<Result<Vec<_>, _>>());
    json!({"file_bytes":fs::metadata(path).unwrap().len(), "page_size":scalar(conn,"PRAGMA page_size"),
        "page_count":scalar(conn,"PRAGMA page_count"), "freelist_pages":scalar(conn,"PRAGMA freelist_count"),
        "objects": objects.ok(), "dbstat_note":"null means host SQLite lacks DBSTAT"})
}

fn distribution(mut ns: Vec<u128>) -> Value {
    ns.sort_unstable();
    let p = |percent: usize| ns[(ns.len() * percent).div_ceil(100).saturating_sub(1)];
    json!({"samples":ns.len(),"p50_ns":p(50),"p95_ns":p(95),"p99_ns":p(99)})
}

fn workload(conn: &Connection, path: &Path) -> Value {
    let mut append_ns = Vec::new();
    let mut replay_ns = Vec::new();
    let mut read_ns = Vec::new();
    let mut wal_bytes = 0;
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    for i in 0..OPERATION_SAMPLES {
        let value = json!({"event_id":format!("measured-{i:06}"),"ts":"2026-11-09T00:00:00Z","event_type":"task.claim","task_id":"feat_fixture00000001","actor":"agent-fixture","payload":{"lease_generation":i,"revision":i}});
        let start = Instant::now();
        events::append_on_conn(conn, "todo", &value).unwrap();
        append_ns.push(start.elapsed().as_nanos());
        wal_bytes += fs::metadata(format!("{}-wal", path.display()))
            .map(|m| m.len())
            .unwrap_or(0);
        // Empty the WAL between samples to measure cumulative bytes, not its
        // high-water file size. Checkpoint time is deliberately excluded.
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
            .unwrap();
        let start = Instant::now();
        events::append_on_conn(conn, "todo", &value).unwrap();
        replay_ns.push(start.elapsed().as_nanos());
        let start = Instant::now();
        scalar(conn, "SELECT MAX(seq) FROM events WHERE stream='todo'");
        read_ns.push(start.elapsed().as_nanos());
    }
    let plans = ["EXPLAIN QUERY PLAN SELECT MAX(seq) FROM events WHERE stream='todo'",
        "EXPLAIN QUERY PLAN SELECT event_id,payload FROM events WHERE stream='todo' ORDER BY seq DESC LIMIT 64"]
        .iter().map(|sql| {
            let mut stmt=conn.prepare(sql).unwrap();
            stmt.query_map([],|r|r.get::<_,String>(3)).unwrap().collect::<Result<Vec<_>,_>>().unwrap()
        }).collect::<Vec<_>>();
    json!({"append":distribution(append_ns),"idempotent_replay":distribution(replay_ns),"sequence_read":distribution(read_ns),
        "wal_bytes_for_appends":wal_bytes,"query_plans":plans,
        "scope":"events::append_on_conn transaction; excludes broker, CLI and checkpoint cost; warm OS cache",
        "statements_per_new_append":5,"statements_per_idempotent_replay":4,
        "statement_count_method":"source: BEGIN, identity lookup, MAX, INSERT, COMMIT; replay substitutes seq lookup for MAX/INSERT"})
}

fn inventory(conn: &Connection) -> Value {
    let mut tables = Vec::new();
    let catalog = conn.inspect_schema().unwrap();
    for table in &catalog.tables {
        let name = table.name.replace('"', "\"\"");
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info(\"{name}\")"))
            .unwrap();
        let mut columns=stmt.query_map([],|r|Ok(json!({"name":r.get::<_,String>(1)?,"type":r.get::<_,String>(2)?,"not_null":r.get::<_,i64>(3)?!=0,"default_sql":r.get::<_,Option<String>>(4)?,"primary_key_position":r.get::<_,i64>(5)?}))).unwrap().collect::<Result<Vec<_>,_>>().unwrap();
        for column in &mut columns {
            let field = column["name"].as_str().unwrap().replace('"', "\"\"");
            let stats=conn.query_row(&format!("SELECT MAX(length(CAST(\"{field}\" AS BLOB))), SUM(length(CAST(\"{field}\" AS BLOB))) FROM \"{name}\""),[],|r|Ok(json!({"max_utf8_or_blob_bytes":r.get::<_,Option<u64>>(0)?,"sum_logical_value_bytes":r.get::<_,Option<u64>>(1)?}))).unwrap();
            column["synthetic_observed"] = stats;
            column["domain_limits"] =
                json!("not tightened by this increment; see table DDL for existing constraints");
            column["required_for_kernel_correctness"] = Value::Null;
            column["frequency"] = json!("production unmeasured; fixture counts only");
            column["proposed_change"] = json!("none");
        }
        let ddl: String = conn
            .query_row(
                "SELECT sql FROM sqlite_schema WHERE type='table' AND name=?1",
                [&table.name],
                |r| r.get(0),
            )
            .unwrap();
        tables.push(json!({"name":table.name,"columns":columns,"ddl":ddl,"rows":table.row_count,"foreign_keys":table.foreign_keys,"unique_constraints":table.unique_constraints}));
    }
    json!({"tables":tables,"indexes":catalog.indexes,"scope":"effective migration bootstrap plus explicitly invoked TODO, knowledge, LCM, health, aptitude, federation, cron, reflex, archive, feedback, policy, obligation and decision initializers; lazy broker request_dedupe is separately declared in entities.json"})
}

/// Run explicitly; no timing assertions in CI. Synthetic evidence is not a
/// hosted, production, complete workload or cold filesystem-cache claim.
#[test]
#[ignore = "explicit reproducible datastore measurement"]
fn datastore_before_after_evidence() {
    let mut reports = Vec::new();
    for trial in 0..3 {
        for journal in ["delete", "wal"] {
            for (rows, payload_bytes) in [(1_000, 128), (10_000, 1024), (100_000, 1024)] {
                let root = tempfile::tempdir().unwrap();
                let init = Instant::now();
                migration::check_and_migrate(root.path()).unwrap();
                let data = root.path().join("data");
                decapod::core::todo::initialize_todo_db(&data).unwrap();
                decapod::core::db::initialize_knowledge_db(&data).unwrap();
                decapod::plugins::lcm::initialize_lcm_db(&data).unwrap();
                decapod::plugins::health::initialize_health_db(&data).unwrap();
                decapod::plugins::aptitude::initialize_aptitude_db(&data).unwrap();
                decapod::plugins::federation::initialize_federation_db(&data).unwrap();
                decapod::plugins::cron::initialize_cron_db(&data).unwrap();
                decapod::plugins::reflex::initialize_reflex_db(&data).unwrap();
                decapod::plugins::archive::initialize_archive_db(&data).unwrap();
                decapod::plugins::feedback::initialize_feedback_db(&data).unwrap();
                decapod::plugins::policy::initialize_policy_db(&data).unwrap();
                decapod::core::obligation::initialize_obligation_db(&data).unwrap();
                decapod::core::db::initialize_decide_db(&data).unwrap();
                let init_ns = init.elapsed().as_nanos();
                let path = data.join("decapod.db");
                let conn = Connection::open(&path).unwrap();
                // Both versions use the same mode and default synchronous setting.
                // DELETE is the current fresh-store default; WAL is a separate fixture,
                // not a runtime durability or journal-mode change.
                conn.execute_batch(&format!("PRAGMA journal_mode={journal}"))
                    .unwrap();
                conn.execute_batch("PRAGMA synchronous=FULL").unwrap();
                conn.execute_batch(schemas::EVENTS_TABLE_SCHEMA).unwrap();
                let payload=json!({"description":"x".repeat(payload_bytes),"proof_ref":"sha256:fixture","schema_version":1}).to_string();
                conn.execute_batch("BEGIN IMMEDIATE").unwrap();
                for i in 0..rows {
                    if i % 40 == 0 {
                        conn.execute("INSERT INTO tasks(id,hash,title,description,tags,created_at,updated_at,dir_path,scope,revision,lease_generation) VALUES(?1,?2,?3,?4,'storage,fixture','2026-11-09T00:00:00Z','2026-11-09T00:00:00Z','/fixture/repository','root',40,3)",decapod::params![format!("task-{:08}",i/40),format!("{:06}",i/40),format!("Fixture task {}",i/40),"Synthetic governed task description"]).unwrap();
                    }
                    conn.execute("INSERT INTO events(event_id,ts,seq,stream,subject_kind,subject_id,event_type,payload,actor) VALUES(?1,'2026-11-09T00:00:00Z',?2,'todo','task',?3,'task.journal.checkpoint',?4,'fixture-agent')",
                decapod::params![format!("event-{i:012}"),i+1,format!("task-{:08}",i/40),payload]).unwrap();
                }
                conn.execute_batch(
                "COMMIT; PRAGMA wal_checkpoint(TRUNCATE); VACUUM; PRAGMA wal_checkpoint(TRUNCATE)",
            )
            .unwrap();
                let baseline = storage(&conn, &path);
                let digest = snapshot(&conn);
                let operational = operational_snapshot(&conn);
                // Alternate timing order across trials to expose order/cache bias.
                let optimized_first = if trial % 2 == 1 {
                    compact(root.path()).unwrap();
                    conn.execute_batch("VACUUM; PRAGMA wal_checkpoint(TRUNCATE)")
                        .unwrap();
                    let result = workload(&conn, &path);
                    conn.execute("DELETE FROM events WHERE event_id LIKE 'measured-%'", [])
                        .unwrap();
                    conn.execute_batch(schemas::EVENTS_TABLE_SCHEMA).unwrap();
                    conn.execute_batch("VACUUM; PRAGMA wal_checkpoint(TRUNCATE)")
                        .unwrap();
                    Some(result)
                } else {
                    None
                };
                // Each variant starts from the same bytes and compact page packing.
                let baseline_work = workload(&conn, &path);
                conn.execute("DELETE FROM events WHERE event_id LIKE 'measured-%'", [])
                    .unwrap();
                let start = Instant::now();
                compact(root.path()).unwrap();
                let migration_ns = start.elapsed().as_nanos();
                conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
                    .unwrap();
                assert_eq!(digest, snapshot(&conn));
                assert_eq!(operational, operational_snapshot(&conn));
                let after_drop = storage(&conn, &path);
                conn.execute_batch("VACUUM; PRAGMA wal_checkpoint(TRUNCATE)")
                    .unwrap();
                let reclaimed = storage(&conn, &path);
                let optimized_work = optimized_first.unwrap_or_else(|| workload(&conn, &path));
                let schema = if rows == 1_000 && journal == "delete" && trial == 0 {
                    Some(inventory(&conn))
                } else {
                    None
                };
                let sqlite: String = conn
                    .query_row("SELECT sqlite_version()", [], |r| r.get(0))
                    .unwrap();
                let journal: String = conn
                    .query_row("PRAGMA journal_mode", [], |r| r.get(0))
                    .unwrap();
                reports.push(json!({"trial":trial,"timing_order":if trial%2==0 {"baseline_first"} else {"optimized_first"},"rows":rows,"payload_body_bytes":payload_bytes,"sqlite_version":sqlite,"journal_mode":journal,
            "synchronous":scalar(&conn,"PRAGMA synchronous"),"init_ns":init_ns,"migration_ns":migration_ns,
            "baseline":baseline,"after_drop":after_drop,"after_fixture_vacuum":reclaimed,
                "baseline_operations":baseline_work,"optimized_operations":optimized_work,"inventory":schema,"preserved_event_digest":digest,"preserved_operational_digests":operational}));
            }
        }
    }
    let report = json!({"schema_version":1,"fixture_version":1,"baseline_ddl":"historical EVENTS_TABLE_SCHEMA plus unique index","optimization":COMPACTION,
        "unmeasured":["peak_memory","allocations","full CLI cold-start","task/knowledge workload latency","hosted PostgreSQL","network bytes","multi-agent contention"],"cases":reports});
    if let Ok(path) = std::env::var("DECAPOD_DATASTORE_REPORT") {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .unwrap();
        file.write_all(&serde_json::to_vec_pretty(&report).unwrap())
            .unwrap();
    }
    println!("{}", serde_json::to_string(&report).unwrap());
}

#[test]
fn v2_checkpoint_and_event_compaction_reduces_bytes_losslessly() {
    use decapod::core::{storage_compaction, todo};

    let v1_dir = tempfile::tempdir().unwrap();
    let v2_dir = tempfile::tempdir().unwrap();
    let v1_data = v1_dir.path().join("data");
    let v2_data = v2_dir.path().join("data");
    todo::initialize_todo_db(&v1_data).unwrap();
    todo::initialize_todo_db(&v2_data).unwrap();

    let large_desc =
        "Governed task specification and acceptance invariants for storage compaction.\n"
            .repeat(24);
    let large_scope = format!(
        "paths:{}",
        serde_json::to_string(&vec![
            "src/decapod/core/task_journal.rs",
            "src/decapod/core/storage_compaction.rs",
            "src/decapod/core/events.rs",
            "src/decapod/core/storage_contracts.rs",
            "src/decapod/core/storage_inventory.rs",
            "src/decapod/core/task_transfer.rs",
        ])
        .unwrap()
    );

    // Seed v2 store through actual v2 journal + _ce_v1 compact event path (10 tasks x 12 transitions).
    for t in 0..10 {
        let added = todo::add_task(
            &v2_data,
            &todo::TodoCommand::Add {
                title: format!("Compact benchmark task {t}"),
                description: large_desc.clone(),
                priority: "high".into(),
                tags: "storage,v2,compact".into(),
                owner: String::new(),
                due: None,
                r#ref: "issue:1630".into(),
                scope: large_scope.clone(),
                dir: Some(v2_dir.path().to_string_lossy().into_owned()),
                depends_on: String::new(),
                blocks: String::new(),
                parent: None,
                one_shot: 0,
            },
        )
        .unwrap();
        let id = added["id"].as_str().unwrap().to_owned();
        for rev in 1..12 {
            todo::edit_task(
                &v2_data,
                &id,
                Some(&format!("Compact benchmark task {t} rev {rev}")),
                None,
                None,
                None,
            )
            .unwrap();
        }
    }

    // Construct an exact logical v1 baseline twin of v2_data (identical tasks, broker rows,
    // and domain events, with verbose envelopes and inline schema_version=1 checkpoints).
    let v2_conn = Connection::open(v2_data.join(schemas::LOCAL_DB_NAME)).unwrap();
    v2_conn
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    fs::create_dir_all(&v1_data).unwrap();
    fs::copy(
        v2_data.join(schemas::LOCAL_DB_NAME),
        v1_data.join(schemas::LOCAL_DB_NAME),
    )
    .unwrap();
    let v1_conn = Connection::open(v1_data.join(schemas::LOCAL_DB_NAME)).unwrap();
    v1_conn.execute_batch("BEGIN IMMEDIATE").unwrap();
    let rows_to_expand: Vec<(String, String, String, String, String, Option<String>, Option<String>, String)> = v1_conn
        .prepare("SELECT stream,event_id,ts,event_type,actor,subject_kind,subject_id,payload FROM events")
        .unwrap()
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for (stream, eid, ts, etype, actor, sk, sid, raw) in rows_to_expand {
        let mut verbose = events::reconstruct_stored_payload(
            &stream,
            &eid,
            &ts,
            &etype,
            &actor,
            sk.as_deref(),
            sid.as_deref(),
            &raw,
        )
        .unwrap();
        if etype == "task.journal.checkpoint"
            && let Some(ckpt) = verbose.get_mut("payload").and_then(Value::as_object_mut)
        {
            ckpt.insert("schema_version".into(), json!(1));
            ckpt.remove("component_refs");
            if let Some(task_obj) = ckpt
                .get_mut("projection")
                .and_then(|p| p.get_mut("task"))
                .and_then(Value::as_object_mut)
            {
                task_obj.insert("description".into(), json!(large_desc));
                task_obj.insert("scope".into(), json!(large_scope));
            }
        }
        v1_conn
            .execute(
                "UPDATE events SET payload = ?1 WHERE stream = ?2 AND event_id = ?3",
                decapod::params![verbose.to_string(), stream, eid],
            )
            .unwrap();
    }
    v1_conn
        .execute_batch("DELETE FROM storage_component_refs; DELETE FROM storage_components; COMMIT")
        .unwrap();
    storage_compaction::reclaim_pages_explicit(&v1_conn).unwrap();
    storage_compaction::reclaim_pages_explicit(&v2_conn).unwrap();

    let v1_bytes = fs::metadata(v1_data.join(schemas::LOCAL_DB_NAME))
        .unwrap()
        .len();
    let v2_bytes = fs::metadata(v2_data.join(schemas::LOCAL_DB_NAME))
        .unwrap()
        .len();
    assert!(
        v2_bytes * 100 < v1_bytes * 70,
        "expected >=30% SQLite file reduction on identical multi-transition store (v1={v1_bytes}, v2={v2_bytes})"
    );
    assert_eq!(todo::task_journal_verify(&v2_data).unwrap()["status"], "ok");
}

/// Reproducible v2 compact storage benchmark matrix (#1630):
/// Fresh, 1K, 10K, and 100K event/checkpoint histories + 3/10/15-agent retry bursts.
#[test]
#[ignore = "explicit reproducible v2 compact storage measurement (#1630)"]
fn datastore_v2_compaction_benchmarks() {
    use decapod::core::{storage_codec, storage_compaction, todo};
    use std::sync::{Arc, Barrier};

    let mut scale_cases = Vec::new();
    let desc_template =
        "Decapod governed task acceptance specification and falsifiable proof criteria.\n"
            .repeat(14); // ~1,106 bytes
    let scope_template =
        "paths:src/decapod/core/task_journal.rs,src/decapod/core/events.rs,src/decapod/core/storage_compaction.rs,"
            .repeat(3); // ~315 bytes

    // Fresh initialized store baseline vs v2
    {
        let root = tempfile::tempdir().unwrap();
        migration::check_and_migrate(root.path()).unwrap();
        let data = root.path().join("data");
        todo::initialize_todo_db(&data).unwrap();
        decapod::plugins::aptitude::initialize_aptitude_db(&data).unwrap();
        let path = data.join(schemas::LOCAL_DB_NAME);
        let conn = Connection::open(&path).unwrap();
        storage_compaction::reclaim_pages_explicit(&conn).unwrap();
        scale_cases.push(json!({
            "case": "fresh_store",
            "events": scalar(&conn, "SELECT COUNT(*) FROM events"),
            "packed_bytes": fs::metadata(&path).unwrap().len(),
            "page_count": scalar(&conn, "PRAGMA page_count"),
        }));
    }

    for total_events in [1_000usize, 10_000, 100_000] {
        let tasks = total_events / 40;
        let v1_root = tempfile::tempdir().unwrap();
        let v2_root = tempfile::tempdir().unwrap();
        let v1_data = v1_root.path().join("data");
        let v2_data = v2_root.path().join("data");
        todo::initialize_todo_db(&v1_data).unwrap();
        todo::initialize_todo_db(&v2_data).unwrap();
        let v1_path = v1_data.join(schemas::LOCAL_DB_NAME);
        let v2_path = v2_data.join(schemas::LOCAL_DB_NAME);
        let v1_conn = Connection::open(&v1_path).unwrap();
        let v2_conn = Connection::open(&v2_path).unwrap();

        // Seed v1 inline checkpoint + verbose envelope fixture
        v1_conn.execute_batch("BEGIN IMMEDIATE").unwrap();
        for t in 0..tasks {
            let tid = format!("bench-task-{t:06}");
            let desc = format!("{desc_template} [task={t}]");
            v1_conn
                .execute(
                    "INSERT INTO tasks(id,hash,title,description,tags,created_at,updated_at,dir_path,scope,revision,lease_generation)
                     VALUES(?1,?2,?3,?4,'bench,storage','2026-11-12T00:00:00Z','2026-11-12T00:00:00Z','/bench/repo',?5,40,4)",
                    decapod::params![tid, format!("h{t:06}"), format!("Bench task {t}"), desc, scope_template],
                )
                .unwrap();
            for ord in 1..=40u64 {
                let seq = (t as u64) * 40 + ord;
                let eid = format!("ev-{seq:09}");
                let raw = json!({
                    "event_id": eid,
                    "ts": "2026-11-12T00:00:00Z",
                    "event_type": "task.journal.checkpoint",
                    "actor": "agent-bench",
                    "subject_kind": "task",
                    "subject_id": tid,
                    "payload": {
                        "schema_version": 1,
                        "task_id": tid,
                        "ordinal": ord,
                        "kind": if ord == 1 { "observed_baseline" } else { "transition" },
                        "source_watermark": seq - 1,
                        "operation": "task.edit",
                        "actor": "agent-bench",
                        "projection": {
                            "task": {
                                "id": tid,
                                "hash": format!("h{t:06}"),
                                "title": format!("Bench task {t} ord {ord}"),
                                "description": desc,
                                "scope": scope_template,
                                "revision": ord
                            }
                        }
                    }
                })
                .to_string();
                v1_conn
                    .execute(
                        "INSERT INTO events(event_id,ts,seq,stream,subject_kind,subject_id,event_type,payload,actor)
                         VALUES(?1,'2026-11-12T00:00:00Z',?2,'todo','task',?3,'task.journal.checkpoint',?4,'agent-bench')",
                        decapod::params![eid, seq, tid, raw],
                    )
                    .unwrap();
            }
        }
        v1_conn.execute_batch("COMMIT").unwrap();
        storage_compaction::reclaim_pages_explicit(&v1_conn).unwrap();

        // Seed v2 deduplicated component + _ce_v1 compact envelope fixture representing identical logical data
        v2_conn.execute_batch("BEGIN IMMEDIATE").unwrap();
        let scope_sha = format!("{:x}", Sha256::digest(scope_template.as_bytes()));
        let scope_blob = match storage_codec::encode(&scope_template).unwrap() {
            storage_codec::StoredText::Zlib(b) => b,
            storage_codec::StoredText::Plain(s) => s.into_bytes(),
        };
        v2_conn
            .execute(
                "INSERT OR IGNORE INTO storage_components(component_kind,codec_version,byte_len,content_sha256,stored_bytes,created_at)
                 VALUES('task.scope',1,?1,?2,?3,'2026-11-12T00:00:00Z')",
                decapod::params![scope_template.len() as i64, scope_sha, scope_blob],
            )
            .unwrap();

        for t in 0..tasks {
            let tid = format!("bench-task-{t:06}");
            let desc = format!("{desc_template} [task={t}]");
            let desc_sha = format!("{:x}", Sha256::digest(desc.as_bytes()));
            let desc_blob = match storage_codec::encode(&desc).unwrap() {
                storage_codec::StoredText::Zlib(b) => b,
                storage_codec::StoredText::Plain(s) => s.into_bytes(),
            };
            v2_conn
                .execute(
                    "INSERT INTO tasks(id,hash,title,description,tags,created_at,updated_at,dir_path,scope,revision,lease_generation)
                     VALUES(?1,?2,?3,?4,'bench,storage','2026-11-12T00:00:00Z','2026-11-12T00:00:00Z','/bench/repo',?5,40,4)",
                    decapod::params![tid, format!("h{t:06}"), format!("Bench task {t}"), desc, scope_template],
                )
                .unwrap();
            v2_conn
                .execute(
                    "INSERT OR IGNORE INTO storage_components(component_kind,codec_version,byte_len,content_sha256,stored_bytes,created_at)
                     VALUES('task.description',1,?1,?2,?3,'2026-11-12T00:00:00Z')",
                    decapod::params![desc.len() as i64, desc_sha, desc_blob],
                )
                .unwrap();
            for ord in 1..=40u64 {
                let seq = (t as u64) * 40 + ord;
                let eid = format!("ev-{seq:09}");
                let compact_raw = json!({
                    "_ce_v1": 63,
                    "payload": {
                        "schema_version": 2,
                        "task_id": tid,
                        "ordinal": ord,
                        "kind": if ord == 1 { "observed_baseline" } else { "transition" },
                        "source_watermark": seq - 1,
                        "operation": "task.edit",
                        "actor": "agent-bench",
                        "component_refs": {
                            "task.description": {"kind": "task.description", "codec_version": 1, "byte_len": desc.len(), "content_sha256": desc_sha},
                            "task.scope": {"kind": "task.scope", "codec_version": 1, "byte_len": scope_template.len(), "content_sha256": scope_sha}
                        },
                        "projection": {
                            "task": {
                                "id": tid,
                                "hash": format!("h{t:06}"),
                                "title": format!("Bench task {t} ord {ord}"),
                                "description": null,
                                "scope": "",
                                "revision": ord
                            }
                        }
                    }
                })
                .to_string();
                v2_conn
                    .execute(
                        "INSERT INTO events(event_id,ts,seq,stream,subject_kind,subject_id,event_type,payload,actor)
                         VALUES(?1,'2026-11-12T00:00:00Z',?2,'todo','task',?3,'task.journal.checkpoint',?4,'agent-bench')",
                        decapod::params![eid, seq, tid, compact_raw],
                    )
                    .unwrap();
                v2_conn
                    .execute(
                        "INSERT INTO storage_component_refs(ref_scope,ref_id,component_kind,codec_version,byte_len,content_sha256)
                         VALUES('task_checkpoint',?1,'task.description',1,?2,?3),
                               ('task_checkpoint',?1,'task.scope',1,?4,?5)",
                        decapod::params![eid, desc.len() as i64, desc_sha, scope_template.len() as i64, scope_sha],
                    )
                    .unwrap();
            }
        }
        v2_conn.execute_batch("COMMIT").unwrap();
        storage_compaction::reclaim_pages_explicit(&v2_conn).unwrap();

        let v1_file_bytes = fs::metadata(&v1_path).unwrap().len();
        let v2_file_bytes = fs::metadata(&v2_path).unwrap().len();
        let v1_payload_bytes = scalar(
            &v1_conn,
            "SELECT SUM(length(CAST(payload AS BLOB))) FROM events",
        );
        let v2_payload_bytes = scalar(
            &v2_conn,
            "SELECT SUM(length(CAST(payload AS BLOB))) FROM events",
        );
        let v2_comp_bytes = scalar(
            &v2_conn,
            "SELECT COALESCE(SUM(length(stored_bytes)),0) FROM storage_components",
        );

        let mut append_ns = Vec::new();
        for s in 0..128 {
            let ev = json!({
                "event_id": format!("AppendSample-{total_events}-{s}"),
                "ts": "2026-11-12T01:00:00Z",
                "event_type": "task.comment",
                "subject_kind": "task",
                "subject_id": "bench-task-000000",
                "actor": "agent-bench",
                "payload": {"comment": "Measured compact envelope append"}
            });
            let t0 = Instant::now();
            events::append_on_conn(&v2_conn, "todo", &ev).unwrap();
            append_ns.push(t0.elapsed().as_nanos());
        }

        let reduction_pct =
            ((v1_file_bytes as f64 - v2_file_bytes as f64) / (v1_file_bytes as f64)) * 100.0;
        scale_cases.push(json!({
            "events": total_events,
            "tasks": tasks,
            "v1_packed_file_bytes": v1_file_bytes,
            "v2_packed_file_bytes": v2_file_bytes,
            "file_bytes_saved": v1_file_bytes.saturating_sub(v2_file_bytes),
            "file_reduction_pct": (reduction_pct * 100.0).round() / 100.0,
            "v1_events_payload_bytes": v1_payload_bytes,
            "v2_events_payload_bytes": v2_payload_bytes,
            "v2_storage_components_bytes": v2_comp_bytes,
            "v2_append_latency": distribution(append_ns),
        }));
    }

    // Multi-agent retry bursts (3, 10, 15 concurrent agents appending + replaying idempotently)
    let mut contention_cases = Vec::new();
    for agents in [3usize, 10, 15] {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        todo::initialize_todo_db(&data).unwrap();
        let db_path = data.join(schemas::LOCAL_DB_NAME);
        let barrier = Arc::new(Barrier::new(agents));
        let mut agent_latencies = Vec::new();
        std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for a in 0..agents {
                let p = db_path.clone();
                let b = Arc::clone(&barrier);
                handles.push(scope.spawn(move || {
                    let conn = Connection::open(&p).unwrap();
                    conn.execute_batch("PRAGMA busy_timeout = 10000").unwrap();
                    b.wait();
                    let mut samples = Vec::new();
                    for op in 0..20 {
                        let ev = json!({
                            "event_id": format!("burst-{agents}-{a}-{op}"),
                            "event_type": "task.claim",
                            "subject_kind": "task",
                            "subject_id": format!("task-{a}"),
                            "actor": format!("agent-{a}"),
                            "payload": {"attempt": op}
                        });
                        let t0 = Instant::now();
                        let seq1 = events::append_on_conn(&conn, "todo", &ev).unwrap();
                        let seq2 = events::append_on_conn(&conn, "todo", &ev).unwrap();
                        assert_eq!(seq1, seq2);
                        samples.push(t0.elapsed().as_nanos());
                    }
                    samples
                }));
            }
            for h in handles {
                agent_latencies.extend(h.join().unwrap());
            }
        });
        let conn = Connection::open(&db_path).unwrap();
        assert_eq!(
            scalar(
                &conn,
                "SELECT COUNT(*) FROM events WHERE event_id LIKE 'burst-%'"
            ),
            (agents * 20) as u64
        );
        contention_cases.push(json!({
            "concurrent_agents": agents,
            "ops_per_agent": 20,
            "total_committed_events": agents * 20,
            "append_plus_idempotent_replay_latency": distribution(agent_latencies),
            "db_file_bytes": fs::metadata(&db_path).unwrap().len(),
        }));
    }

    let summary = json!({
        "schema": "decapod.datastore-v2-compaction-benchmark.v1",
        "release": env!("CARGO_PKG_VERSION"),
        "scale_cases": scale_cases,
        "multi_agent_burst_cases": contention_cases,
    });
    println!("{}", serde_json::to_string_pretty(&summary).unwrap());
}
