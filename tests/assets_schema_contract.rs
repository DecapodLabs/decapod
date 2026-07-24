use serde_json::Value;
use std::fs;
use std::path::Path;

fn read_json(path: &str) -> Value {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let bytes = fs::read(root.join(path)).unwrap_or_else(|error| panic!("read {path}: {error}"));
    serde_json::from_slice(&bytes).unwrap_or_else(|error| panic!("parse {path}: {error}"))
}

fn required(schema: &Value) -> &Vec<Value> {
    schema
        .get("required")
        .and_then(Value::as_array)
        .expect("schema required list")
}

fn has_required(schema: &Value, name: &str) -> bool {
    required(schema).iter().any(|value| value == name)
}

#[test]
fn governed_schemas_are_grouped_and_strict() {
    let schemas = [
        (
            "assets/schemas/claims.schema.json",
            "https://decapod.dev/schemas/research-claims-ledger-1.0.0.schema.json",
            &["schema_version", "claims"][..],
        ),
        (
            "assets/schemas/plan.schema.json",
            "https://decapod.dev/schemas/plan-1.0.0.schema.json",
            &["schema_version", "state", "phases"][..],
        ),
        (
            "assets/schemas/trajectory.schema.json",
            "https://decapod.dev/schemas/trajectory-1.1.0.schema.json",
            &["schema_version", "checks", "proof_status"][..],
        ),
        (
            "assets/schemas/validation.schema.json",
            "https://decapod.dev/schemas/validation-receipt-1.0.0.schema.json",
            &["schema_version", "kind", "validation_epoch"][..],
        ),
        (
            "assets/schemas/constitution.schema.json",
            "",
            &["$schema"][..],
        ),
    ];

    for (path, id, required_fields) in schemas {
        let schema = read_json(path);
        assert_eq!(schema["type"], "object", "{path} root type");
        assert_eq!(schema["additionalProperties"], false, "{path} is strict");
        if !id.is_empty() {
            assert_eq!(schema["$id"], id, "{path} identifier");
        }
        for field in required_fields {
            assert!(has_required(&schema, field), "{path} requires {field}");
        }
    }
}

#[test]
fn plan_and_trajectory_enums_cover_governance_states() {
    let plan = read_json("assets/schemas/plan.schema.json");
    let states = plan["properties"]["state"]["enum"]
        .as_array()
        .expect("plan states");
    for state in ["DRAFT", "ANNOTATING", "APPROVED", "EXECUTING", "DONE"] {
        assert!(
            states.iter().any(|value| value == state),
            "missing plan state {state}"
        );
    }

    let trajectory = read_json("assets/schemas/trajectory.schema.json");
    let proof_status = trajectory["properties"]["proof_status"]["enum"]
        .as_array()
        .expect("trajectory proof status");
    for status in [
        "passed",
        "failed",
        "partial",
        "unavailable",
        "no_checks_run",
    ] {
        assert!(
            proof_status.iter().any(|value| value == status),
            "missing trajectory proof status {status}"
        );
    }
}

#[test]
fn relocatable_support_files_have_one_asset_home() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for path in [
        "assets/Dockerfile.workspace",
        "assets/build/compress_constitution.rs",
        "assets/build/constitution_index.rs",
        "assets/benches/perf_1_1.rs",
        "assets/benches/perf_1_2.rs",
        "assets/benches/perf_1_3.rs",
    ] {
        assert!(
            root.join(path).is_file(),
            "missing consolidated asset {path}"
        );
    }
    for path in [
        "Dockerfile.workspace",
        "build/compress_constitution.rs",
        "project/benches/perf_1_1.rs",
    ] {
        assert!(
            !root.join(path).exists(),
            "stale support path remains: {path}"
        );
    }
}
