//! Fail-closed inventory smoke checks on source-controlled logical schema.
use decapod::core::{db::Connection, schemas, todo};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn logical_schema_inventory_is_structurally_valid_and_matches_runtime_core() {
    let contract: Value = serde_json::from_str(include_str!(
        "../docs/datastore-v2/logical_contract_v1.json"
    ))
    .expect("reviewed JSON contract");
    assert_eq!(contract["schema"], "decapod.logical-storage-inventory.v1");
    let contract_release = contract["release"].as_str().expect("contract release");
    let parse_semver = |v: &str| -> (u64, u64, u64) {
        let mut parts = v.split('.').map(|p| p.parse::<u64>().expect("semver int"));
        (
            parts.next().expect("major"),
            parts.next().expect("minor"),
            parts.next().expect("patch"),
        )
    };
    assert!(
        parse_semver(contract_release) >= (0, 166, 1)
            && parse_semver(contract_release) <= parse_semver(env!("CARGO_PKG_VERSION")),
        "contract release {contract_release} must stay within [0.166.1, {}]",
        env!("CARGO_PKG_VERSION")
    );

    // Keep reviewed JSON limits exactly aligned with runtime storage_contracts::contract_manifest().
    let runtime_manifest = decapod::core::storage_contracts::contract_manifest();
    let limits = &contract["field_contracts"]["limits"];
    assert_eq!(
        limits["id_bytes"],
        runtime_manifest["field_classes"]["id"]["max_utf8_bytes"]
    );
    assert_eq!(
        limits["digest_bytes"],
        runtime_manifest["field_classes"]["digest"]["max_utf8_bytes"]
    );
    assert_eq!(
        limits["principal_bytes"],
        runtime_manifest["field_classes"]["principal"]["max_utf8_bytes"]
    );
    assert_eq!(
        limits["repository_id_bytes"],
        runtime_manifest["field_classes"]["repository_id"]["max_utf8_bytes"]
    );
    assert_eq!(
        limits["stream_bytes"],
        runtime_manifest["field_classes"]["stream"]["max_utf8_bytes"]
    );
    assert_eq!(
        limits["event_type_bytes"],
        runtime_manifest["field_classes"]["event_type"]["max_utf8_bytes"]
    );
    assert_eq!(
        limits["timestamp_bytes"],
        runtime_manifest["field_classes"]["timestamp"]["max_utf8_bytes"]
    );
    assert_eq!(
        limits["enum_token_bytes"],
        runtime_manifest["field_classes"]["enum_token"]["max_utf8_bytes"]
    );
    assert_eq!(
        limits["category_bytes"],
        runtime_manifest["field_classes"]["category_or_component"]["max_utf8_bytes"]
    );
    assert_eq!(
        limits["tag_bytes"],
        runtime_manifest["field_classes"]["tag"]["max_utf8_bytes"]
    );
    assert_eq!(
        limits["title_bytes"],
        runtime_manifest["field_classes"]["title"]["max_utf8_bytes"]
    );
    assert_eq!(
        limits["scope_bytes"],
        runtime_manifest["field_classes"]["scope"]["max_utf8_bytes"]
    );
    assert_eq!(
        limits["path_or_ref_bytes"],
        runtime_manifest["field_classes"]["path_or_ref"]["max_utf8_bytes"]
    );
    assert_eq!(
        limits["description_bytes"],
        runtime_manifest["field_classes"]["description_or_notes"]["max_utf8_bytes"]
    );
    assert_eq!(
        limits["proof_plan_bytes"],
        runtime_manifest["field_classes"]["proof_plan_json"]["max_utf8_bytes"]
    );
    assert_eq!(
        limits["verification_artifacts_bytes"],
        runtime_manifest["field_classes"]["verification_artifacts_json"]["max_utf8_bytes"]
    );
    assert_eq!(
        limits["body_prose_bytes"],
        runtime_manifest["field_classes"]["body_prose"]["max_utf8_bytes"]
    );
    assert_eq!(
        limits["event_payload_bytes"],
        runtime_manifest["field_classes"]["event_payload_json"]["max_utf8_bytes"]
    );
    assert_eq!(
        limits["transfer_event_payload_bytes"],
        runtime_manifest["field_classes"]["transfer_event_payload_json"]["max_utf8_bytes"]
    );
    assert_eq!(
        limits["json_max_depth"],
        runtime_manifest["field_classes"]["event_payload_json"]["max_json_depth"]
    );
    assert_eq!(
        limits["tags_per_task"],
        runtime_manifest["collection_bounds"]["tags_per_task"]
    );
    assert_eq!(
        limits["owners_per_task"],
        runtime_manifest["collection_bounds"]["owners_per_task"]
    );
    assert_eq!(
        limits["dependencies_per_task"],
        runtime_manifest["collection_bounds"]["dependencies_per_task"]
    );
    assert_eq!(
        limits["evidence_refs_per_record"],
        runtime_manifest["collection_bounds"]["evidence_refs_per_record"]
    );
    assert_eq!(
        contract["field_contracts"]["sql_boundary_parity"],
        runtime_manifest["sql_boundary_parity"]
    );
    let tables = contract["tables"].as_array().expect("tables");
    assert!(tables.len() >= 32, "missing current logical inventory");
    let mut names = BTreeSet::new();
    for table in tables {
        let name = table["logical_name"].as_str().expect("table name");
        assert!(names.insert(name), "duplicate table {name}");
        assert!(
            table["authority"]
                .as_str()
                .is_some_and(|x| x != "unclassified"),
            "unclassified authority: {name}"
        );
        let columns = table["columns"].as_array().expect("columns");
        let declared: BTreeSet<&str> = columns
            .iter()
            .map(|column| column["name"].as_str().expect("column name"))
            .collect();
        assert_eq!(declared.len(), columns.len(), "duplicate field in {name}");
        for key in table["primary_key"].as_array().expect("primary key") {
            assert!(
                declared.contains(key.as_str().expect("key name")),
                "{name}: invalid PK"
            );
        }
        for fk in table["foreign_keys"].as_array().expect("foreign keys") {
            let target = fk["ref_table"].as_str().expect("reference");
            assert!(
                tables.iter().any(|t| t["logical_name"] == target),
                "{name}: unknown FK target {target}"
            );
        }
    }

    let scratch = tempfile::tempdir().unwrap();
    let data = scratch.path().join("data");
    todo::initialize_todo_db(&data).unwrap();
    decapod::plugins::health::initialize_health_db(&data).unwrap();
    decapod::plugins::federation::initialize_federation_db(&data).unwrap();
    decapod::plugins::decide::initialize_decide_db(&data).unwrap();
    decapod::plugins::cron::initialize_cron_db(&data).unwrap();
    decapod::plugins::reflex::initialize_reflex_db(&data).unwrap();
    decapod::plugins::lcm::initialize_lcm_db(&data).unwrap();
    decapod::plugins::aptitude::initialize_aptitude_db(&data).unwrap();

    let conn = Connection::open(data.join(schemas::LOCAL_DB_NAME)).unwrap();
    let required: BTreeSet<&str> = [
        "tasks",
        "events",
        "task_owners",
        "task_dependencies",
        "task_tags",
        "task_verification",
        "agents",
        "meta",
        "risk_zones",
        "categories",
        "nodes",
        "node_edges",
        "claims",
        "proof_events",
        "decisions",
        "sessions",
        "preferences",
    ]
    .into_iter()
    .collect();
    let mut observed = BTreeMap::new();
    for table in tables {
        let name = table["logical_name"].as_str().unwrap();
        if !conn.has_table(name).unwrap() {
            assert!(
                !required.contains(name),
                "current core table missing: {name}"
            );
            // Optional and lazily created tables are audited when initialized.
            continue;
        }
        let sql = format!("PRAGMA table_info(\"{name}\")");
        let mut statement = conn.prepare(&sql).unwrap();
        let fields: Vec<String> = statement
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        let declared: Vec<&str> = table["columns"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["name"].as_str().unwrap())
            .collect();
        let field_refs: Vec<&str> = fields.iter().map(String::as_str).collect();
        assert_eq!(field_refs, declared, "{name}: local schema drift");
        observed.insert(name.to_owned(), fields.len());
    }
    for name in required {
        assert!(observed.contains_key(name), "{name}: not verified");
    }
}

