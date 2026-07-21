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

const DOCS_NODES: &[&str] = &[
    "docs/ARCHITECTURE_OVERVIEW",
    "docs/CONTROL_PLANE_API",
    "docs/EVAL_TRANSLATION_MAP",
    "docs/GOVERNANCE_AUDIT",
    "docs/MAINTAINERS",
    "docs/MIGRATIONS",
    "docs/NEGLECTED_ASPECTS_LEDGER",
    "docs/PLAYBOOK",
    "docs/README",
    "docs/RELEASE_PROCESS",
    "docs/SECURITY_THREAT_MODEL",
    "docs/SKILL_TRANSLATION_MAP",
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

const INTERFACE_NODES: &[&str] = &[
    "interfaces/AGENT_CONTEXT_PACK",
    "interfaces/ARCHITECTURE_FOUNDATIONS",
    "interfaces/CLAIMS",
    "interfaces/CONTROL_PLANE",
    "interfaces/DEMANDS_SCHEMA",
    "interfaces/DOC_RULES",
    "interfaces/GLOSSARY",
    "interfaces/INTERNALIZATION_SCHEMA",
    "interfaces/KNOWLEDGE_SCHEMA",
    "interfaces/KNOWLEDGE_STORE",
    "interfaces/LCM",
    "interfaces/MEMORY_INDEX",
    "interfaces/MEMORY_SCHEMA",
    "interfaces/PLAN_GOVERNED_EXECUTION",
    "interfaces/PROCEDURAL_NORMS",
    "interfaces/PROJECT_SPECS",
    "interfaces/RISK_POLICY_GATE",
    "interfaces/STORE_MODEL",
    "interfaces/TESTING",
    "interfaces/TODO_SCHEMA",
    "interfaces/jsonschema/internalization/InternalizationAttachResult.schema",
    "interfaces/jsonschema/internalization/InternalizationCreateResult.schema",
    "interfaces/jsonschema/internalization/InternalizationDetachResult.schema",
    "interfaces/jsonschema/internalization/InternalizationInspectResult.schema",
    "interfaces/jsonschema/internalization/InternalizationManifest.schema",
];

const DATA_NODES: &[&str] = &[
    "data/CACHING",
    "data/DATABASE",
    "data/DATA_ENGINEERING",
    "data/PIPELINES",
    "data/POSTGRESQL",
];

const METADATA_NODES: &[&str] = &[
    "metadata/skills/AGENT_DECAPOD_INTERFACE",
    "metadata/skills/BUNDLE",
    "metadata/skills/HUMAN_AGENT_UX",
    "metadata/skills/INTENT_REFINEMENT",
];

const PLUGIN_NODES: &[&str] = &[
    "plugins/APTITUDE",
    "plugins/ARCHIVE",
    "plugins/AUDIT",
    "plugins/AUTOUPDATE",
    "plugins/CONTAINER",
    "plugins/CONTEXT",
    "plugins/CRON",
    "plugins/DB_BROKER",
    "plugins/DECIDE",
    "plugins/EMERGENCY_PROTOCOL",
    "plugins/FEDERATION",
    "plugins/FEEDBACK",
    "plugins/HEALTH",
    "plugins/HEARTBEAT",
    "plugins/KNOWLEDGE",
    "plugins/MANIFEST",
    "plugins/POLICY",
    "plugins/REFLEX",
    "plugins/TODO",
    "plugins/TRUST",
    "plugins/VERIFY",
    "plugins/WATCHER",
];

const SPEC_NODES: &[&str] = &[
    "specs/AMENDMENTS",
    "specs/DB_BROKER_QUEUE",
    "specs/GIT",
    "specs/INTENT",
    "specs/SECURITY",
    "specs/SYSTEM",
    "specs/engineering/FRONTEND_BACKEND_E2E",
    "specs/evaluations/JUDGE_CONTRACT",
    "specs/evaluations/VARIANCE_EVALS",
    "specs/skills/SKILL_GOVERNANCE",
];

const METHODOLOGY_NODES: &[&str] = &[
    "methodology/ARCHITECTURE",
    "methodology/CI_CD",
    "methodology/ENGINEERING_MANAGEMENT",
    "methodology/INCIDENT_RESPONSE",
    "methodology/KNOWLEDGE",
    "methodology/MARKET_INTELLIGENCE",
    "methodology/MEMORY",
    "methodology/METRICS",
    "methodology/OPERATING_MODEL_EXECUTION",
    "methodology/OPERATIONS",
    "methodology/PLATFORM",
    "methodology/PRODUCT",
    "methodology/QA",
    "methodology/RELEASE_MANAGEMENT",
    "methodology/RESEARCH",
    "methodology/RESEARCH_PRODUCTION",
    "methodology/SOUL",
    "methodology/STRATEGIC_DECISION",
    "methodology/STRATEGY_DIAGNOSIS",
    "methodology/STRATEGY_ECONOMICS",
    "methodology/TESTING",
    "methodology/VALUE_RISK_GOVERNANCE",
    "methodology/EXECUTIVE_ALIGNMENT",
];

const METHODOLOGY_DOCTRINE_FIELDS: &[&str] = ARCHITECTURE_DOCTRINE_FIELDS;

const METHODOLOGY_SPEC_IMPACT_FIELDS: &[&str] = &[
    "intent",
    "architecture",
    "interfaces",
    "operations",
    "security",
    "validation",
    "semantics",
    "proof",
];

const RESEARCH_NODES: &[&str] = &[
    "research/ADAM_2014",
    "research/ALEXNET_2012",
    "research/ALPHAFOLD_2021",
    "research/ALPHAGO_2016",
    "research/BACKPROP_1986",
    "research/BATCH_NORM_2015",
    "research/BERT_2018",
    "research/BIGTABLE_2006",
    "research/BITCOIN_2008",
    "research/BLOOM_FILTER_1970",
    "research/BORG_2015",
    "research/BROOKS_1986",
    "research/BTREE_1972",
    "research/CHORD_2001",
    "research/CLIP_2021",
    "research/CODD_1970",
    "research/CONTAINERS_2007",
    "research/COOK_1971",
    "research/COT_2022",
    "research/DIFFIE_HELLMAN_1976",
    "research/DIFFUSION_2020",
    "research/DIJKSTRA_1959",
    "research/DQN_2013",
    "research/DREMEL_2010",
    "research/DROPOUT_2014",
    "research/DYNAMO_2007",
    "research/EBPF_1993",
    "research/END_TO_END_1984",
    "research/FLP_1985",
    "research/GAN_2014",
    "research/GFS_2003",
    "research/GNN_2008",
    "research/GPT3_2020",
    "research/GRAY_1981",
    "research/HOARE_1969",
    "research/HYPERLOGLOG_2007",
    "research/KAFKA_2011",
    "research/L4_1995",
    "research/LAMPORT_1978",
    "research/LAMPSON_1983",
    "research/LECUN_1998",
    "research/LFS_1992",
    "research/LISKOV_1974",
    "research/LLAMA_2023",
    "research/LORA_2021",
    "research/LSM_TREE_1996",
    "research/LSTM_1997",
    "research/MAPREDUCE_2004",
    "research/MCCULLOCH_PITTS_1943",
    "research/MESI_1984",
    "research/MILLWHEEL_2013",
    "research/MINSKY_1961",
    "research/PAXOS_1998",
    "research/PERCEPTRONS_1958",
    "research/RAFT_2014",
    "research/RAID_1988",
    "research/RDMA_2003",
    "research/REDBLACK_TREE_1978",
    "research/RESNET_2015",
    "research/RLHF_2022",
    "research/RSA_1978",
    "research/SALTZER_SCHROEDER_1975",
    "research/SHANNON_1948",
    "research/SPANNER_2012",
    "research/SPARK_2012",
    "research/SPECTRE_MELTDOWN_2018",
    "research/SVM_1995",
    "research/TENSORFLOW_2016",
    "research/THOMPSON_1984",
    "research/TRANSFORMER_2017",
    "research/TURING_1936",
    "research/UNIX_1974",
    "research/VIRTUAL_MEMORY_1962",
    "research/VIT_2021",
    "research/VON_NEUMANN_1945",
    "research/WAIT_FREE_1991",
    "research/WORD2VEC_2013",
    "research/YOLO_2016",
    "research/ZFS_2003",
    "research/ZOOKEEPER_2010",
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

fn assert_methodology_delivery_fields(node_id: &str, node: &serde_json::Value) {
    assert_architect_grade_fields(node_id, node);

    for field in METHODOLOGY_SPEC_IMPACT_FIELDS {
        assert_non_empty_array(
            &node["spec_impacts"][*field],
            &format!("{node_id}.spec_impacts.{field}"),
        );
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
fn all_docs_nodes_have_architect_grade_documentation_doctrine() {
    let constitution = load_constitution_asset();
    let nodes = constitution["nodes"].as_object().expect("nodes object");
    let lookup = constitution["lookup"].as_object().expect("lookup object");

    let actual_docs_count = nodes
        .values()
        .filter(|node| node["category"].as_str() == Some("docs"))
        .count();
    assert_eq!(
        actual_docs_count,
        DOCS_NODES.len(),
        "DOCS_NODES test list must cover every docs/* node"
    );

    for node_id in DOCS_NODES {
        let node = nodes
            .get(*node_id)
            .unwrap_or_else(|| panic!("missing docs node {node_id}"));
        assert_eq!(
            node["category"].as_str(),
            Some("docs"),
            "{node_id} must remain in the docs namespace"
        );
        assert_architect_grade_fields(node_id, node);
        assert_non_empty_array(
            &node["links"]["references"],
            &format!("{node_id}.links.references"),
        );
        assert!(
            node["links"]["referenced_by"]
                .as_array()
                .is_some_and(|entries| entries
                    .iter()
                    .any(|entry| entry.as_str() == Some("core/DOCS"))),
            "{node_id} must be routed from core/DOCS"
        );

        let sections = node["sections"].as_object().expect("sections object");
        for section in [
            "match",
            "ambiguity",
            "decisions",
            "standards",
            "failure_modes",
            "proceed_when",
        ] {
            assert!(
                sections
                    .get(section)
                    .and_then(|items| items.as_array())
                    .is_some_and(|items| !items.is_empty()),
                "{node_id}.{section} must preserve documentation doctrine"
            );
        }

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
fn all_interface_nodes_have_architect_grade_contract_doctrine() {
    let constitution = load_constitution_asset();
    let nodes = constitution["nodes"].as_object().expect("nodes object");
    let lookup = constitution["lookup"].as_object().expect("lookup object");

    let actual_interface_count = nodes
        .values()
        .filter(|node| node["category"].as_str() == Some("interfaces"))
        .count();
    assert_eq!(
        actual_interface_count,
        INTERFACE_NODES.len(),
        "INTERFACE_NODES test list must cover every interfaces/* node"
    );

    for node_id in INTERFACE_NODES {
        let node = nodes
            .get(*node_id)
            .unwrap_or_else(|| panic!("missing interface node {node_id}"));
        assert_eq!(
            node["category"].as_str(),
            Some("interfaces"),
            "{node_id} must remain in the interfaces namespace"
        );
        assert_architect_grade_fields(node_id, node);
        assert_non_empty_array(
            &node["links"]["references"],
            &format!("{node_id}.links.references"),
        );
        let sections = node["sections"].as_object().expect("sections object");
        for section in [
            "match",
            "ambiguity",
            "decisions",
            "standards",
            "failure_modes",
            "proceed_when",
        ] {
            assert!(
                sections
                    .get(section)
                    .and_then(|items| items.as_array())
                    .is_some_and(|items| !items.is_empty()),
                "{node_id}.{section} must preserve contract guidance"
            );
        }

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
fn all_data_nodes_have_architect_grade_persistence_and_pipeline_doctrine() {
    let constitution = load_constitution_asset();
    let nodes = constitution["nodes"].as_object().expect("nodes object");
    let lookup = constitution["lookup"].as_object().expect("lookup object");

    let actual_data_count = nodes
        .values()
        .filter(|node| node["category"].as_str() == Some("data"))
        .count();
    assert_eq!(
        actual_data_count,
        DATA_NODES.len(),
        "DATA_NODES test list must cover every data/* node"
    );

    let core_refs = nodes["core/DATA"]["links"]["references"]
        .as_array()
        .expect("core data references");

    for node_id in DATA_NODES {
        let node = nodes
            .get(*node_id)
            .unwrap_or_else(|| panic!("missing data node {node_id}"));
        assert_eq!(
            node["category"].as_str(),
            Some("data"),
            "{node_id} must remain in the data namespace"
        );
        assert_architect_grade_fields(node_id, node);
        assert_non_empty_array(
            &node["links"]["references"],
            &format!("{node_id}.links.references"),
        );
        assert!(
            node["links"]["referenced_by"]
                .as_array()
                .is_some_and(|refs| refs.iter().any(|value| value.as_str() == Some("core/DATA"))),
            "{node_id} must be reachable from core/DATA"
        );
        assert!(
            core_refs
                .iter()
                .any(|value| value.as_str() == Some(node_id)),
            "core/DATA must route to {node_id}"
        );

        let sections = node["sections"].as_object().expect("sections object");
        for section in [
            "match",
            "ambiguity",
            "decisions",
            "standards",
            "failure_modes",
            "proceed_when",
        ] {
            assert!(
                sections
                    .get(section)
                    .and_then(|items| items.as_array())
                    .is_some_and(|items| !items.is_empty()),
                "{node_id}.{section} must preserve data guidance"
            );
        }

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
fn all_metadata_nodes_have_architect_grade_skill_and_ux_doctrine() {
    let constitution = load_constitution_asset();
    let nodes = constitution["nodes"].as_object().expect("nodes object");
    let lookup = constitution["lookup"].as_object().expect("lookup object");

    let actual_metadata_count = nodes
        .values()
        .filter(|node| node["category"].as_str() == Some("metadata"))
        .count();
    assert_eq!(
        actual_metadata_count,
        METADATA_NODES.len(),
        "METADATA_NODES test list must cover every metadata/* node"
    );

    let core_refs = nodes["core/METADATA"]["links"]["references"]
        .as_array()
        .expect("core metadata references");

    for node_id in METADATA_NODES {
        let node = nodes
            .get(*node_id)
            .unwrap_or_else(|| panic!("missing metadata node {node_id}"));
        assert_eq!(
            node["category"].as_str(),
            Some("metadata"),
            "{node_id} must remain in the metadata namespace"
        );
        assert_architect_grade_fields(node_id, node);
        assert_non_empty_array(
            &node["links"]["references"],
            &format!("{node_id}.links.references"),
        );
        assert!(
            node["links"]["referenced_by"]
                .as_array()
                .is_some_and(|refs| refs
                    .iter()
                    .any(|value| value.as_str() == Some("core/METADATA"))),
            "{node_id} must be reachable from core/METADATA"
        );
        assert!(
            core_refs
                .iter()
                .any(|value| value.as_str() == Some(node_id)),
            "core/METADATA must route to {node_id}"
        );

        let sections = node["sections"].as_object().expect("sections object");
        for section in [
            "match",
            "ambiguity",
            "decisions",
            "standards",
            "failure_modes",
            "proceed_when",
        ] {
            assert!(
                sections
                    .get(section)
                    .and_then(|items| items.as_array())
                    .is_some_and(|items| !items.is_empty()),
                "{node_id}.{section} must preserve metadata guidance"
            );
        }

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
fn all_plugin_nodes_have_architect_grade_subsystem_doctrine() {
    let constitution = load_constitution_asset();
    let nodes = constitution["nodes"].as_object().expect("nodes object");
    let lookup = constitution["lookup"].as_object().expect("lookup object");

    let actual_plugin_count = nodes
        .values()
        .filter(|node| node["category"].as_str() == Some("plugins"))
        .count();
    assert_eq!(
        actual_plugin_count,
        PLUGIN_NODES.len(),
        "PLUGIN_NODES test list must cover every plugins/* node"
    );

    let core_refs = nodes["core/PLUGINS"]["links"]["references"]
        .as_array()
        .expect("core plugin references");

    for node_id in PLUGIN_NODES {
        let node = nodes
            .get(*node_id)
            .unwrap_or_else(|| panic!("missing plugin node {node_id}"));
        assert_eq!(
            node["category"].as_str(),
            Some("plugins"),
            "{node_id} must remain in the plugins namespace"
        );
        assert_architect_grade_fields(node_id, node);
        assert_non_empty_array(
            &node["links"]["references"],
            &format!("{node_id}.links.references"),
        );
        assert!(
            node["links"]["references"]
                .as_array()
                .is_some_and(|refs| refs
                    .iter()
                    .any(|value| value.as_str() == Some("interfaces/CONTROL_PLANE"))),
            "{node_id} must link plugin metadata to the control-plane contract"
        );
        assert!(
            node["links"]["referenced_by"]
                .as_array()
                .is_some_and(|refs| refs
                    .iter()
                    .any(|value| value.as_str() == Some("core/PLUGINS"))),
            "{node_id} must be reachable from core/PLUGINS"
        );
        assert!(
            core_refs
                .iter()
                .any(|value| value.as_str() == Some(node_id)),
            "core/PLUGINS must route to {node_id}"
        );

        let sections = node["sections"].as_object().expect("sections object");
        for section in [
            "match",
            "ambiguity",
            "decisions",
            "standards",
            "failure_modes",
            "proceed_when",
        ] {
            assert!(
                sections
                    .get(section)
                    .and_then(|items| items.as_array())
                    .is_some_and(|items| !items.is_empty()),
                "{node_id}.{section} must preserve subsystem guidance"
            );
        }

        assert!(
            node["architect_notes"].as_array().is_some_and(|notes| {
                notes.iter().any(|note| {
                    note.as_str().is_some_and(|text| {
                        text.contains(node_id.strip_prefix("plugins/").unwrap())
                    })
                })
            }),
            "{node_id} architect notes must identify the owned plugin"
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
fn all_spec_nodes_have_architect_grade_specification_doctrine() {
    let constitution = load_constitution_asset();
    let nodes = constitution["nodes"].as_object().expect("nodes object");
    let lookup = constitution["lookup"].as_object().expect("lookup object");

    let actual_spec_count = nodes
        .values()
        .filter(|node| node["category"].as_str() == Some("specs"))
        .count();
    assert_eq!(
        actual_spec_count,
        SPEC_NODES.len(),
        "SPEC_NODES test list must cover every specs/* node"
    );

    let core_refs = nodes["core/SPECS"]["links"]["references"]
        .as_array()
        .expect("core specs references");

    for node_id in SPEC_NODES {
        let node = nodes
            .get(*node_id)
            .unwrap_or_else(|| panic!("missing spec node {node_id}"));
        assert_eq!(
            node["category"].as_str(),
            Some("specs"),
            "{node_id} must remain in the specs namespace"
        );
        assert_architect_grade_fields(node_id, node);
        assert_non_empty_array(
            &node["links"]["references"],
            &format!("{node_id}.links.references"),
        );
        assert!(
            node["links"]["references"]
                .as_array()
                .is_some_and(|refs| refs
                    .iter()
                    .any(|value| value.as_str() == Some("core/SPECS"))),
            "{node_id} must link to core/SPECS"
        );
        assert!(
            node["links"]["references"].as_array().is_some_and(|refs| {
                refs.iter()
                    .any(|value| value.as_str() == Some("interfaces/PROJECT_SPECS"))
            }),
            "{node_id} must link to the project-spec contract"
        );
        assert!(
            node["links"]["referenced_by"]
                .as_array()
                .is_some_and(|refs| refs
                    .iter()
                    .any(|value| value.as_str() == Some("core/SPECS"))),
            "{node_id} must be reachable from core/SPECS"
        );
        assert!(
            core_refs
                .iter()
                .any(|value| value.as_str() == Some(node_id)),
            "core/SPECS must route to {node_id}"
        );

        let sections = node["sections"].as_object().expect("sections object");
        for section in [
            "match",
            "ambiguity",
            "decisions",
            "standards",
            "failure_modes",
            "proceed_when",
        ] {
            assert!(
                sections
                    .get(section)
                    .and_then(|items| items.as_array())
                    .is_some_and(|items| !items.is_empty()),
                "{node_id}.{section} must preserve specification guidance"
            );
        }

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
fn all_methodology_nodes_have_architect_grade_delivery_doctrine() {
    let constitution = load_constitution_asset();
    let nodes = constitution["nodes"].as_object().expect("nodes object");
    let lookup = constitution["lookup"].as_object().expect("lookup object");

    let actual_methodology_count = nodes
        .keys()
        .filter(|id| id.starts_with("methodology/"))
        .count();
    assert_eq!(
        actual_methodology_count,
        METHODOLOGY_NODES.len(),
        "METHODOLOGY_NODES test list must cover every methodology/* node"
    );

    let core_refs = nodes["core/METHODOLOGY"]["links"]["references"]
        .as_array()
        .expect("core methodology references");

    for node_id in METHODOLOGY_NODES {
        let node = nodes
            .get(*node_id)
            .unwrap_or_else(|| panic!("missing methodology node {node_id}"));
        assert_eq!(
            node["category"].as_str(),
            Some("methodology"),
            "{node_id} must remain in the methodology namespace"
        );
        assert_methodology_delivery_fields(node_id, node);

        assert!(
            core_refs
                .iter()
                .any(|value| value.as_str() == Some(node_id)),
            "core/METHODOLOGY must route to {node_id}"
        );

        assert!(
            node["links"]["referenced_by"]
                .as_array()
                .is_some_and(|refs| refs
                    .iter()
                    .any(|value| value.as_str() == Some("core/METHODOLOGY"))),
            "{node_id} must be referenced by core/METHODOLOGY"
        );

        let sections = node["sections"].as_object().expect("sections object");
        for section in ["match", "ambiguity", "failure_modes", "proceed_when"] {
            assert!(
                sections
                    .get(section)
                    .and_then(|items| items.as_array())
                    .is_some_and(|items| !items.is_empty()),
                "{node_id}.{section} must preserve route, stop, risk, and proceed guidance"
            );
        }

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
fn all_research_nodes_have_foundational_knowledge_doctrine() {
    let constitution = load_constitution_asset();
    let nodes = constitution["nodes"].as_object().expect("nodes object");
    let lookup = constitution["lookup"].as_object().expect("lookup object");

    let actual_research_count = nodes
        .keys()
        .filter(|id| id.starts_with("research/"))
        .count();
    assert_eq!(
        actual_research_count,
        RESEARCH_NODES.len(),
        "RESEARCH_NODES must cover every research/* node"
    );

    let core_refs = nodes["core/RESEARCH"]["links"]["references"]
        .as_array()
        .expect("core research references");
    assert_eq!(
        core_refs.len(),
        RESEARCH_NODES.len(),
        "core/RESEARCH must route to every research node"
    );

    let core_sections = nodes["core/RESEARCH"]["sections"]
        .as_object()
        .expect("core research sections");
    for section in [
        "match",
        "decide",
        "route",
        "apply",
        "loop_guards",
        "proof_planning",
    ] {
        assert!(
            core_sections
                .get(section)
                .and_then(|items| items.as_array())
                .is_some_and(|items| !items.is_empty()),
            "core/RESEARCH.{section} must preserve research routing and proof guidance"
        );
    }

    for node_id in RESEARCH_NODES {
        let node = nodes
            .get(*node_id)
            .unwrap_or_else(|| panic!("missing research node {node_id}"));
        assert_eq!(
            node["category"].as_str(),
            Some("research"),
            "{node_id} must remain in the research namespace"
        );
        assert_architect_grade_fields(node_id, node);

        let doctrine = node["research_doctrine"]
            .as_object()
            .unwrap_or_else(|| panic!("{node_id}.research_doctrine must be an object"));
        assert!(
            doctrine["authors"]
                .as_array()
                .is_some_and(|items| !items.is_empty()),
            "{node_id}.research_doctrine.authors must identify the source"
        );
        assert!(
            doctrine["year"]
                .as_i64()
                .is_some_and(|year| (1900..=2100).contains(&year)),
            "{node_id}.research_doctrine.year must be a plausible publication year"
        );
        for field in ["domain", "research_question", "contribution", "mechanism"] {
            assert!(
                doctrine[field]
                    .as_str()
                    .is_some_and(|text| !text.trim().is_empty()),
                "{node_id}.research_doctrine.{field} must preserve context and mechanism"
            );
        }
        for field in [
            "engineering_translation",
            "limitations",
            "modern_parallels",
            "misreadings",
        ] {
            assert!(
                doctrine[field]
                    .as_array()
                    .is_some_and(|items| items.len() >= 3),
                "{node_id}.research_doctrine.{field} must provide depth and transfer boundaries"
            );
        }
        assert!(
            doctrine["related_nodes"]
                .as_array()
                .is_some_and(|items| items.len() >= 3
                    && items.iter().all(|item| {
                        item.as_str()
                            .is_some_and(|related| related.starts_with("research/"))
                    })),
            "{node_id}.research_doctrine.related_nodes must preserve research cross-links"
        );
        for retrieval_field in ["match", "avoid", "next_queries"] {
            assert!(
                doctrine["retrieval"][retrieval_field]
                    .as_array()
                    .is_some_and(|items| !items.is_empty()),
                "{node_id}.research_doctrine.retrieval.{retrieval_field} must guide retrieval"
            );
        }

        let sections = node["sections"]
            .as_object()
            .expect("research sections object");
        for section in ["context", "concepts", "impact", "relevance"] {
            assert!(
                sections
                    .get(section)
                    .and_then(|items| items.as_array())
                    .is_some_and(|items| !items.is_empty()),
                "{node_id}.{section} must preserve the existing research context"
            );
        }
        assert!(
            node["links"]["references"]
                .as_array()
                .is_some_and(|refs| refs
                    .iter()
                    .any(|value| { value.as_str() == Some("methodology/RESEARCH") })),
            "{node_id} must remain linked to methodology/RESEARCH"
        );
        assert!(
            node["links"]["referenced_by"]
                .as_array()
                .is_some_and(|refs| refs
                    .iter()
                    .any(|value| { value.as_str() == Some("methodology/RESEARCH") })),
            "{node_id} must remain reachable from methodology/RESEARCH"
        );
        assert!(
            core_refs
                .iter()
                .any(|value| value.as_str() == Some(node_id)),
            "core/RESEARCH must route to {node_id}"
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
fn schema_requires_doctrine_model_for_uplifted_namespaces() {
    let schema = load_constitution_schema();

    assert_eq!(
        schema["x-doctrine_model"]["schema_version"].as_str(),
        Some("section-doctrine-v1"),
        "schema must document the section doctrine model version"
    );
    assert!(
        schema["x-doctrine_model"]["migration_strategy"]
            .as_str()
            .is_some_and(|strategy| {
                strategy.contains("interfaces nodes")
                    && strategy.contains("docs")
                    && strategy.contains("research")
                    && strategy.contains("data nodes")
                    && strategy.contains("metadata nodes")
                    && strategy.contains("plugins nodes")
                    && strategy.contains("specs nodes")
            }),
        "schema must document in-place interfaces, data, metadata, plugins, and specs namespace migration"
    );

    let node_schema = &schema["definitions"]["node"];
    let rules = node_schema["allOf"]
        .as_array()
        .expect("node schema must declare doctrine conditionals");
    let architecture_rule = rules
        .iter()
        .find(|rule| rule["if"]["properties"]["category"]["const"].as_str() == Some("architecture"))
        .expect("node schema must declare architecture doctrine conditional");
    assert_eq!(
        architecture_rule["if"]["properties"]["category"]["const"].as_str(),
        Some("architecture"),
        "conditional must target architecture nodes"
    );
    let docs_rule = rules
        .iter()
        .find(|rule| rule["if"]["properties"]["category"]["const"].as_str() == Some("docs"))
        .expect("node schema must declare docs doctrine conditional");
    let research_rule = rules
        .iter()
        .find(|rule| rule["if"]["properties"]["category"]["const"].as_str() == Some("research"))
        .expect("node schema must declare research doctrine conditional");
    let required = docs_rule["then"]["required"]
        .as_array()
        .expect("docs doctrine required fields");
    for field in ARCHITECTURE_DOCTRINE_FIELDS {
        assert!(
            required.iter().any(|value| value.as_str() == Some(field)),
            "docs schema must require {field}"
        );
    }
    let required = research_rule["then"]["required"]
        .as_array()
        .expect("research doctrine required fields");
    for field in ARCHITECTURE_DOCTRINE_FIELDS
        .iter()
        .copied()
        .chain(std::iter::once("research_doctrine"))
    {
        assert!(
            required.iter().any(|value| value.as_str() == Some(field)),
            "research schema must require {field}"
        );
    }
    let methodology_rule = rules
        .iter()
        .find(|rule| rule["if"]["properties"]["category"]["const"].as_str() == Some("methodology"))
        .expect("node schema must declare methodology doctrine conditional");
    let interface_rule = rules
        .iter()
        .find(|rule| rule["if"]["properties"]["category"]["const"].as_str() == Some("interfaces"))
        .expect("node schema must declare interfaces doctrine conditional");
    let data_rule = rules
        .iter()
        .find(|rule| rule["if"]["properties"]["category"]["const"].as_str() == Some("data"))
        .expect("node schema must declare data doctrine conditional");
    let metadata_rule = rules
        .iter()
        .find(|rule| rule["if"]["properties"]["category"]["const"].as_str() == Some("metadata"))
        .expect("node schema must declare metadata doctrine conditional");
    let plugin_rule = rules
        .iter()
        .find(|rule| rule["if"]["properties"]["category"]["const"].as_str() == Some("plugins"))
        .expect("node schema must declare plugin doctrine conditional");
    let spec_rule = rules
        .iter()
        .find(|rule| rule["if"]["properties"]["category"]["const"].as_str() == Some("specs"))
        .expect("node schema must declare specs doctrine conditional");

    let required = architecture_rule["then"]["required"]
        .as_array()
        .expect("architecture doctrine required fields");
    for field in ARCHITECTURE_DOCTRINE_FIELDS {
        assert!(
            required.iter().any(|value| value.as_str() == Some(field)),
            "architecture schema must require {field}"
        );
    }

    let required = methodology_rule["then"]["required"]
        .as_array()
        .expect("methodology doctrine required fields");
    for field in METHODOLOGY_DOCTRINE_FIELDS {
        assert!(
            required.iter().any(|value| value.as_str() == Some(field)),
            "methodology schema must require {field}"
        );
    }

    let required = interface_rule["then"]["required"]
        .as_array()
        .expect("interfaces doctrine required fields");
    for field in ARCHITECTURE_DOCTRINE_FIELDS {
        assert!(
            required.iter().any(|value| value.as_str() == Some(field)),
            "interfaces schema must require {field}"
        );
    }

    let required = data_rule["then"]["required"]
        .as_array()
        .expect("data doctrine required fields");
    for field in ARCHITECTURE_DOCTRINE_FIELDS {
        assert!(
            required.iter().any(|value| value.as_str() == Some(field)),
            "data schema must require {field}"
        );
    }

    let required = metadata_rule["then"]["required"]
        .as_array()
        .expect("metadata doctrine required fields");
    for field in ARCHITECTURE_DOCTRINE_FIELDS {
        assert!(
            required.iter().any(|value| value.as_str() == Some(field)),
            "metadata schema must require {field}"
        );
    }

    let required = plugin_rule["then"]["required"]
        .as_array()
        .expect("plugin doctrine required fields");
    for field in ARCHITECTURE_DOCTRINE_FIELDS {
        assert!(
            required.iter().any(|value| value.as_str() == Some(field)),
            "plugin schema must require {field}"
        );
    }

    let required = spec_rule["then"]["required"]
        .as_array()
        .expect("spec doctrine required fields");
    for field in ARCHITECTURE_DOCTRINE_FIELDS {
        assert!(
            required.iter().any(|value| value.as_str() == Some(field)),
            "spec schema must require {field}"
        );
    }

    let spec_impacts = schema["definitions"]["spec_impacts"]["properties"]
        .as_object()
        .expect("spec_impacts properties");
    for field in METHODOLOGY_SPEC_IMPACT_FIELDS {
        assert!(
            spec_impacts.contains_key(*field),
            "schema spec_impacts must allow methodology {field}"
        );
    }
    assert!(
        node_schema["properties"].get("research_doctrine").is_some(),
        "node schema must expose the research doctrine field"
    );
    assert!(
        schema["definitions"].get("research_doctrine").is_some(),
        "schema must define the research doctrine contract"
    );

    let base_required = node_schema["required"]
        .as_array()
        .expect("base node required fields");
    assert!(
        !ARCHITECTURE_DOCTRINE_FIELDS
            .iter()
            .any(|field| base_required
                .iter()
                .any(|value| value.as_str() == Some(field))),
        "non-uplifted nodes must keep the compact node contract"
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
fn core_decapod_routes_agentic_work_substrate() {
    let constitution = load_constitution_asset();
    let nodes = constitution["nodes"].as_object().expect("nodes object");
    let lookup = constitution["lookup"].as_object().expect("lookup object");
    let core = &nodes["core/DECAPOD"];

    assert!(
        core["summary"]
            .as_str()
            .is_some_and(|summary| summary.contains("governed work substrate")),
        "core/DECAPOD summary must make the substrate thesis explicit"
    );

    for term in ["agentic", "substrate", "custody", "validation"] {
        assert!(
            core["terms"]
                .as_array()
                .is_some_and(|terms| terms.iter().any(|value| value.as_str() == Some(term))),
            "core/DECAPOD terms must include {term}"
        );
    }

    let route_text = core["sections"]["route"]
        .as_array()
        .expect("core/DECAPOD route entries")
        .iter()
        .filter_map(|value| value.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    for expected in [
        "agentic work substrate",
        "bounded scope",
        "coordination state",
        "independently reviewable completion",
    ] {
        assert!(
            route_text.contains(expected),
            "core/DECAPOD route guidance must mention {expected}"
        );
    }

    let notes = core["architect_notes"]
        .as_array()
        .expect("core/DECAPOD architect notes")
        .iter()
        .filter_map(|value| value.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        notes.contains("governed agent action over trusted repo-native evidence"),
        "core/DECAPOD architect notes must distinguish governed action from context feeding"
    );

    for (term, node_id) in [
        ("agentic work substrate", "core/DECAPOD"),
        ("substrate", "interfaces/CONTROL_PLANE"),
        ("custody", "interfaces/TODO_SCHEMA"),
        ("bounded", "core/INTERFACES"),
        ("scope", "interfaces/PLAN_GOVERNED_EXECUTION"),
        ("completion", "plugins/VERIFY"),
    ] {
        let entries = lookup
            .get(term)
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

#[test]
fn embedded_constitution_preserves_interface_contract_doctrine() {
    for node_id in INTERFACE_NODES {
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
    }
}

#[test]
fn embedded_constitution_preserves_data_doctrine() {
    for node_id in DATA_NODES {
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
            node["links"]["references"]
                .as_array()
                .is_some_and(|refs| !refs.is_empty()),
            "{node_id} must preserve data cross-links in embedded output"
        );
    }
}

#[test]
fn embedded_constitution_preserves_metadata_doctrine() {
    for node_id in METADATA_NODES {
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
            node["links"]["references"]
                .as_array()
                .is_some_and(|refs| !refs.is_empty()),
            "{node_id} must preserve metadata cross-links in embedded output"
        );
    }
}

#[test]
fn embedded_constitution_preserves_plugin_doctrine() {
    for node_id in PLUGIN_NODES {
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
            node["links"]["referenced_by"]
                .as_array()
                .is_some_and(|refs| refs
                    .iter()
                    .any(|value| value.as_str() == Some("core/PLUGINS"))),
            "{node_id} must preserve plugin routing in embedded output"
        );
    }
}

#[test]
fn embedded_constitution_preserves_spec_doctrine() {
    for node_id in SPEC_NODES {
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
            node["links"]["references"]
                .as_array()
                .is_some_and(|refs| refs
                    .iter()
                    .any(|value| value.as_str() == Some("core/SPECS"))),
            "{node_id} must preserve spec routing in embedded output"
        );
    }
}

#[test]
fn embedded_constitution_preserves_methodology_delivery_doctrine() {
    for node_id in [
        "methodology/ARCHITECTURE",
        "methodology/PRODUCT",
        "methodology/QA",
        "methodology/STRATEGIC_DECISION",
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
        assert_methodology_delivery_fields(node_id, &node);
        assert!(
            node["scaffolding"]["capture"]
                .as_array()
                .is_some_and(|items| items.iter().any(|item| item
                    .as_str()
                    .is_some_and(|text| text.contains("user outcome")))),
            "{node_id} must preserve consultative capture guidance in embedded output"
        );
    }
}
