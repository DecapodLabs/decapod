use serde_json::Value;

const DESIGN: &str = include_str!("../docs/architecture/interfaces-v2-restructure.md");
const SCHEMA: &str = include_str!("../docs/architecture/interfaces-v2-contract.schema.json");

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
