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
        let mut stmt = conn
            .prepare(&format!("SELECT * FROM \"{name}\" ORDER BY rowid"))
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
