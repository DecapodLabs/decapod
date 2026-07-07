use std::process::Command;

const RUNTIME_NODES: &[&str] = &[
    "architecture/RUST",
    "architecture/GO",
    "architecture/JAVA",
    "architecture/PYTHON",
    "architecture/RUBY",
    "architecture/JAVASCRIPT",
    "architecture/REACT",
    "architecture/TERRAFORM",
    "data/POSTGRESQL",
];

fn load_constitution_asset() -> serde_json::Value {
    serde_json::from_str(include_str!("../assets/constitution.json")).expect("constitution json")
}

fn resolve_decapod_bin() -> std::path::PathBuf {
    let cargo_bin = env!("CARGO_BIN_EXE_decapod");
    if let Ok(p) = std::path::Path::new(cargo_bin).canonicalize() {
        return p;
    }
    std::path::PathBuf::from(cargo_bin)
}

fn assert_non_empty_array(value: &serde_json::Value, path: &str) {
    let array = value
        .as_array()
        .unwrap_or_else(|| panic!("{path} must be an array"));
    assert!(!array.is_empty(), "{path} must not be empty");
}

fn assert_architect_grade_fields(node_id: &str, node: &serde_json::Value) {
    assert_non_empty_array(
        &node["architect_notes"],
        &format!("{node_id}.architect_notes"),
    );
    assert!(
        node["init_questions"]
            .as_array()
            .is_some_and(|v| v.len() >= 2),
        "{node_id}.init_questions must contain init discovery prompts"
    );
    assert!(
        node["decision_matrix"]
            .as_array()
            .is_some_and(|v| v.len() >= 3),
        "{node_id}.decision_matrix must contain decision guidance"
    );
    assert!(
        node["tradeoffs"].as_array().is_some_and(|v| v.len() >= 2),
        "{node_id}.tradeoffs must contain tradeoff guidance"
    );

    for field in ["capture", "generate", "defer", "proof"] {
        assert_non_empty_array(
            &node["scaffolding"][field],
            &format!("{node_id}.scaffolding.{field}"),
        );
    }

    for field in ["intent", "architecture", "interfaces", "proof"] {
        assert_non_empty_array(
            &node["spec_impacts"][field],
            &format!("{node_id}.spec_impacts.{field}"),
        );
    }

    for field in ["static", "runtime", "release"] {
        assert_non_empty_array(&node["proof"][field], &format!("{node_id}.proof.{field}"));
    }
}

#[test]
fn constitution_contains_scaffolding_router_and_runtime_nodes() {
    let constitution = load_constitution_asset();
    let nodes = constitution["nodes"].as_object().expect("nodes object");

    assert!(
        nodes.contains_key("core/SCAFFOLDING"),
        "constitution must include the project scaffolding router"
    );

    let scaffolding_refs = nodes["core/SCAFFOLDING"]["links"]["references"]
        .as_array()
        .expect("scaffolding references");
    for node_id in RUNTIME_NODES {
        assert!(
            nodes.contains_key(*node_id),
            "constitution must include {node_id}"
        );
        assert!(
            scaffolding_refs.iter().any(|v| v.as_str() == Some(node_id)),
            "core/SCAFFOLDING must route to {node_id}"
        );
    }

    let architecture_refs = nodes["core/ARCHITECTURE"]["links"]["references"]
        .as_array()
        .expect("architecture references");
    for node_id in RUNTIME_NODES {
        assert!(
            architecture_refs
                .iter()
                .any(|v| v.as_str() == Some(node_id)),
            "core/ARCHITECTURE must route to {node_id}"
        );
    }
}

#[test]
fn scaffolding_nodes_have_architect_grade_guidance() {
    let constitution = load_constitution_asset();
    let nodes = constitution["nodes"].as_object().expect("nodes object");

    assert_architect_grade_fields("core/SCAFFOLDING", &nodes["core/SCAFFOLDING"]);
    for node_id in RUNTIME_NODES {
        assert_architect_grade_fields(node_id, &nodes[*node_id]);
    }
}

#[test]
fn lookup_routes_init_and_major_runtime_terms() {
    let constitution = load_constitution_asset();
    let lookup = constitution["lookup"].as_object().expect("lookup object");

    let expected = [
        ("scaffolding", "core/SCAFFOLDING"),
        ("init", "core/SCAFFOLDING"),
        ("rust", "architecture/RUST"),
        ("go", "architecture/GO"),
        ("java", "architecture/JAVA"),
        ("python", "architecture/PYTHON"),
        ("ruby", "architecture/RUBY"),
        ("javascript", "architecture/JAVASCRIPT"),
        ("typescript", "architecture/JAVASCRIPT"),
        ("react", "architecture/REACT"),
        ("postgres", "data/POSTGRESQL"),
        ("terraform", "architecture/TERRAFORM"),
    ];

    for (term, node_id) in expected {
        let entries = lookup
            .get(term)
            .unwrap_or_else(|| panic!("lookup must include {term}"))
            .as_array()
            .unwrap_or_else(|| panic!("lookup.{term} must be an array"));
        assert!(
            entries.iter().any(|v| v.as_str() == Some(node_id)),
            "lookup.{term} must route to {node_id}"
        );
    }
}

#[test]
fn embedded_constitution_preserves_architect_grade_fields() {
    let output = Command::new(resolve_decapod_bin())
        .args(["constitution", "get", "core/SCAFFOLDING"])
        .output()
        .expect("run decapod constitution get");
    assert!(
        output.status.success(),
        "constitution get failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let node: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("constitution get json");
    assert_architect_grade_fields("core/SCAFFOLDING", &node);
}
