//! Phase 1 hardening qualification suite (#1937):
//! Datastore and evidence encoding compatibility corpus, version-floor refusal,
//! additive vs fail-closed evolution, duplicate/interrupted migration recovery,
//! and local/hosted logical schema parity without engine byte equality.

use decapod::core::db::{Connection, OpenFlags, params};
use decapod::core::error::DecapodError;
use decapod::core::{
    cloud_parity_matrix, events, migration, schemas, storage_codec, storage_compaction,
    storage_contracts, todo,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use tempfile::tempdir;

const CORPUS_MANIFEST_JSON: &str = include_str!("fixtures/migration/corpus_manifest.json");
const V1_PRE_CODEC_SQL: &str = include_str!("fixtures/migration/v1_pre_codec_store.sql");
const V2_STALE_FLOOR_SQL: &str = include_str!("fixtures/migration/v2_stale_floor_0_141_0.sql");
const V2_CURRENT_SQL: &str = include_str!("fixtures/migration/v2_current_accepted_store.sql");
const V2_TRANSFER_BUNDLE_JSON: &str =
    include_str!("fixtures/migration/v2_task_transfer_bundle.json");
const ADDITIVE_COMPAT_JSON: &str = include_str!("fixtures/migration/additive_compatible.json");
const VERSION_POLICY_JSON: &str = include_str!("fixtures/migration/version_policy_vectors.json");
const MALFORMED_VECTORS_JSON: &str = include_str!("fixtures/migration/malformed_vectors.json");
const LEGACY_TASKS_SQL: &str = include_str!("fixtures/migration/legacy_tasks.sql");
const EXPECTED_TODO_EVENTS_JSONL: &str =
    include_str!("fixtures/migration/expected_todo_events.jsonl");
const ENCODING_COMPAT_DOC: &str = include_str!("../docs/encoding-compatibility.md");

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn fixture_bytes(name: &str) -> &'static [u8] {
    match name {
        "corpus_manifest.json" => CORPUS_MANIFEST_JSON.as_bytes(),
        "v1_pre_codec_store.sql" => V1_PRE_CODEC_SQL.as_bytes(),
        "v2_stale_floor_0_141_0.sql" => V2_STALE_FLOOR_SQL.as_bytes(),
        "v2_current_accepted_store.sql" => V2_CURRENT_SQL.as_bytes(),
        "v2_task_transfer_bundle.json" => V2_TRANSFER_BUNDLE_JSON.as_bytes(),
        "additive_compatible.json" => ADDITIVE_COMPAT_JSON.as_bytes(),
        "version_policy_vectors.json" => VERSION_POLICY_JSON.as_bytes(),
        "malformed_vectors.json" => MALFORMED_VECTORS_JSON.as_bytes(),
        "legacy_tasks.sql" => LEGACY_TASKS_SQL.as_bytes(),
        "expected_todo_events.jsonl" => EXPECTED_TODO_EVENTS_JSONL.as_bytes(),
        other => panic!("unregistered fixture in test harness: {other}"),
    }
}

fn load_sql_fixture_into_data_dir(data_dir: &Path, sql: &str) -> std::path::PathBuf {
    fs::create_dir_all(data_dir).unwrap();
    let db_path = data_dir.join(schemas::LOCAL_DB_NAME);
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(sql).unwrap();
    db_path
}

fn snapshot_dir_digests(dir: &Path) -> Vec<(String, String)> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".lock") {
            continue;
        }
        let path = entry.path();
        if path.is_file() {
            entries.push((name, sha256_hex(&fs::read(&path).unwrap())));
        }
    }
    entries.sort();
    entries
}

#[test]
fn corpus_manifest_sha256_digests_and_policy_metadata_match_checked_in_fixtures() {
    let manifest: Value =
        serde_json::from_str(CORPUS_MANIFEST_JSON).expect("parse corpus manifest");
    assert_eq!(
        manifest["schema_version"],
        "decapod.migration-compatibility-corpus.v1"
    );
    assert_eq!(manifest["issue"], "DecapodLabs/decapod#1937");
    let related = manifest["related_issues"].as_array().unwrap();
    assert!(related.iter().any(|v| v == "DecapodLabs/dactyl#146"));
    assert!(related.iter().any(|v| v == "DecapodLabs/propodus#227"));

    assert_eq!(
        manifest["policy"]["storage_format_version"],
        json!(schemas::STORAGE_FORMAT_VERSION)
    );
    assert_eq!(
        manifest["policy"]["min_reader_version"],
        json!(schemas::STORAGE_MIN_READER_VERSION)
    );
    assert_eq!(
        manifest["policy"]["min_writer_version"],
        json!(schemas::STORAGE_MIN_WRITER_VERSION)
    );

    let fixtures = manifest["fixtures"].as_array().unwrap();
    assert!(fixtures.len() >= 9);
    for entry in fixtures {
        let name = entry["file"].as_str().unwrap();
        let bytes = fixture_bytes(name);
        assert_eq!(
            entry["byte_len"].as_u64().unwrap(),
            bytes.len() as u64,
            "byte_len mismatch for {name}"
        );
        assert_eq!(
            entry["sha256"].as_str().unwrap(),
            sha256_hex(bytes),
            "sha256 mismatch for {name}"
        );
    }
}

