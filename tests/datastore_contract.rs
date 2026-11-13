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
    assert_eq!(contract["release"], env!("CARGO_PKG_VERSION"));
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
        conn.execute(
            "UPDATE tasks SET description = ?1 WHERE id = ?2",
            decapod::params![historical_oversized, id],
        )
        .unwrap();
    }
    todo::task_journal_checkpoint(&data, None).unwrap();

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
