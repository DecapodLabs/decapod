//! Fail-closed inventory smoke checks on source-controlled logical schema.
use decapod::core::{db::Connection, schemas, todo};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn logical_schema_inventory_is_structurally_valid_and_matches_runtime_core() {
    let contract: Value = serde_json::from_str(include_str!("../docs/datastore-v2/logical_contract_v1.json"))
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
            table["authority"].as_str().is_some_and(|x| x != "unclassified"),
            "unclassified authority: {name}"
        );
        let columns = table["columns"].as_array().expect("columns");
        let declared: BTreeSet<&str> = columns
            .iter()
            .map(|column| column["name"].as_str().expect("column name"))
            .collect();
        assert_eq!(declared.len(), columns.len(), "duplicate field in {name}");
        for key in table["primary_key"].as_array().expect("primary key") {
            assert!(declared.contains(key.as_str().expect("key name")), "{name}: invalid PK");
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
    decapod::plugins::cron::initialize_cron_db(&data).unwrap();
    decapod::plugins::reflex::initialize_reflex_db(&data).unwrap();
    decapod::plugins::lcm::initialize_lcm_db(&data).unwrap();
    decapod::plugins::aptitude::initialize_aptitude_db(&data).unwrap();

    let conn = Connection::open(data.join(schemas::LOCAL_DB_NAME)).unwrap();
    let required: BTreeSet<&str> = [
        "tasks", "events", "task_owners", "task_dependencies", "task_tags",
        "task_verification", "agents", "meta", "risk_zones", "categories",
        "nodes", "node_edges", "claims", "proof_events", "decisions",
        "sessions", "preferences"
    ].into_iter().collect();
    let mut observed = BTreeMap::new();
    for table in tables {
        let name = table["logical_name"].as_str().unwrap();
        if !conn.has_table(name).unwrap() {
            assert!(!required.contains(name), "current core table missing: {name}");
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
        let declared: Vec<&str> = table["columns"].as_array().unwrap().iter()
            .map(|c| c["name"].as_str().unwrap()).collect();
        let field_refs: Vec<&str> = fields.iter().map(String::as_str).collect();
        assert_eq!(field_refs, declared, "{name}: local schema drift");
        observed.insert(name.to_owned(), fields.len());
    }
    for name in required {
        assert!(observed.contains_key(name), "{name}: not verified");
    }
}