#[test]
fn historical_v1_fixture_upgrades_to_v2_preserving_canonical_digests_and_proof_semantics() {
    let manifest: Value = serde_json::from_str(CORPUS_MANIFEST_JSON).unwrap();
    let expected_task_id = manifest["expected_logical_digests"]["task_id"]
        .as_str()
        .unwrap();
    let expected_proj_digest = manifest["expected_logical_digests"]["projection_v1_digest"]
        .as_str()
        .unwrap();
    assert_eq!(
        expected_proj_digest,
        manifest["expected_logical_digests"]["projection_v2_digest"]
            .as_str()
            .unwrap()
    );

    let temp = tempdir().unwrap();
    let decapod_root = temp.path().join(".decapod");
    let data = decapod_root.join("data");
    let db_path = load_sql_fixture_into_data_dir(&data, V1_PRE_CODEC_SQL);

    // Pre-upgrade v1 store must already verify cleanly and match the manifest's canonical projection digest.
    let pre_verify = todo::task_journal_verify(&data).unwrap();
    assert_eq!(pre_verify["status"], "ok");
    assert_eq!(pre_verify["journal_valid"], true);
    let pre_todo = events::query(&data, events::TODO, usize::MAX).unwrap();
    let pre_broker = events::query(&data, events::BROKER, usize::MAX).unwrap();
    let pre_fed = events::query(&data, events::FEDERATION, usize::MAX).unwrap();
    let plain_summary: String = {
        let conn = Connection::open(&db_path).unwrap();
        conn.query_row(
            "SELECT summary_text FROM summaries WHERE summary_hash = 'sum_hist_1'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };

    // Verify pre-upgrade checkpoint head digest matches manifest.
    let pre_head = pre_todo
        .iter()
        .find(|e| e.seq == 4 && e.event_type == "task.journal.checkpoint")
        .unwrap();
    assert_eq!(
        pre_head.payload["payload"]["projection_digest"],
        expected_proj_digest
    );
    assert_eq!(
        pre_head.payload["payload"]["record_digest"],
        manifest["expected_logical_digests"]["v1_checkpoint_head_digest"]
    );

    // Seed an applied ledger right before seq=1000 (`events.compact_sequence_index.v002` + `v002` compaction)
    // to exercise the full v1 -> v2 upgrade path via check_and_migrate_with_backup_report.
    fs::create_dir_all(decapod_root.join("managed/migrations")).unwrap();
    let prior_ids: Vec<_> = migration::all_migrations()
        .into_iter()
        .filter(|m| m.sequence < 1000)
        .map(|m| {
            json!({
                "id": m.id,
                "sequence": m.sequence,
                "scope": m.scope,
                "kind": m.kind,
                "script_path": m.script_path,
                "min_version": m.min_version,
                "target_version": m.target_version,
                "applied_at": "1700000000Z",
                "applied_by_version": "0.130.0"
            })
        })
        .collect();
    fs::write(
        decapod_root.join("managed/migrations/applied.json"),
        serde_json::to_vec_pretty(&json!({ "schema_version": "1.0.0", "entries": prior_ids }))
            .unwrap(),
    )
    .unwrap();

    let report =
        migration::check_and_migrate_with_backup_report(&decapod_root, |_| Ok(())).unwrap();
    assert!(
        report
            .applied_migrations
            .iter()
            .any(|id| id == storage_compaction::V2_COMPACTION_MIGRATION_ID),
        "expected v2 compaction migration in {:?}",
        report.applied_migrations
    );

    // Logical events across all streams reconstruct identically.
    assert_eq!(
        events::query(&data, events::TODO, usize::MAX).unwrap(),
        pre_todo
    );
    assert_eq!(
        events::query(&data, events::BROKER, usize::MAX).unwrap(),
        pre_broker
    );
    assert_eq!(
        events::query(&data, events::FEDERATION, usize::MAX).unwrap(),
        pre_fed
    );

    // Physical SQLite rows are compacted to `_ce_v1`, while `federation` remains bare JSON.
    let conn = Connection::open(&db_path).unwrap();
    let broker_raw: String = conn
        .query_row(
            "SELECT payload FROM events WHERE event_id = 'EV_V1_BROKER_1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(broker_raw.contains("\"_ce_v1\""), "{broker_raw}");
    let fed_raw: String = conn
        .query_row(
            "SELECT payload FROM events WHERE event_id = 'EV_V1_FED_1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!fed_raw.contains("\"_ce_v1\""), "{fed_raw}");

    // Duplicate `agent_prompts` collapsed with earliest created_at and summed usage_count (2 + 3 = 5).
    let (prompt_count, usage_sum, created_at): (i64, i64, String) = conn
        .query_row(
            "SELECT COUNT(*), SUM(usage_count), MIN(created_at) FROM agent_prompts WHERE context = 'compat'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(prompt_count, 1);
    assert_eq!(usage_sum, 5);
    assert_eq!(created_at, "2026-01-01T00:00:00Z");

    // `storage_migration_manifests` records matching pre/post logical event digests.
    let (pre_d, post_d): (String, String) = conn
        .query_row(
            "SELECT pre_digest, post_digest FROM storage_migration_manifests WHERE migration_id = ?1",
            [storage_compaction::V2_COMPACTION_MIGRATION_ID],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(pre_d, post_d);

    // Historical task proof & verification_artifacts bytes remain immutable and valid.
    let post_verify = todo::task_journal_verify(&data).unwrap();
    assert_eq!(post_verify["status"], "ok");
    assert_eq!(post_verify["current_digest"], pre_verify["current_digest"]);
    let artifacts_after: String = conn
        .query_row(
            "SELECT verification_artifacts FROM task_verification WHERE todo_id = ?1",
            [expected_task_id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(artifacts_after.contains("sha256:historical1937"));

    // Cold LCM summary was compressed losslessly into a `DCPLCM1\0` BLOB during migration.
    let stored_type: String = conn
        .query_row(
            "SELECT typeof(summary_text) FROM summaries WHERE summary_hash = 'sum_hist_1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stored_type, "blob");
    let stored_param = conn
        .query_row(
            "SELECT summary_text FROM summaries WHERE summary_hash = 'sum_hist_1'",
            [],
            |r| Ok(r.parameters().into_iter().next().unwrap()),
        )
        .unwrap();
    assert_eq!(storage_codec::decode(&stored_param).unwrap(), plain_summary);
}

#[test]
fn historical_v2_stale_0_141_0_floor_restamps_safely_and_preserves_higher_floors() {
    let manifest: Value = serde_json::from_str(CORPUS_MANIFEST_JSON).unwrap();
    let expected_proj_digest = manifest["expected_logical_digests"]["projection_v2_digest"]
        .as_str()
        .unwrap();
    let temp = tempdir().unwrap();
    let decapod_root = temp.path().join(".decapod");
    let data = decapod_root.join("data");
    let db_path = load_sql_fixture_into_data_dir(&data, V2_STALE_FLOOR_SQL);

    // Read-only connection passes compatibility and never writes.
    let ro_conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    storage_compaction::check_read_compatibility(&ro_conn).unwrap();
    assert!(!storage_compaction::reconcile_v2_storage_format_metadata(&ro_conn).unwrap());
    drop(ro_conn);

    let events_before = events::query(&data, events::TODO, usize::MAX).unwrap();
    let verify_before = todo::task_journal_verify(&data).unwrap();
    assert_eq!(verify_before["status"], "ok");

    {
        let conn = Connection::open(&db_path).unwrap();
        assert!(storage_compaction::v2_format_metadata_needs_floor_upgrade(&conn).unwrap());
    }

    migration::check_and_migrate_with_backup_report(&decapod_root, |_| Ok(())).unwrap();

    let conn = Connection::open(&db_path).unwrap();
    assert!(!storage_compaction::v2_format_metadata_needs_floor_upgrade(&conn).unwrap());
    let reader: String = conn
        .query_row(
            "SELECT value FROM meta WHERE namespace='storage' AND key='min_reader_version'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let writer: String = conn
        .query_row(
            "SELECT value FROM meta WHERE namespace='storage' AND key='min_writer_version'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(reader, schemas::STORAGE_MIN_READER_VERSION);
    assert_eq!(writer, schemas::STORAGE_MIN_WRITER_VERSION);

    assert_eq!(
        events::query(&data, events::TODO, usize::MAX).unwrap(),
        events_before
    );
    let head = events_before
        .iter()
        .find(|e| e.seq == 4 && e.event_type == "task.journal.checkpoint")
        .unwrap();
    assert_eq!(
        head.payload["payload"]["projection_digest"],
        expected_proj_digest
    );
    assert_eq!(todo::task_journal_verify(&data).unwrap(), verify_before);

    // Higher valid floor is never downgraded.
    let policy: Value = serde_json::from_str(VERSION_POLICY_JSON).unwrap();
    let higher = policy["higher_valid_floor"].as_str().unwrap();
    conn.execute(
        "UPDATE meta SET value = ?1 WHERE namespace='storage' AND key IN ('min_reader_version','min_writer_version')",
        [higher],
    )
    .unwrap();
    assert!(!storage_compaction::reconcile_v2_storage_format_metadata(&conn).unwrap());
    storage_compaction::stamp_storage_format_metadata(&conn).unwrap();
    let preserved: String = conn
        .query_row(
            "SELECT value FROM meta WHERE namespace='storage' AND key='min_writer_version'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(preserved, higher);
}

#[test]
fn current_v2_fixture_and_task_transfer_bundle_preserve_codec_and_collision_escape() {
    let manifest: Value = serde_json::from_str(CORPUS_MANIFEST_JSON).unwrap();
    let expected_proj_digest = manifest["expected_logical_digests"]["projection_v2_digest"]
        .as_str()
        .unwrap();
    let temp = tempdir().unwrap();
    let decapod_root = temp.path().join(".decapod");
    let data = decapod_root.join("data");
    let db_path = load_sql_fixture_into_data_dir(&data, V2_CURRENT_SQL);

    // Verify both codec_version=0 and codec_version=1 components exist and hydrate to the exact v2 head digest.
    let conn = Connection::open(&db_path).unwrap();
    let codecs: Vec<u64> = conn
        .prepare("SELECT DISTINCT codec_version FROM storage_components ORDER BY codec_version")
        .unwrap()
        .query_map([], |r| r.get::<_, u64>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(codecs, vec![0u64, 1u64]);

    let verified = todo::task_journal_verify(&data).unwrap();
    assert_eq!(verified["status"], "ok");
    assert_eq!(verified["journal_valid"], true);

    let todo_events = events::query(&data, events::TODO, usize::MAX).unwrap();
    let head = todo_events
        .iter()
        .find(|e| e.seq == 4 && e.event_type == "task.journal.checkpoint")
        .unwrap();
    assert_eq!(
        head.payload["payload"]["projection_digest"],
        expected_proj_digest
    );
    assert_eq!(
        head.payload["payload"]["record_digest"],
        manifest["expected_logical_digests"]["v2_checkpoint_head_digest"]
    );

    // `_ce_raw` collision-escaped event reconstructs the literal caller `_ce_v1` key intact.
    let raw_escape = todo_events
        .iter()
        .find(|e| e.event_id == "EV_V2_RAW_ESCAPE")
        .unwrap();
    assert_eq!(raw_escape.payload["_ce_v1"], "caller-owned-key");
    assert_eq!(raw_escape.payload["payload"]["comment"], "escape collision");

    // `v2_task_transfer_bundle.json` parses cleanly and carries valid codec 0/1 components.
    let bundle: Value = serde_json::from_str(V2_TRANSFER_BUNDLE_JSON).unwrap();
    assert_eq!(bundle["schema"], "decapod.task-transfer.v1");
    assert_eq!(bundle["components"].as_array().unwrap().len(), 3);
}

#[test]
fn additive_compatible_fields_are_accepted_losslessly_across_events_artifacts_and_ledgers() {
    let vectors: Value = serde_json::from_str(ADDITIVE_COMPAT_JSON).unwrap();
    let temp = tempdir().unwrap();
    let decapod_root = temp.path().join(".decapod");
    let data = decapod_root.join("data");
    todo::initialize_todo_db(&data).unwrap();

    let added = todo::add_task(
        &data,
        &todo::TodoCommand::Add {
            title: "Additive compatibility test".into(),
            description: "Task verifying additive fields".into(),
            priority: "medium".into(),
            tags: "additive".into(),
            owner: String::new(),
            due: None,
            r#ref: String::new(),
            scope: "root".into(),
            dir: Some(temp.path().to_string_lossy().into_owned()),
            depends_on: String::new(),
            blocks: String::new(),
            parent: None,
            one_shot: 0,
        },
    )
    .unwrap();
    let task_id = added["id"].as_str().unwrap().to_owned();

    let mut verbose_ev = vectors["verbose_event_with_additive_fields"].clone();
    verbose_ev["task_id"] = json!(task_id);
    let fed_ev = vectors["federation_with_additive_fields"].clone();

    let conn = Connection::open(data.join(schemas::LOCAL_DB_NAME)).unwrap();
    events::append_on_conn(&conn, events::TODO, &verbose_ev).unwrap();
    events::append_on_conn(&conn, events::FEDERATION, &fed_ev).unwrap();

    // Compacting after additive writes must preserve additive fields & logical digest identically.
    let m1 = storage_compaction::apply_v2_compaction_migration(&conn).unwrap();
    let m2 = storage_compaction::apply_v2_compaction_migration(&conn).unwrap();
    assert_eq!(m1.events_logical_sha256, m2.events_logical_sha256);

    let queried_todo = events::query(&data, events::TODO, usize::MAX).unwrap();
    let add_ev = queried_todo
        .iter()
        .find(|e| e.event_id == "EV_ADDITIVE_1")
        .unwrap();
    assert_eq!(add_ev.payload["trace_id"], "tr_01m85k_additive");
    assert_eq!(add_ev.payload["client_sdk"]["version"], "0.164.1");
    assert_eq!(
        add_ev.payload["payload"]["extension_meta"]["tier"],
        "post-cutover"
    );

    let queried_fed = events::query(&data, events::FEDERATION, usize::MAX).unwrap();
    let fed_row = queried_fed
        .iter()
        .find(|e| e.payload["node_id"] == "node_additive_1")
        .unwrap();
    assert_eq!(fed_row.payload["dactyl_envelope_version"], "0.19.0");
    assert_eq!(
        fed_row.payload["optional_hints"],
        json!(["preserve-unknown-keys"])
    );

    // Verification artifacts JSON with additive fields round-trips byte-for-byte.
    let artifacts_str = vectors["verification_artifacts_with_additive_fields"].to_string();
    conn.execute(
        "INSERT INTO task_verification(todo_id,proof_plan,verification_artifacts,last_verified_status,verification_policy_days,updated_at)
         VALUES(?1,'[\"validate_passes\"]',?2,'passed',90,'2026-11-23T00:00:00Z')",
        params![task_id, artifacts_str],
    )
    .unwrap();
    let stored_art: String = conn
        .query_row(
            "SELECT verification_artifacts FROM task_verification WHERE todo_id = ?1",
            [&task_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stored_art, artifacts_str);

    // Migration JSON ledgers tolerate additive optional fields cleanly.
    fs::create_dir_all(decapod_root.join("managed/migrations")).unwrap();
    fs::write(
        decapod_root.join("managed/migrations/applied.json"),
        serde_json::to_vec_pretty(&vectors["applied_migration_ledger_with_additive_fields"])
            .unwrap(),
    )
    .unwrap();
    fs::write(
        decapod_root.join("managed/version_counter.json"),
        serde_json::to_vec_pretty(&vectors["version_counter_with_additive_fields"]).unwrap(),
    )
    .unwrap();
    let inspected =
        migration::inspect_migration_state_at(temp.path(), temp.path(), &decapod_root).unwrap();
    assert_eq!(inspected.applied.count, 1);
    assert_eq!(inspected.version_counter.version_count, Some(2));
}

#[test]
fn below_floor_and_future_versions_refuse_safely_without_mutating_on_disk_files() {
    let policy: Value = serde_json::from_str(VERSION_POLICY_JSON).unwrap();
    let temp = tempdir().unwrap();
    let decapod_root = temp.path().join(".decapod");
    let data = decapod_root.join("data");
    let db_path = load_sql_fixture_into_data_dir(&data, V2_CURRENT_SQL);

    // 1. Below-floor simulated binaries fail closed for both read and write without touching disk.
    let baseline_files = snapshot_dir_digests(&data);
    {
        let ro = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        for old_ver in policy["below_floor_versions"].as_array().unwrap() {
            let old = old_ver.as_str().unwrap();
            for require_write in [false, true] {
                let err = storage_compaction::check_storage_format_compatibility_for_version(
                    &ro,
                    old,
                    require_write,
                )
                .unwrap_err()
                .to_string();
                assert!(
                    err.contains("STORAGE_FORMAT_INCOMPATIBLE") && err.contains("0.144.2"),
                    "expected refusal for {old} (write={require_write}), got: {err}"
                );
            }
        }
        for ok_ver in policy["supported_versions"].as_array().unwrap() {
            let ok = ok_ver.as_str().unwrap();
            for require_write in [false, true] {
                storage_compaction::check_storage_format_compatibility_for_version(
                    &ro,
                    ok,
                    require_write,
                )
                .unwrap();
            }
        }
    }
    assert_eq!(snapshot_dir_digests(&data), baseline_files);

    // 2. Future min_writer_version allows read-only paths but refuses ALL write/migrate paths with zero disk mutation.
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute(
            "UPDATE meta SET value = ?1 WHERE namespace='storage' AND key='min_writer_version'",
            [policy["future_min_writer_version"].as_str().unwrap()],
        )
        .unwrap();
    }
    let writer_locked_files = snapshot_dir_digests(&data);

    // Read-only paths succeed without modifying disk.
    assert!(!events::query(&data, events::TODO, 10).unwrap().is_empty());
    assert_eq!(todo::task_journal_verify(&data).unwrap()["status"], "ok");
    let dry = storage_compaction::compact_datastore_lossless(&data, true, false).unwrap();
    assert!(dry.dry_run);
    assert_eq!(snapshot_dir_digests(&data), writer_locked_files);

    // Write/migrate paths fail closed without modifying disk.
    assert!(
        todo::initialize_todo_db(&data)
            .unwrap_err()
            .to_string()
            .contains("STORAGE_FORMAT_INCOMPATIBLE")
    );
    {
        let conn = Connection::open(&db_path).unwrap();
        assert!(
            storage_compaction::apply_v2_compaction_migration(&conn)
                .unwrap_err()
                .to_string()
                .contains("STORAGE_FORMAT_INCOMPATIBLE")
        );
        assert!(
            events::append_on_conn(
                &conn,
                events::TODO,
                &json!({"event_id":"blocked","event_type":"task.comment","task_id":"feat_hist1937compat01","ts":"2026-11-23T00:00:00Z","actor":"tester","payload":{}})
            )
            .unwrap_err()
            .to_string()
            .contains("STORAGE_FORMAT_INCOMPATIBLE")
        );
    }
    assert!(
        storage_compaction::compact_datastore_lossless(&data, false, false)
            .unwrap_err()
            .to_string()
            .contains("STORAGE_FORMAT_INCOMPATIBLE")
    );
    assert!(
        migration::check_and_migrate_with_backup_report(&decapod_root, |_| Ok(()))
            .unwrap_err()
            .to_string()
            .contains("STORAGE_FORMAT_INCOMPATIBLE")
    );
    assert_eq!(snapshot_dir_digests(&data), writer_locked_files);

    // 3. Future min_reader_version and future format_version refuse BOTH reads and writes without modifying disk.
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute(
            "UPDATE meta SET value = ?1 WHERE namespace='storage' AND key='min_reader_version'",
            [policy["future_min_reader_version"].as_str().unwrap()],
        )
        .unwrap();
    }
    let reader_locked_files = snapshot_dir_digests(&data);
    assert!(
        events::query(&data, events::TODO, 10)
            .unwrap_err()
            .to_string()
            .contains("STORAGE_FORMAT_INCOMPATIBLE")
    );
    assert!(
        storage_compaction::compact_datastore_lossless(&data, true, false)
            .unwrap_err()
            .to_string()
            .contains("STORAGE_FORMAT_INCOMPATIBLE")
    );
    assert_eq!(snapshot_dir_digests(&data), reader_locked_files);

    for future_fmt in policy["future_format_versions"].as_array().unwrap() {
        let fmt_str = future_fmt.as_u64().unwrap().to_string();
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute(
                "UPDATE meta SET value = '0.144.2' WHERE namespace='storage' AND key IN ('min_reader_version','min_writer_version')",
                [],
            )
            .unwrap();
            conn.execute(
                "UPDATE meta SET value = ?1 WHERE namespace='storage' AND key='format_version'",
                [&fmt_str],
            )
            .unwrap();
        }
        let fmt_locked_files = snapshot_dir_digests(&data);
        assert!(
            events::query(&data, events::TODO, 10)
                .unwrap_err()
                .to_string()
                .contains("STORAGE_FORMAT_INCOMPATIBLE")
        );
        assert!(
            todo::initialize_todo_db(&data)
                .unwrap_err()
                .to_string()
                .contains("STORAGE_FORMAT_INCOMPATIBLE")
        );
        assert_eq!(snapshot_dir_digests(&data), fmt_locked_files);
    }
}

#[test]
fn malformed_or_missing_mandatory_fields_fail_closed() {
    let vectors: Value = serde_json::from_str(MALFORMED_VECTORS_JSON).unwrap();

    // 1. Malformed storage.* meta stamps.
    for case in vectors["invalid_meta_stamps"].as_array().unwrap() {
        let temp = tempdir().unwrap();
        let data = temp.path().join("data");
        let db_path = load_sql_fixture_into_data_dir(&data, V2_CURRENT_SQL);
        let conn = Connection::open(&db_path).unwrap();
        conn.execute(
            "UPDATE meta SET value = ?1 WHERE namespace='storage' AND key = ?2",
            params![
                case["value"].as_str().unwrap(),
                case["key"].as_str().unwrap()
            ],
        )
        .unwrap();
        let read_res = storage_compaction::check_read_compatibility(&conn);
        let write_res = storage_compaction::check_storage_format_compatibility(&conn);
        if case["expect_read_err"].as_bool().unwrap() {
            assert!(
                read_res
                    .unwrap_err()
                    .to_string()
                    .contains("STORAGE_FORMAT_INCOMPATIBLE")
            );
        } else {
            read_res.unwrap();
        }
        if case["expect_write_err"].as_bool().unwrap() {
            assert!(
                write_res
                    .unwrap_err()
                    .to_string()
                    .contains("STORAGE_FORMAT_INCOMPATIBLE")
            );
        }
    }

    // 2. Malformed `_ce_v1` compact envelopes.
    for case in vectors["invalid_compact_envelopes"].as_array().unwrap() {
        let err = events::reconstruct_stored_payload(
            events::TODO,
            "EV_MALFORMED",
            "2026-11-23T00:00:00Z",
            "task.comment",
            "tester",
            case["subject_kind"].as_str(),
            case["subject_id"].as_str(),
            case["raw"].as_str().unwrap(),
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.contains("COMPACT_EVENT_ENVELOPE"),
            "case {} unexpected err: {err}",
            case["name"]
        );
    }

    // 3. Corrupt `DCPLCM1\0` storage_codec blobs (exercised through real SQLite BLOB storage).
    {
        let temp = tempdir().unwrap();
        let data = temp.path().join("data");
        let db_path = load_sql_fixture_into_data_dir(&data, V2_CURRENT_SQL);
        let conn = Connection::open(&db_path).unwrap();
        let valid_blob: Vec<u8> = conn
            .query_row(
                "SELECT stored_bytes FROM storage_components WHERE codec_version = 1 LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let decode_blob = |blob: &[u8]| {
            conn.execute(
                "INSERT OR REPLACE INTO summaries(summary_hash,ts,scope,original_hashes,summary_text,token_estimate)
                 VALUES('corrupt_probe','2026-11-23T00:00:00Z','repo','[]',?1,1)",
                [blob],
            )
            .unwrap();
            let param = conn
                .query_row(
                    "SELECT summary_text FROM summaries WHERE summary_hash='corrupt_probe'",
                    [],
                    |r| Ok(r.parameters().into_iter().next().unwrap()),
                )
                .unwrap();
            storage_codec::decode(&param)
        };
        assert!(decode_blob(b"BADMAGIC").is_err());
        assert!(decode_blob(&valid_blob[..20]).is_err());
        let mut bad_sha = valid_blob.clone();
        bad_sha[15] ^= 0xff;
        assert!(decode_blob(&bad_sha).is_err());
        let mut bad_len = valid_blob.clone();
        bad_len[8..12].copy_from_slice(&1u32.to_le_bytes());
        assert!(decode_blob(&bad_len).is_err());
        let mut bad_zlib = valid_blob.clone();
        let last = bad_zlib.len() - 1;
        bad_zlib[last] ^= 0xff;
        assert!(decode_blob(&bad_zlib).is_err());
    }

    // 4. Malformed / missing mandatory fields / unknown fields in `task.journal.checkpoint`.
    for case in vectors["invalid_checkpoint_payloads"].as_array().unwrap() {
        let temp = tempdir().unwrap();
        let data = temp.path().join("data");
        let db_path = load_sql_fixture_into_data_dir(&data, V2_CURRENT_SQL);
        let conn = Connection::open(&db_path).unwrap();
        let mutated = json!({ "_ce_v1": 95, "payload": case["mutated"] });
        conn.execute(
            "UPDATE events SET payload = ?1 WHERE stream='todo' AND seq = 4",
            [mutated.to_string()],
        )
        .unwrap();
        let err = todo::task_journal_verify(&data).unwrap_err().to_string();
        assert!(
            err.contains("TASK_JOURNAL_CONFLICT"),
            "case {} unexpected err: {err}",
            case["name"]
        );
    }

    // 5. Missing externalized storage_components row & tampered component bytes fail closed.
    {
        let temp = tempdir().unwrap();
        let data = temp.path().join("data");
        let db_path = load_sql_fixture_into_data_dir(&data, V2_CURRENT_SQL);
        let conn = Connection::open(&db_path).unwrap();
        conn.execute("PRAGMA foreign_keys = OFF", []).unwrap();
        conn.execute("DELETE FROM storage_components WHERE codec_version = 1", [])
            .unwrap();
        let err = todo::task_journal_verify(&data).unwrap_err().to_string();
        assert!(err.contains("TASK_JOURNAL_CONFLICT"), "{err}");
    }
    {
        let temp = tempdir().unwrap();
        let data = temp.path().join("data");
        let db_path = load_sql_fixture_into_data_dir(&data, V2_CURRENT_SQL);
        let conn = Connection::open(&db_path).unwrap();
        conn.execute(
            "UPDATE storage_components SET stored_bytes = X'636f7272757074' WHERE codec_version = 0",
            [],
        )
        .unwrap();
        let err = todo::task_journal_verify(&data).unwrap_err().to_string();
        assert!(err.contains("TASK_JOURNAL_CONFLICT"), "{err}");
    }
}

#[test]
fn duplicate_migrator_runs_and_interrupted_upgrade_rollback_leave_recoverable_state() {
    // 1. Duplicate migrator runs are strictly idempotent and preserve digests.
    {
        let temp = tempdir().unwrap();
        let decapod_root = temp.path().join(".decapod");
        let data = decapod_root.join("data");
        let db_path = load_sql_fixture_into_data_dir(&data, V1_PRE_CODEC_SQL);

        let r1 =
            migration::check_and_migrate_with_backup_report(&decapod_root, |_| Ok(())).unwrap();
        assert!(!r1.applied_migrations.is_empty());
        let events_after_1 = events::query(&data, events::TODO, usize::MAX).unwrap();
        let verify_after_1 = todo::task_journal_verify(&data).unwrap();

        let r2 =
            migration::check_and_migrate_with_backup_report(&decapod_root, |_| Ok(())).unwrap();
        assert!(r2.applied_migrations.is_empty());
        assert_eq!(
            events::query(&data, events::TODO, usize::MAX).unwrap(),
            events_after_1
        );
        assert_eq!(todo::task_journal_verify(&data).unwrap(), verify_after_1);

        let conn = Connection::open(&db_path).unwrap();
        let m1 = storage_compaction::apply_v2_compaction_migration(&conn).unwrap();
        let m2 = storage_compaction::apply_v2_compaction_migration(&conn).unwrap();
        assert_eq!(m1.events_logical_sha256, m2.events_logical_sha256);
        assert_eq!(m2.event_compaction.compacted_rows, 0);
        assert_eq!(m2.prompt_dedupe.duplicate_rows_removed, 0);
    }

    // 2. Interrupted mid-transaction v2 compaction rolls back cleanly and recovers on retry.
    {
        let temp = tempdir().unwrap();
        let data = temp.path().join("data");
        let db_path = load_sql_fixture_into_data_dir(&data, V1_PRE_CODEC_SQL);
        let conn = Connection::open(&db_path).unwrap();
        let events_before = events::query(&data, events::TODO, usize::MAX).unwrap();
        let broker_raw_before: String = conn
            .query_row(
                "SELECT payload FROM events WHERE event_id = 'EV_V1_BROKER_1'",
                [],
                |r| r.get(0),
            )
            .unwrap();

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS storage_migration_manifests (
                migration_id TEXT PRIMARY KEY,
                schema_version TEXT NOT NULL,
                applied_at TEXT NOT NULL,
                binary_version TEXT NOT NULL,
                pre_digest TEXT NOT NULL,
                post_digest TEXT NOT NULL,
                manifest_json TEXT NOT NULL
             ) WITHOUT ROWID;",
        )
        .unwrap();
        conn.execute(
            "CREATE TRIGGER abort_mid_v2_upgrade
             BEFORE INSERT ON storage_migration_manifests
             BEGIN
                 SELECT RAISE(ABORT, 'simulated_power_loss_mid_upgrade');
             END",
            [],
        )
        .unwrap();

        let err = storage_compaction::apply_v2_compaction_migration(&conn).unwrap_err();
        assert!(err.to_string().contains("simulated_power_loss_mid_upgrade"));

        // Transaction rollback left events, prompts, and meta untouched.
        let broker_raw_after_abort: String = conn
            .query_row(
                "SELECT payload FROM events WHERE event_id = 'EV_V1_BROKER_1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(broker_raw_after_abort, broker_raw_before);
        let prompt_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM agent_prompts WHERE context = 'compat'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(prompt_count, 2);
        let meta_fmt: Option<String> = conn
            .query_row(
                "SELECT value FROM meta WHERE namespace='storage' AND key='format_version'",
                [],
                |r| r.get(0),
            )
            .ok();
        assert_eq!(meta_fmt, None);

        // Drop abort trigger and retry upgrade cleanly.
        conn.execute_batch("DROP TRIGGER abort_mid_v2_upgrade;")
            .unwrap();
        let manifest = storage_compaction::apply_v2_compaction_migration(&conn).unwrap();
        assert_eq!(manifest.format_version, schemas::STORAGE_FORMAT_VERSION);
        assert_eq!(
            events::query(&data, events::TODO, usize::MAX).unwrap(),
            events_before
        );
    }

    // 3. Post-migration verification failure rolls back BOTH `.decapod/data` AND `applied.json`/managed files,
    //    preserving stale `.migration_backup_*` markers and allowing clean retry recovery.
    {
        let temp = tempdir().unwrap();
        let decapod_root = temp.path().join(".decapod");
        let data = decapod_root.join("data");
        let db_path = load_sql_fixture_into_data_dir(&data, V1_PRE_CODEC_SQL);

        // Seed stale backup dir from a prior crashed run + partial applied.json before seq=1000.
        let stale_backup = data.join(".migration_backup_0_140_0_STALE");
        fs::create_dir_all(&stale_backup).unwrap();
        fs::write(stale_backup.join("sentinel.txt"), b"keep-me").unwrap();

        fs::create_dir_all(decapod_root.join("managed/migrations")).unwrap();
        let prior_entries: Vec<_> = migration::all_migrations()
            .into_iter()
            .filter(|m| m.sequence < 1000)
            .map(|m| {
                json!({
                    "id": m.id,
                    "sequence": m.sequence,
                    "scope": m.scope,
                    "kind": m.kind,
                    "script_path": m.script_path,
                    "min_version": m.min_version,
                    "target_version": m.target_version,
                    "applied_at": "1700000000Z",
                    "applied_by_version": "0.130.0"
                })
            })
            .collect();
        let applied_path = decapod_root.join("managed/migrations/applied.json");
        let initial_applied_bytes = serde_json::to_vec_pretty(
            &json!({ "schema_version": "1.0.0", "entries": prior_entries }),
        )
        .unwrap();
        fs::write(&applied_path, &initial_applied_bytes).unwrap();
        let pre_db_sha = sha256_hex(&fs::read(&db_path).unwrap());

        let err = migration::check_and_migrate_with_backup_report(&decapod_root, |_| {
            Err(DecapodError::ValidationError(
                "simulated_post_migration_verify_failure".into(),
            ))
        })
        .unwrap_err()
        .to_string();
        assert!(err.contains("Migration failed; restored .decapod/data backup"));
        assert!(err.contains("simulated_post_migration_verify_failure"));

        // Both data and managed migration ledger rolled back atomically!
        assert_eq!(sha256_hex(&fs::read(&db_path).unwrap()), pre_db_sha);
        assert_eq!(fs::read(&applied_path).unwrap(), initial_applied_bytes);
        assert!(!decapod_root.join("managed/version_counter.json").exists());
        assert!(
            !decapod_root
                .join("managed/migrations/catalog.json")
                .exists()
        );
        assert_eq!(
            fs::read(stale_backup.join("sentinel.txt")).unwrap(),
            b"keep-me"
        );

        // Subsequent recovery run applies all pending migrations cleanly!
        let recovered =
            migration::check_and_migrate_with_backup_report(&decapod_root, |_| Ok(())).unwrap();
        assert!(
            recovered
                .applied_migrations
                .iter()
                .any(|id| id == storage_compaction::V2_COMPACTION_MIGRATION_ID)
        );
        assert_eq!(todo::task_journal_verify(&data).unwrap()["status"], "ok");
    }
}

#[test]
fn local_and_hosted_logical_schema_parity_preserves_semantics_without_engine_byte_equality() {
    let temp = tempdir().unwrap();
    let matrix_report = cloud_parity_matrix::inspect_cloud_parity(temp.path()).unwrap();
    assert!(!matrix_report.blanket_parity_claimed);
    assert!(matrix_report.cross_check.consistent);
    assert!(matrix_report.cross_check.blanket_parity_rejected);
    assert!(matrix_report.cross_check.cloud_sqlite_fallback_forbidden);

    // UTF-8 byte length equivalence (Rust str::len == SQLite CAST(x AS BLOB) == PostgreSQL octet_length).
    let multibyte = "🦀".repeat(64);
    assert_eq!(multibyte.len(), 256);
    storage_contracts::validate_utf8_field(
        "title",
        &multibyte,
        storage_contracts::MAX_TITLE_BYTES,
        true,
    )
    .unwrap();
    let over_title = "🦀".repeat(257);
    assert!(
        storage_contracts::validate_utf8_field(
            "title",
            &over_title,
            storage_contracts::MAX_TITLE_BYTES,
            true
        )
        .unwrap_err()
        .to_string()
        .contains("STORAGE_CONTRACT_VIOLATION")
    );

    // Cloud backend refuses local SQLite compaction without creating local SQLite files.
    let init_out = std::process::Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["init", "--backend", "cloud", "--force", "--proof"])
        .current_dir(temp.path())
        .output()
        .expect("run decapod init --backend cloud");
    assert!(
        init_out.status.success(),
        "cloud init failed: {}",
        String::from_utf8_lossy(&init_out.stderr)
    );
    let cloud_err = storage_compaction::compact_datastore_lossless(
        &temp.path().join(".decapod/data"),
        false,
        false,
    )
    .unwrap_err()
    .to_string();
    assert!(
        cloud_err.contains("DATA_COMPACT_CLOUD_UNSUPPORTED"),
        "{cloud_err}"
    );
    assert!(!temp.path().join(".decapod/data/decapod.db").exists());

    // Documentation explicitly links Dactyl wire evolution and Propodus deployment skew without claiming engine byte equality.
    assert!(ENCODING_COMPAT_DOC.contains("DecapodLabs/dactyl#146"));
    assert!(ENCODING_COMPAT_DOC.contains("DecapodLabs/propodus#227"));
    assert!(
        ENCODING_COMPAT_DOC.contains("without imposing physical DDL or byte-level engine equality")
    );
}
