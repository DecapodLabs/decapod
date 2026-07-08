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

const ARCHITECTURE_NODES: &[&str] = &[
    "architecture/ALGORITHMS",
    "architecture/API_DESIGN",
    "architecture/AUTH",
    "architecture/CI_CD_PIPELINES",
    "architecture/CLOUD",
    "architecture/CODING_STANDARDS",
    "architecture/COMPLIANCE",
    "architecture/CONCURRENCY",
    "architecture/CONTAINERS",
    "architecture/COST_OPTIMIZATION",
    "architecture/DISTRIBUTED_SYSTEMS",
    "architecture/DR",
    "architecture/ENCRYPTION",
    "architecture/ENTERPRISE",
    "architecture/EVENT_DRIVEN",
    "architecture/FRONTEND",
    "architecture/GO",
    "architecture/GRAPHQL",
    "architecture/GRPC",
    "architecture/INFRASTRUCTURE",
    "architecture/JAVA",
    "architecture/JAVASCRIPT",
    "architecture/KNOWLEDGE_BASE",
    "architecture/KUBERNETES",
    "architecture/MEMORY",
    "architecture/MESSAGING",
    "architecture/METRICS",
    "architecture/MICROSERVICES",
    "architecture/NETWORKING",
    "architecture/OBSERVABILITY",
    "architecture/PERFORMANCE",
    "architecture/PYTHON",
    "architecture/REACT",
    "architecture/RUBY",
    "architecture/RUST",
    "architecture/SCALING",
    "architecture/SECRETS",
    "architecture/SECURITY",
    "architecture/SYSTEMS_DESIGN",
    "architecture/TERRAFORM",
    "architecture/TESTING_STRATEGY",
    "architecture/UI",
    "architecture/WEB",
];

const ARCHITECTURE_DOCTRINE_FIELDS: &[&str] = &[
    "architect_notes",
    "init_questions",
    "decision_matrix",
    "tradeoffs",
    "scaffolding",
    "spec_impacts",
    "proof",
];

const CORE_NODES: &[&str] = &[
    "core/ARCHITECTURE",
    "core/DATA",
    "core/DECAPOD",
    "core/DEMANDS",
    "core/DEPRECATION",
    "core/DOCS",
    "core/EMERGENCY_PROTOCOL",
    "core/ENGINEERING_EXCELLENCE",
    "core/GAPS",
    "core/INTERFACES",
    "core/METADATA",
    "core/METHODOLOGY",
    "core/PLUGINS",
    "core/RESEARCH",
    "core/SCAFFOLDING",
    "core/SPECS",
];

const STRATEGY_METHODOLOGY_NODES: &[&str] = &[
    "methodology/STRATEGIC_DECISION",
    "methodology/STRATEGY_DIAGNOSIS",
    "methodology/MARKET_INTELLIGENCE",
    "methodology/STRATEGY_ECONOMICS",
    "methodology/OPERATING_MODEL_EXECUTION",
    "methodology/VALUE_RISK_GOVERNANCE",
    "methodology/EXECUTIVE_ALIGNMENT",
];

const STRATEGY_CHILD_NODES: &[&str] = &[
    "methodology/STRATEGY_DIAGNOSIS",
    "methodology/MARKET_INTELLIGENCE",
    "methodology/STRATEGY_ECONOMICS",
    "methodology/OPERATING_MODEL_EXECUTION",
    "methodology/VALUE_RISK_GOVERNANCE",
    "methodology/EXECUTIVE_ALIGNMENT",
];

