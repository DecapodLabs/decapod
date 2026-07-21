use serde_json::Value;

const DESIGN: &str = include_str!("../docs/architecture/interfaces-v2-restructure.md");
const SCHEMA: &str = include_str!("../docs/architecture/interfaces-v2-contract.schema.json");
const CONSTITUTION: &str = include_str!("../assets/constitution.json");

#[test]
fn aggressive_interface_design_schema_is_strict_and_versioned() {
    let schema: Value =
        serde_json::from_str(SCHEMA).expect("interface envelope schema is valid JSON");

    assert_eq!(
        schema["$id"],
        "https://decapod.dev/schemas/interfaces/contract-envelope-2.0.0.json"
    );
    assert_eq!(schema["properties"]["contract_version"]["const"], "2.0.0");
    assert_eq!(schema["additionalProperties"], false);

    let required = schema["required"]
        .as_array()
        .expect("top-level required fields are an array");
    for field in [
        "contract_id",
        "contract_version",
        "kind",
        "operation",
        "request_id",
        "correlation_id",
        "producer",
        "consumer",
        "scope",
        "lifecycle",
        "outcome",
        "proof",
    ] {
        assert!(
            required.iter().any(|value| value == field),
            "missing {field}"
        );
    }

    for definition in [
        "operation",
        "participant",
        "scope",
        "lifecycle",
        "outcome",
        "error",
        "proof",
    ] {
        assert!(
            schema["$defs"][definition].is_object(),
            "missing {definition} definition"
        );
    }
}

#[test]
fn aggressive_interface_design_carries_the_migration_inventory_and_boundary() {
    for interface in [
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
    ] {
        assert!(
            DESIGN.contains(interface),
            "design inventory omits {interface}"
        );
    }

    for marker in [
        "Phase 0",
        "Phase 5",
        "rollback",
        "InternalizationCreateResult",
        "InternalizationInspectResult",
        "Knowledge remains a separate subsystem",
        "does not implement the knowledge subsystem",
    ] {
        assert!(DESIGN.contains(marker), "design is missing {marker}");
    }
}

#[test]
fn core_interfaces_constitution_requires_envelope_adherence() {
    let constitution: Value =
        serde_json::from_str(CONSTITUTION).expect("constitution is valid JSON");
    let router = &constitution["nodes"]["core/INTERFACES"];
    assert!(router.is_object(), "core/INTERFACES router exists");

    let references = router["links"]["references"]
        .as_array()
        .expect("interface router references are an array");
    assert_eq!(
        references.len(),
        25,
        "all current interfaces/* nodes remain routed"
    );

    let sections = &router["sections"];
    for section in [
        "match",
        "decide",
        "route",
        "apply",
        "avoid",
        "proof_planning",
    ] {
        assert!(sections[section].is_array(), "missing {section} doctrine");
    }

    let doctrine = serde_json::to_string(router).expect("router can be serialized");
    for marker in [
        "canonical interface contract envelope",
        "command, entity, event, artifact, or schema",
        "temporary adapter window",
        "rollback trigger",
        "typed outcome/failure",
        "deprecated adapters",
    ] {
        assert!(doctrine.contains(marker), "core/INTERFACES omits {marker}");
    }
    assert!(
        !doctrine.contains("v2") && !doctrine.contains("V2"),
        "constitution router must describe the contract, not its version label"
    );
}