#[test]
fn write_admission_enforces_utf8_byte_bounds_nul_freedom_json_depth_and_collection_limits() {
    use decapod::core::{events, storage_contracts};
    use decapod::plugins::aptitude;
    use serde_json::json;

    let scratch = tempfile::tempdir().unwrap();
    let data = scratch.path().join("data");
    todo::initialize_todo_db(&data).unwrap();
    aptitude::initialize_aptitude_db(&data).unwrap();

    // 1. Multi-byte UTF-8 boundary: 256 4-byte scalars = 1024 UTF-8 bytes (exact MAX_TITLE_BYTES)
    let exact_title = "🦀".repeat(256);
    assert_eq!(exact_title.len(), storage_contracts::MAX_TITLE_BYTES);
    let ok_task = todo::add_task(
        &data,
        &todo::TodoCommand::Add {
            title: exact_title.clone(),
            description: "bounded multibyte check".into(),
            priority: "high".into(),
            tags: "utf8,contract".into(),
            owner: String::new(),
            due: None,
            r#ref: String::new(),
            scope: "root".into(),
            dir: Some(scratch.path().to_string_lossy().into_owned()),
            depends_on: String::new(),
            blocks: String::new(),
            parent: None,
            one_shot: 0,
        },
    )
    .expect("exact 1024 UTF-8 byte title must pass without truncation");
    let task_id = ok_task["id"].as_str().unwrap().to_owned();

    let conn = Connection::open(data.join(schemas::LOCAL_DB_NAME)).unwrap();
    let stored_title: String = conn
        .query_row("SELECT title FROM tasks WHERE id = ?1", [&task_id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(stored_title, exact_title);

    // 1025 UTF-8 bytes (257 chars << 1024 chars, but > 1024 UTF-8 bytes) must fail closed.
    let over_title = format!("{exact_title}a");
    let err = todo::add_task(
        &data,
        &todo::TodoCommand::Add {
            title: over_title,
            description: String::new(),
            priority: "medium".into(),
            tags: String::new(),
            owner: String::new(),
            due: None,
            r#ref: String::new(),
            scope: "root".into(),
            dir: Some(scratch.path().to_string_lossy().into_owned()),
            depends_on: String::new(),
            blocks: String::new(),
            parent: None,
            one_shot: 0,
        },
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("STORAGE_CONTRACT_VIOLATION"),
        "{err}"
    );

    // 2. Embedded NUL (0x00) rejection across task, comment, event value/key, and aptitude prompt.
    let nul_task_err = todo::add_task(
        &data,
        &todo::TodoCommand::Add {
            title: "bad\0title".into(),
            description: String::new(),
            priority: "medium".into(),
            tags: String::new(),
            owner: String::new(),
            due: None,
            r#ref: String::new(),
            scope: "root".into(),
            dir: Some(scratch.path().to_string_lossy().into_owned()),
            depends_on: String::new(),
            blocks: String::new(),
            parent: None,
            one_shot: 0,
        },
    )
    .unwrap_err();
    assert!(nul_task_err.to_string().contains("NUL"));

    let nul_comment_err = todo::comment_task(&data, &task_id, "comment\0nul").unwrap_err();
    assert!(nul_comment_err.to_string().contains("NUL"));

    let nul_val_err = events::append_on_conn(
        &conn,
        "todo",
        &json!({"event_id": "nul-val", "event_type": "task.comment", "actor": "tester", "payload": {"note": "a\0b"}}),
    )
    .unwrap_err();
    assert!(nul_val_err.to_string().contains("NUL"));

    let nul_key_err = events::append_on_conn(
        &conn,
        "todo",
        &json!({"event_id": "nul-key", "event_type": "task.comment", "actor": "tester", "payload": {"bad\0key": "ok"}}),
    )
    .unwrap_err();
    assert!(nul_key_err.to_string().contains("NUL"));

    let store = decapod::core::store::Store {
        kind: decapod::core::store::StoreKind::Repo,
        root: data.clone(),
    };
    let nul_prompt_err =
        aptitude::add_agent_prompt(&store, "git_commit", "prompt\0body", Some(50)).unwrap_err();
    assert!(nul_prompt_err.to_string().contains("NUL"));

    // 3. JSON nesting depth > MAX_JSON_DEPTH rejected in verification and event append.
    let mut deep = json!("leaf");
    for _ in 0..(storage_contracts::MAX_JSON_DEPTH + 2) {
        deep = json!({"n": deep});
    }
    let deep_str = serde_json::to_string(&deep).unwrap();
    let ver_err = storage_contracts::validate_verification_write(
        &task_id,
        "[]",
        Some(&deep_str),
        Some("passed"),
        None,
    )
    .unwrap_err();
    assert!(ver_err.to_string().contains("JSON nesting depth"));

    let ev_depth_err = events::append_on_conn(
        &conn,
        "todo",
        &json!({"event_id": "deep-ev", "event_type": "task.comment", "actor": "tester", "payload": deep}),
    )
    .unwrap_err();
    assert!(ev_depth_err.to_string().contains("JSON nesting depth"));

    // 4. Collection cardinality bounds (tags > 64, owners > 32, dependencies > 128).
    let too_many_tags = (0..=storage_contracts::MAX_TAGS_PER_TASK)
        .map(|i| format!("t{i}"))
        .collect::<Vec<_>>()
        .join(",");
    assert!(
        todo::add_task(
            &data,
            &todo::TodoCommand::Add {
                title: "tags".into(),
                description: String::new(),
                priority: "low".into(),
                tags: too_many_tags,
                owner: String::new(),
                due: None,
                r#ref: String::new(),
                scope: "root".into(),
                dir: Some(scratch.path().to_string_lossy().into_owned()),
                depends_on: String::new(),
                blocks: String::new(),
                parent: None,
                one_shot: 0,
            },
        )
        .is_err()
    );

    let too_many_owners = (0..=storage_contracts::MAX_OWNERS_PER_TASK)
        .map(|i| format!("owner-{i}"))
        .collect::<Vec<_>>()
        .join(",");
    assert!(todo::edit_task(&data, &task_id, None, None, Some(&too_many_owners), None,).is_err());

    let too_many_deps = (0..=storage_contracts::MAX_DEPENDENCIES_PER_TASK)
        .map(|i| format!("dep_{i:04}"))
        .collect::<Vec<_>>()
        .join(",");
    assert!(
        todo::add_task(
            &data,
            &todo::TodoCommand::Add {
                title: "deps".into(),
                description: String::new(),
                priority: "low".into(),
                tags: String::new(),
                owner: String::new(),
                due: None,
                r#ref: String::new(),
                scope: "root".into(),
                dir: Some(scratch.path().to_string_lossy().into_owned()),
                depends_on: too_many_deps,
                blocks: String::new(),
                parent: None,
                one_shot: 0,
            },
        )
        .is_err()
    );
}

#[test]
fn historical_oversized_task_field_remains_losslessly_readable_and_patchable_on_unrelated_columns()
{
    use decapod::core::storage_contracts;

    let scratch = tempfile::tempdir().unwrap();
    let data = scratch.path().join("data");
    todo::initialize_todo_db(&data).unwrap();

    let created = todo::add_task(
        &data,
        &todo::TodoCommand::Add {
            title: "Legacy task".into(),
            description: "initial".into(),
            priority: "medium".into(),
            tags: "legacy".into(),
            owner: String::new(),
            due: None,
            r#ref: String::new(),
            scope: "root".into(),
            dir: Some(scratch.path().to_string_lossy().into_owned()),
            depends_on: String::new(),
            blocks: String::new(),
            parent: None,
            one_shot: 0,
        },
    )
    .unwrap();
    let id = created["id"].as_str().unwrap().to_owned();

    // Simulate a pre-0.142 historical row whose description exceeds MAX_DESCRIPTION_BYTES (64 KiB)
    // and re-baseline it so journal custody agrees with the historical row.
    let historical_oversized = "H".repeat(storage_contracts::MAX_DESCRIPTION_BYTES + 34_464);
    assert_eq!(historical_oversized.len(), 100_000);
    {
        let conn = Connection::open(data.join(schemas::LOCAL_DB_NAME)).unwrap();
        conn.execute("DELETE FROM events WHERE stream = 'todo'", [])
            .unwrap();
        conn.execute("DROP TRIGGER IF EXISTS trg_decapod_tasks_upd_contract", [])
            .unwrap();
        conn.execute(
            "UPDATE tasks SET description = ?1 WHERE id = ?2",
            decapod::params![historical_oversized, id],
        )
        .unwrap();
    }
    todo::initialize_todo_db(&data).unwrap();
    todo::task_journal_checkpoint(&data, None).unwrap();

    // Direct SQL overwrite of bounded description must fail closed inside SQLite once triggers are active.
    {
        let conn = Connection::open(data.join(schemas::LOCAL_DB_NAME)).unwrap();
        let sql_err = conn
            .execute(
                "UPDATE tasks SET description = ?1 WHERE id = ?2",
                decapod::params!["J".repeat(storage_contracts::MAX_DESCRIPTION_BYTES + 1), id],
            )
            .unwrap_err();
        assert!(
            sql_err.to_string().contains("STORAGE_CONTRACT_VIOLATION"),
            "expected SQLite trigger STORAGE_CONTRACT_VIOLATION, got {sql_err}"
        );
    }

    // Updating unrelated columns (title + owner) must succeed and preserve all 100,000 bytes.
    let patched = todo::edit_task(
        &data,
        &id,
        Some("Retitled without touching oversized description"),
        None,
        Some("patched-owner"),
        None,
    )
    .expect("unrelated patch on historical oversized task must succeed");
    assert_eq!(patched["status"], "ok");

    let conn = Connection::open(data.join(schemas::LOCAL_DB_NAME)).unwrap();
    let (title, desc, owner): (String, String, String) = conn
        .query_row(
            "SELECT title, description, owner FROM tasks WHERE id = ?1",
            [&id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(title, "Retitled without touching oversized description");
    assert_eq!(desc.len(), 100_000);
    assert_eq!(desc, historical_oversized);
    assert_eq!(owner, "patched-owner");
}

#[test]
fn authority_convergence_contract_preserves_unresolved_dependencies_and_detects_drift() {
    use decapod::core::storage_contracts;

    let scratch = tempfile::tempdir().unwrap();
    let data = scratch.path().join("data");
    todo::initialize_todo_db(&data).unwrap();

    let dep = todo::add_task(
        &data,
        &todo::TodoCommand::Add {
            title: "Upstream prerequisite".into(),
            description: String::new(),
            priority: "high".into(),
            tags: " zeta , alpha , zeta ".into(),
            owner: String::new(),
            due: None,
            r#ref: String::new(),
            scope: "root".into(),
            dir: Some(scratch.path().to_string_lossy().into_owned()),
            depends_on: String::new(),
            blocks: String::new(),
            parent: None,
            one_shot: 0,
        },
    )
    .unwrap();
    let dep_id = dep["id"].as_str().unwrap().to_owned();

    let downstream = todo::add_task(
        &data,
        &todo::TodoCommand::Add {
            title: "Downstream consumer".into(),
            description: String::new(),
            priority: "medium".into(),
            tags: "beta,alpha".into(),
            owner: String::new(),
            due: None,
            r#ref: String::new(),
            scope: "root".into(),
            dir: Some(scratch.path().to_string_lossy().into_owned()),
            depends_on: format!("{dep_id},future_planned_task_001"),
            blocks: String::new(),
            parent: None,
            one_shot: 0,
        },
    )
    .unwrap();
    let down_id = downstream["id"].as_str().unwrap().to_owned();

    let conn = Connection::open(data.join(schemas::LOCAL_DB_NAME)).unwrap();
    let report = storage_contracts::audit_task_authority_convergence(&conn).unwrap();
    assert!(report.is_converged(), "{report:?}");
    assert!(report.noncanonical_tag_csv_tasks.is_empty(), "{report:?}");
    assert_eq!(
        report.unresolved_declared_dependencies.get(&down_id),
        Some(&vec!["future_planned_task_001".to_owned()])
    );

    // Injecting an unreflected tag row must be flagged by the authority convergence audit.
    conn.execute(
        "INSERT INTO task_tags(task_id, tag) VALUES(?1, 'orphan-tag')",
        [&dep_id],
    )
    .unwrap();
    let drifted = storage_contracts::audit_task_authority_convergence(&conn).unwrap();
    assert!(!drifted.is_converged());
    assert_eq!(drifted.tag_set_divergences, vec![dep_id]);
}

#[test]
fn events_read_and_write_paths_enforce_storage_format_compatibility_boundaries() {
    use decapod::core::{events, storage_compaction};
    use serde_json::json;

    let scratch = tempfile::tempdir().unwrap();
    let data = scratch.path().join("data");
    todo::initialize_todo_db(&data).unwrap();

    let added = todo::add_task(
        &data,
        &todo::TodoCommand::Add {
            title: "Compatibility contract task".into(),
            description: "Verifies storage format floor on event boundary".into(),
            priority: "medium".into(),
            tags: "compat".into(),
            owner: String::new(),
            due: None,
            r#ref: String::new(),
            scope: "root".into(),
            dir: Some(scratch.path().to_string_lossy().into_owned()),
            depends_on: String::new(),
            blocks: String::new(),
            parent: None,
            one_shot: 0,
        },
    )
    .unwrap();
    let task_id = added["id"].as_str().unwrap().to_owned();

    let conn = Connection::open(data.join(schemas::LOCAL_DB_NAME)).unwrap();
    let subject_event = json!({
        "event_id": "compat-subject-1",
        "event_type": "workunit.checkpoint",
        "task_id": task_id,
        "ts": "2026-11-18T00:00:00Z",
        "actor": "tester",
        "payload": {"step": 1}
    });
    events::append_on_conn(&conn, events::WORKUNIT, &subject_event).unwrap();

    // Confirm advertised compatibility floor is 0.144.2 (rejecting 0.141.0 and 0.144.1).
    assert!(
        storage_compaction::check_storage_format_compatibility_for_version(&conn, "0.144.1", false)
            .is_err()
    );
    storage_compaction::check_storage_format_compatibility_for_version(&conn, "0.144.2", true)
        .unwrap();

    // Higher min_writer_version allows reads while blocking event writes & capacity reservations.
    conn.execute(
        "UPDATE meta SET value = '99.0.0' WHERE namespace = 'storage' AND key = 'min_writer_version'",
        [],
    )
    .unwrap();
    assert!(!events::query(&data, events::TODO, 10).unwrap().is_empty());
    assert!(
        !events::query_all_on_conn(&conn, events::TODO)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        events::query_subject_on_conn(&conn, events::WORKUNIT, "task", &task_id, 10)
            .unwrap()
            .len(),
        1
    );
    assert!(
        events::query_subject_event_on_conn(
            &conn,
            events::WORKUNIT,
            "task",
            &task_id,
            "compat-subject-1"
        )
        .unwrap()
        .is_some()
    );

    let next_event = json!({
        "event_id": "compat-subject-2",
        "event_type": "workunit.checkpoint",
        "task_id": task_id,
        "ts": "2026-11-18T00:01:00Z",
        "actor": "tester",
        "payload": {"step": 2}
    });
    assert!(
        events::append_on_conn(&conn, events::WORKUNIT, &next_event)
            .unwrap_err()
            .to_string()
            .contains("STORAGE_FORMAT_INCOMPATIBLE")
    );
    conn.execute_batch("BEGIN IMMEDIATE").unwrap();
    let cap_err = events::check_subject_append_capacity(
        &conn,
        events::WORKUNIT,
        "task",
        &task_id,
        &next_event,
        10,
    )
    .unwrap_err()
    .to_string();
    conn.execute_batch("ROLLBACK").unwrap();
    assert!(cap_err.contains("STORAGE_FORMAT_INCOMPATIBLE"));
    assert!(
        events::ensure_tables(&conn)
            .unwrap_err()
            .to_string()
            .contains("STORAGE_FORMAT_INCOMPATIBLE")
    );

    // Higher min_reader_version blocks both reads and writes fail-closed.
    conn.execute(
        "UPDATE meta SET value = '99.0.0' WHERE namespace = 'storage' AND key = 'min_reader_version'",
        [],
    )
    .unwrap();
    assert!(
        events::query(&data, events::TODO, 10)
            .unwrap_err()
            .to_string()
            .contains("STORAGE_FORMAT_INCOMPATIBLE")
    );
    assert!(
        events::query_all_on_conn(&conn, events::TODO)
            .unwrap_err()
            .to_string()
            .contains("STORAGE_FORMAT_INCOMPATIBLE")
    );
    assert!(
        events::query_subject_on_conn(&conn, events::WORKUNIT, "task", &task_id, 10)
            .unwrap_err()
            .to_string()
            .contains("STORAGE_FORMAT_INCOMPATIBLE")
    );
    assert!(
        events::query_subject_event_on_conn(
            &conn,
            events::WORKUNIT,
            "task",
            &task_id,
            "compat-subject-1"
        )
        .unwrap_err()
        .to_string()
        .contains("STORAGE_FORMAT_INCOMPATIBLE")
    );
}

#[test]
fn non_task_plugin_stores_enforce_utf8_byte_and_collection_bounds() {
    use decapod::core::{
        storage_contracts,
        store::{Store, StoreKind},
    };
    use decapod::plugins::{
        aptitude, archive, cron, decide, federation, feedback, health, knowledge, lcm,
    };

    let scratch = tempfile::tempdir().unwrap();
    let data = scratch.path().join("data");
    todo::initialize_todo_db(&data).unwrap();
    health::initialize_health_db(&data).unwrap();
    federation::initialize_federation_db(&data).unwrap();
    decide::initialize_decide_db(&data).unwrap();
    cron::initialize_cron_db(&data).unwrap();
    archive::initialize_archive_db(&data).unwrap();
    aptitude::initialize_aptitude_db(&data).unwrap();
    lcm::initialize_lcm_db(&data).unwrap();

    let store = Store {
        kind: StoreKind::Repo,
        root: data.clone(),
    };

    let over_title = format!("{}a", "🦀".repeat(256)); // 1025 UTF-8 bytes
    let over_id = "x".repeat(storage_contracts::MAX_ID_BYTES + 1);
    let over_body = "b".repeat(storage_contracts::MAX_BODY_PROSE_BYTES + 1);
    let over_ref = "r".repeat(storage_contracts::MAX_PATH_OR_REF_BYTES + 1);
    let over_desc = "d".repeat(storage_contracts::MAX_DESCRIPTION_BYTES + 1);

    // 1. knowledge write + promotion evidence ref bounds
    let k_err = knowledge::add_scoped_knowledge(
        &store,
        knowledge::AddKnowledgeParams {
            id: "k_ok",
            title: &over_title,
            content: "valid",
            provenance: "docs/a.md",
            claim_id: None,
            merge_key: None,
            conflict_policy: knowledge::KnowledgeConflictPolicy::Merge,
            status: "active",
            ttl_policy: "none",
            expires_ts: None,
        },
        "root",
    )
    .unwrap_err();
    assert!(
        k_err.to_string().contains("STORAGE_CONTRACT_VIOLATION"),
        "{k_err}"
    );

    let too_many_refs: Vec<String> = (0..=storage_contracts::MAX_EVIDENCE_REFS_PER_RECORD)
        .map(|i| format!("ref-{i}"))
        .collect();
    let promo_err = knowledge::record_promotion_event(
        &store,
        knowledge::KnowledgePromotionEventInput {
            source_entry_id: "k_src",
            evidence_refs: &too_many_refs,
            approved_by: "reviewer",
            actor: "tester",
            reason: "excess refs boundary check",
        },
    )
    .unwrap_err();
    assert!(
        promo_err.to_string().contains("STORAGE_CONTRACT_VIOLATION"),
        "{promo_err}"
    );

    // 2. federation node + edge bounds (UTF-8 title + NUL rejection + sources collection cap)
    let fed_err = federation::add_node(
        &store,
        &over_title,
        "decision",
        "notable",
        "agent_inferred",
        "body",
        "",
        "",
        "repo",
        None,
        "tester",
    )
    .unwrap_err();
    assert!(
        fed_err.to_string().contains("STORAGE_CONTRACT_VIOLATION"),
        "{fed_err}"
    );

    let edge_err = federation::add_edge(&store, "n\0src", "n_dst", "depends_on").unwrap_err();
    assert!(edge_err.to_string().contains("NUL"), "{edge_err}");

    // 3. health claim + proof bounds
    let hc_err = health::add_claim(&store, &over_id, "subj", "assertion", "prov").unwrap_err();
    assert!(
        hc_err.to_string().contains("STORAGE_CONTRACT_VIOLATION"),
        "{hc_err}"
    );
    let hp_err = health::record_proof(&store, "claim-1", "surface", "bad\0result", 60).unwrap_err();
    assert!(hp_err.to_string().contains("NUL"), "{hp_err}");

    // 4. decide session + decision bounds
    let ds_err = decide::start_session(&store, "tree", &over_title, "desc", "tester").unwrap_err();
    assert!(
        ds_err.to_string().contains("STORAGE_CONTRACT_VIOLATION"),
        "{ds_err}"
    );
    let dd_err =
        decide::record_decision(&store, "sess-1", "q1", "opt", &over_desc, "tester").unwrap_err();
    assert!(
        dd_err.to_string().contains("STORAGE_CONTRACT_VIOLATION"),
        "{dd_err}"
    );

    // 5. cron write bounds: run_cron_cli logs and rejects write (0 rows inserted); validator fails closed.
    cron::run_cron_cli(
        &store,
        cron::CronCli {
            command: cron::CronCommand::Add {
                name: over_title.clone(),
                description: "desc".into(),
                schedule: "* * * * *".into(),
                command: "echo ok".into(),
                status: "active".into(),
                tags: String::new(),
                dir: Some(scratch.path().to_string_lossy().into_owned()),
            },
        },
    )
    .unwrap();
    let conn = Connection::open(data.join(schemas::LOCAL_DB_NAME)).unwrap();
    let cron_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM cron_jobs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(cron_count, 0, "oversized cron write must not persist");
    let cron_err = storage_contracts::validate_cron_write(
        Some(&over_title),
        Some("desc"),
        Some("* * * * *"),
        Some("echo ok"),
        Some("active"),
        Some(""),
        None,
        None,
    )
    .unwrap_err();
    assert!(
        cron_err.to_string().contains("STORAGE_CONTRACT_VIOLATION"),
        "{cron_err}"
    );

    // 6. reflex contract validator (direct validator parity for private add_reflex/update_reflex)
    storage_contracts::validate_reflex_write(
        Some("reflex-name"),
        Some("Command reflex"),
        Some("event"),
        Some("{}"),
        Some("command"),
        Some("echo done"),
        Some("active"),
        Some(""),
    )
    .expect("command-style action_config string should be admitted when bounded");
    let reflex_err = storage_contracts::validate_reflex_write(
        Some("reflex-name"),
        Some(&over_desc),
        Some("event"),
        Some("{}"),
        Some("command"),
        Some("{}"),
        Some("active"),
        Some(""),
    )
    .unwrap_err();
    assert!(
        reflex_err
            .to_string()
            .contains("STORAGE_CONTRACT_VIOLATION"),
        "{reflex_err}"
    );
    let over_cfg = "x".repeat(storage_contracts::MAX_PROOF_PLAN_BYTES + 1);
    let reflex_cfg_err = storage_contracts::validate_reflex_write(
        Some("reflex-name"),
        Some("desc"),
        Some("event"),
        Some("{}"),
        Some("command"),
        Some(&over_cfg),
        Some("active"),
        Some(""),
    )
    .unwrap_err();
    assert!(
        reflex_cfg_err
            .to_string()
            .contains("STORAGE_CONTRACT_VIOLATION"),
        "{reflex_cfg_err}"
    );

    // 7. feedback + archive + aptitude + lcm bounds
    let fb_err = feedback::add_feedback(&store, "user", &over_desc, None).unwrap_err();
    assert!(
        fb_err.to_string().contains("STORAGE_CONTRACT_VIOLATION"),
        "{fb_err}"
    );

    let arch_err = archive::register_archive(
        &store,
        "arch-1",
        std::path::Path::new(&over_ref),
        "content",
        "summary",
    )
    .unwrap_err();
    assert!(
        arch_err.to_string().contains("STORAGE_CONTRACT_VIOLATION"),
        "{arch_err}"
    );

    let apt_pat_err = aptitude::add_pattern(
        &store,
        aptitude::PatternInput {
            name: over_title.clone(),
            category: "cli".into(),
            regex_pattern: "git .*".into(),
            preference_category: None,
            preference_key: None,
            description: None,
        },
    )
    .unwrap_err();
    assert!(
        apt_pat_err
            .to_string()
            .contains("STORAGE_CONTRACT_VIOLATION"),
        "{apt_pat_err}"
    );

    let apt_obs_err = aptitude::record_observation(&store, &over_desc, None).unwrap_err();
    assert!(
        apt_obs_err
            .to_string()
            .contains("STORAGE_CONTRACT_VIOLATION"),
        "{apt_obs_err}"
    );

    let lcm_err = lcm::ingest(&store, &over_body, "event", "tester", None, None).unwrap_err();
    assert!(
        lcm_err.to_string().contains("STORAGE_CONTRACT_VIOLATION"),
        "{lcm_err}"
    );
}