const STRATEGY_MODE_LOOKUPS: &[(&str, &str)] = &[
    ("situation assessment", "methodology/STRATEGY_DIAGNOSIS"),
    ("growth barriers", "methodology/STRATEGY_DIAGNOSIS"),
    ("assumption audit", "methodology/STRATEGY_DIAGNOSIS"),
    ("market mapping", "methodology/MARKET_INTELLIGENCE"),
    (
        "competitive intelligence",
        "methodology/MARKET_INTELLIGENCE",
    ),
    ("customer segmentation", "methodology/MARKET_INTELLIGENCE"),
    ("profit pool analysis", "methodology/MARKET_INTELLIGENCE"),
    ("strategic options", "methodology/STRATEGY_ECONOMICS"),
    ("pricing strategy", "methodology/STRATEGY_ECONOMICS"),
    ("business case", "methodology/STRATEGY_ECONOMICS"),
    ("portfolio review", "methodology/STRATEGY_ECONOMICS"),
    ("operating model", "methodology/OPERATING_MODEL_EXECUTION"),
    (
        "initiative prioritization",
        "methodology/OPERATING_MODEL_EXECUTION",
    ),
    (
        "transformation roadmap",
        "methodology/OPERATING_MODEL_EXECUTION",
    ),
    ("war gaming", "methodology/VALUE_RISK_GOVERNANCE"),
    ("risk mitigation", "methodology/VALUE_RISK_GOVERNANCE"),
    ("risk register", "methodology/VALUE_RISK_GOVERNANCE"),
    ("kpi architecture", "methodology/VALUE_RISK_GOVERNANCE"),
    ("value realization", "methodology/VALUE_RISK_GOVERNANCE"),
    ("stakeholder alignment", "methodology/EXECUTIVE_ALIGNMENT"),
    ("narrative builder", "methodology/EXECUTIVE_ALIGNMENT"),
    ("decision memo", "methodology/EXECUTIVE_ALIGNMENT"),
];

const STRATEGY_SEARCH_QUERIES: &[(&str, &str)] = &[
    (
        "growth barriers assumption audit root cause",
        "methodology/STRATEGY_DIAGNOSIS",
    ),
    (
        "market mapping customer segmentation profit pool",
        "methodology/MARKET_INTELLIGENCE",
    ),
    (
        "pricing strategy business case portfolio review",
        "methodology/STRATEGY_ECONOMICS",
    ),
    (
        "operating model initiative prioritization transformation roadmap",
        "methodology/OPERATING_MODEL_EXECUTION",
    ),
    (
        "war gaming risk register kpi architecture value realization",
        "methodology/VALUE_RISK_GOVERNANCE",
    ),
    (
        "stakeholder alignment narrative builder decision memo",
        "methodology/EXECUTIVE_ALIGNMENT",
    ),
];

fn load_constitution_asset() -> serde_json::Value {
    serde_json::from_str(include_str!("../assets/constitution.json")).expect("constitution json")
}

fn load_constitution_schema() -> serde_json::Value {
    serde_json::from_str(include_str!("../assets/constitution.schema.json"))
        .expect("constitution schema json")
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

fn assert_architecture_text_has_no_decapod_nuance(node_id: &str, value: &serde_json::Value) {
    fn visit(node_id: &str, path: &mut Vec<String>, value: &serde_json::Value) {
        match value {
            serde_json::Value::String(text) => {
                if path.first().is_some_and(|segment| segment == "links") {
                    return;
                }
                for forbidden in [
                    "Decapod",
                    "decapod",
                    "Issue #",
                    "GitHub Issue",
                    "constitution retrieval",
                    "generated spec",
                    "generated artifact",
                ] {
                    assert!(
                        !text.contains(forbidden),
                        "{node_id}.{} must not expose Decapod/process nuance: {text}",
                        path.join(".")
                    );
                }
            }
            serde_json::Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    path.push(index.to_string());
                    visit(node_id, path, item);
                    path.pop();
                }
            }
            serde_json::Value::Object(map) => {
                for (key, item) in map {
                    path.push(key.clone());
                    visit(node_id, path, item);
                    path.pop();
                }
            }
            _ => {}
        }
    }

    visit(node_id, &mut Vec::new(), value);
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
fn all_architecture_nodes_have_architect_grade_guidance() {
    let constitution = load_constitution_asset();
    let nodes = constitution["nodes"].as_object().expect("nodes object");

    let actual_architecture_count = nodes
        .keys()
        .filter(|id| id.starts_with("architecture/"))
        .count();
    assert_eq!(
        actual_architecture_count,
        ARCHITECTURE_NODES.len(),
        "ARCHITECTURE_NODES test list must cover every architecture/* node"
    );

    for node_id in ARCHITECTURE_NODES {
        let node = nodes
            .get(*node_id)
            .unwrap_or_else(|| panic!("missing architecture node {node_id}"));
        assert_architect_grade_fields(node_id, node);
        assert_architecture_text_has_no_decapod_nuance(node_id, node);

        let sections = node["sections"].as_object().expect("sections object");
        for section in ["match", "ambiguity", "failure_modes", "proceed_when"] {
            assert!(
                sections
                    .get(section)
                    .and_then(|items| items.as_array())
                    .is_some_and(|items| !items.is_empty()),
                "{node_id}.{section} must preserve retrieval, stop, risk, and proceed guidance"
            );
        }
    }
}

#[test]
fn schema_requires_doctrine_model_for_architecture_nodes_only() {
    let schema = load_constitution_schema();

    assert_eq!(
        schema["x-doctrine_model"]["schema_version"].as_str(),
        Some("architecture-doctrine-v1"),
        "schema must document the architecture doctrine model version"
    );
    assert!(
        schema["x-doctrine_model"]["migration_strategy"]
            .as_str()
            .is_some_and(|strategy| strategy.contains("migrated in place")),
        "schema must document in-place architecture node migration"
    );

    let node_schema = &schema["definitions"]["node"];
    let architecture_rule = node_schema["allOf"]
        .as_array()
        .and_then(|rules| rules.first())
        .expect("node schema must declare architecture doctrine conditional");
    assert_eq!(
        architecture_rule["if"]["properties"]["category"]["const"].as_str(),
        Some("architecture"),
        "conditional must target architecture nodes"
    );

    let required = architecture_rule["then"]["required"]
        .as_array()
        .expect("architecture doctrine required fields");
    for field in ARCHITECTURE_DOCTRINE_FIELDS {
        assert!(
            required.iter().any(|value| value.as_str() == Some(field)),
            "architecture schema must require {field}"
        );
    }

    let base_required = node_schema["required"]
        .as_array()
        .expect("base node required fields");
    assert!(
        !ARCHITECTURE_DOCTRINE_FIELDS
            .iter()
            .any(|field| base_required
                .iter()
                .any(|value| value.as_str() == Some(field))),
        "non-architecture nodes must keep the compact node contract"
    );
}

#[test]
fn all_architecture_nodes_remain_discoverable_without_schema_migration() {
    let constitution = load_constitution_asset();
    let nodes = constitution["nodes"].as_object().expect("nodes object");
    let lookup = constitution["lookup"].as_object().expect("lookup object");

    for node_id in ARCHITECTURE_NODES {
        let node = nodes
            .get(*node_id)
            .unwrap_or_else(|| panic!("missing architecture node {node_id}"));

        assert_non_empty_array(
            &node["links"]["references"],
            &format!("{node_id}.links.references"),
        );

        assert!(
            lookup.values().any(|entries| {
                entries.as_array().is_some_and(|entries| {
                    entries.iter().any(|entry| entry.as_str() == Some(node_id))
                })
            }),
            "{node_id} must remain reachable through at least one lookup term"
        );
    }
}

#[test]
fn all_core_nodes_have_architect_grade_routing_guidance() {
    let constitution = load_constitution_asset();
    let nodes = constitution["nodes"].as_object().expect("nodes object");

    let actual_core_count = nodes.keys().filter(|id| id.starts_with("core/")).count();
    assert_eq!(
        actual_core_count,
        CORE_NODES.len(),
        "CORE_NODES test list must cover every core/* node"
    );

    for node_id in CORE_NODES {
        let node = nodes
            .get(*node_id)
            .unwrap_or_else(|| panic!("missing core node {node_id}"));
        assert_architect_grade_fields(node_id, node);

        let sections = node["sections"].as_object().expect("sections object");
        for section in ["loop_guards", "proof_planning"] {
            assert!(
                sections
                    .get(section)
                    .and_then(|items| items.as_array())
                    .is_some_and(|items| items.len() >= 3),
                "{node_id}.{section} must prevent routing/proof-planning loops"
            );
        }
    }
}

#[test]
fn core_lookup_routes_loop_reports_and_untrusted_attachments() {
    let constitution = load_constitution_asset();
    let lookup = constitution["lookup"].as_object().expect("lookup object");

    let expected = [
        ("routing loop", "core/GAPS"),
        ("proof planning", "core/SPECS"),
        ("untrusted attachment", "core/EMERGENCY_PROTOCOL"),
        ("malicious attachment", "core/EMERGENCY_PROTOCOL"),
        ("unsafe patch", "core/EMERGENCY_PROTOCOL"),
        ("core routing", "core/DECAPOD"),
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
fn constitution_routes_strategy_decision_nudges_through_methodology() {
    let constitution = load_constitution_asset();
    let nodes = constitution["nodes"].as_object().expect("nodes object");
    let lookup = constitution["lookup"].as_object().expect("lookup object");
    let parent_id = "methodology/STRATEGIC_DECISION";

    for node_id in STRATEGY_METHODOLOGY_NODES {
        let node = nodes
            .get(*node_id)
            .unwrap_or_else(|| panic!("missing methodology node {node_id}"));
        assert_eq!(node["category"].as_str(), Some("methodology"));

        for section in [
            "match",
            "concepts",
            "ambiguity",
            "decisions",
            "standards",
            "failure_modes",
            "next_queries",
            "proceed_when",
        ] {
            assert_non_empty_array(&node["sections"][section], &format!("{node_id}.{section}"));
        }
    }

    let core_refs = nodes["core/METHODOLOGY"]["links"]["references"]
        .as_array()
        .expect("core methodology references");
    assert!(
        core_refs
            .iter()
            .any(|value| value.as_str() == Some(parent_id)),
        "core/METHODOLOGY must route to {parent_id}"
    );

    let product_refs = nodes["methodology/PRODUCT"]["links"]["references"]
        .as_array()
        .expect("product methodology references");
    assert!(
        product_refs
            .iter()
            .any(|value| value.as_str() == Some(parent_id)),
        "methodology/PRODUCT must route to {parent_id}"
    );

    let parent_refs = nodes[parent_id]["links"]["references"]
        .as_array()
        .expect("strategy decision references");
    for child_id in STRATEGY_CHILD_NODES {
        assert!(
            parent_refs
                .iter()
                .any(|value| value.as_str() == Some(child_id)),
            "{parent_id} must route to {child_id}"
        );
    }

    let parent_standards = nodes[parent_id]["sections"]["standards"]
        .as_array()
        .expect("standards array")
        .iter()
        .filter_map(|value| value.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    for expected in [
        "decision statement",
        "options considered",
        "load-bearing assumptions",
        "material risks",
        "success metrics",
        "next evidence tests",
    ] {
        assert!(
            parent_standards.contains(expected),
            "{parent_id}.standards must nudge agents toward {expected}"
        );
    }

    for (term, node_id) in STRATEGY_MODE_LOOKUPS {
        let entries = lookup
            .get(*term)
            .unwrap_or_else(|| panic!("lookup must include {term}"))
            .as_array()
            .unwrap_or_else(|| panic!("lookup.{term} must be an array"));
        assert!(
            entries.iter().any(|value| value.as_str() == Some(node_id)),
            "lookup.{term} must route to {node_id}"
        );
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
fn embedded_constitution_search_surfaces_strategy_decision_methodology() {
    for (query, node_id) in STRATEGY_SEARCH_QUERIES {
        let output = Command::new(resolve_decapod_bin())
            .args(["constitution", "search", "--query", query])
            .output()
            .expect("run decapod constitution search");
        assert!(
            output.status.success(),
            "constitution search failed for {query}: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains(node_id),
            "constitution search must surface {node_id} for {query}:\n{stdout}"
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

#[test]
fn embedded_constitution_preserves_core_architect_grade_fields() {
    for node_id in ["core/DECAPOD", "core/GAPS", "core/EMERGENCY_PROTOCOL"] {
        let output = Command::new(resolve_decapod_bin())
            .args(["constitution", "get", node_id])
            .output()
            .expect("run decapod constitution get");
        assert!(
            output.status.success(),
            "constitution get {node_id} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let node: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("constitution get json");
        assert_architect_grade_fields(node_id, &node);
        assert!(
            node["sections"]["loop_guards"].as_array().is_some(),
            "{node_id} must preserve loop guards in embedded output"
        );
    }
}

#[test]
fn embedded_constitution_preserves_architecture_architect_grade_fields() {
    for node_id in [
        "architecture/API_DESIGN",
        "architecture/MEMORY",
        "architecture/SECURITY",
    ] {
        let output = Command::new(resolve_decapod_bin())
            .args(["constitution", "get", node_id])
            .output()
            .expect("run decapod constitution get");
        assert!(
            output.status.success(),
            "constitution get {node_id} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let node: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("constitution get json");
        assert_architect_grade_fields(node_id, &node);
        assert!(
            node["decision_matrix"].as_array().is_some_and(|items| {
                items.iter().any(|item| {
                    item["choice"]
                        .as_str()
                        .is_some_and(|choice| choice.contains("doctrine"))
                })
            }),
            "{node_id} must preserve doctrine decision guidance in embedded output"
        );
    }
}
