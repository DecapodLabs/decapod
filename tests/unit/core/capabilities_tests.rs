// Moved from src/decapod/core/capabilities.rs
use super::*;

#[test]
fn test_registry_contains_builtins() {
    let registry = CapabilityRegistry::new();
    assert!(registry.get("public-api").is_some());
    assert!(registry.get("persistent-state").is_some());
    assert!(registry.get("background-processing").is_some());
    assert_eq!(registry.ids().len(), 12);
    assert_eq!(registry.ids().len(), registry.all().len());
}

#[test]
#[should_panic(expected = "duplicate built-in capability registration")]
fn duplicate_registry_ids_are_rejected() {
    let mut registry = CapabilityRegistry::new();
    registry.register(CapabilityDefinition {
        id: "public-api".to_string(),
        name: "duplicate".to_string(),
        purpose: String::new(),
        affected_specs: vec![],
        required_decisions: vec![],
        proof_obligations: vec![],
        scaffolding_recommendations: vec![],
        evidence_signals: vec![],
        conflicts: vec![],
        requires: vec![],
    });
}

#[test]
fn test_validate_capabilities_valid() {
    let registry = CapabilityRegistry::new();
    let result =
        registry.validate_capabilities(&["public-api".to_string(), "persistent-state".to_string()]);
    assert!(result.is_ok());
}

#[test]
fn test_validate_capabilities_conflict() {
    let registry = CapabilityRegistry::new();
    // stateless conflicts with persistent-state
    let result =
        registry.validate_capabilities(&["stateless".to_string(), "persistent-state".to_string()]);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("conflicts"));
}

#[test]
fn test_validate_capabilities_missing_requirement() {
    let registry = CapabilityRegistry::new();
    // authorization requires authentication
    let result = registry.validate_capabilities(&["authorization".to_string()]);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("requires"));
}

#[test]
fn test_validate_capabilities_duplicate() {
    let registry = CapabilityRegistry::new();
    let result =
        registry.validate_capabilities(&["public-api".to_string(), "public-api".to_string()]);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Duplicate"));
}

#[test]
fn test_validate_unknown_capability() {
    let registry = CapabilityRegistry::new();
    let result = registry.validate_capabilities(&["unknown-capability".to_string()]);
    // Unknown capabilities are now allowed (open vocabulary)
    assert!(result.is_ok());
}

#[test]
fn test_generate_overlays() {
    let registry = CapabilityRegistry::new();
    let overlays =
        registry.generate_overlays(&["public-api".to_string(), "persistent-state".to_string()]);
    assert_eq!(overlays.len(), 2);
}

#[test]
fn built_in_overlays_do_not_select_universal_service_levels() {
    let content = apply_capability_overlays(
        "VALIDATION.md",
        "# Validation\n\n## Gates\n".to_string(),
        &[
            "public-api".to_string(),
            "persistent-state".to_string(),
            "background-processing".to_string(),
        ],
    );
    for forbidden in [
        "90 days",
        "daily incremental",
        "< 1 hour",
        "< 4 hours",
        "30 seconds",
        "Exactly-once processing verification",
    ] {
        assert!(!content.contains(forbidden), "overlay contains {forbidden}");
    }
    assert!(content.contains("Migration Proof Command"));
    assert!(content.contains("Public API Validation Overlay"));
    assert!(content.contains("Persistent State Validation Overlay"));
    assert!(content.contains("Background Processing Validation Overlay"));
    assert!(content.contains("Verify the declared delivery guarantee"));
}

#[test]
fn capability_overlays_are_not_nested_inside_codebase_attestation() {
    // Compacted living specs with mid-document prose after potential anchors
    // must still receive overlays immediately before attestation only.
    let body = "# Semantics## State Machines\n- item\n- Business rule 3:## Idempotency Contracts\n\n| Operation | Key |\n|---|---|\n| create | id |\n## Language Note\n- Primary language inferred: Rust\n<!-- decapod:codebase-attestation:start -->\n## Codebase Attestation\n- Repository signal fingerprint: `abc`\n<!-- decapod:codebase-attestation:end -->\n";
    let updated = reconcile_capability_overlays(
        ".decapod/managed/specs/SEMANTICS.md",
        body.to_string(),
        &[
            "background-processing".to_string(),
            "persistent-state".to_string(),
        ],
    );

    let attestation_start = updated
        .find("<!-- decapod:codebase-attestation:start -->")
        .expect("attestation start");
    let overlay_start = updated
        .find("<!-- decapod:capability-overlay:background-processing:start -->")
        .expect("background-processing overlay");
    assert!(
        overlay_start < attestation_start,
        "capability overlays must precede codebase attestation:\n{updated}"
    );
    // Authored prose after the last bullet and before attestation must stay
    // contiguous before overlays (never split by refresh).
    let authored_tail = &updated[..overlay_start];
    assert!(
        authored_tail.contains("- Business rule 3:## Idempotency Contracts"),
        "authored prose must stay before overlays:\n{updated}"
    );
    assert!(
        authored_tail.contains("| Operation | Key |"),
        "table body must stay before overlays:\n{updated}"
    );
    assert!(
        authored_tail.contains("## Language Note"),
        "language note must stay before overlays:\n{updated}"
    );
    let attestation_end = updated
        .find("<!-- decapod:codebase-attestation:end -->")
        .expect("attestation end");
    let between = &updated[attestation_start..attestation_end];
    assert!(
        !between.contains("decapod:capability-overlay:"),
        "overlays must not nest inside attestation:\n{between}"
    );

    // A second reconcile must keep placement and authored prose.
    let again = reconcile_capability_overlays(
        ".decapod/managed/specs/SEMANTICS.md",
        updated.clone(),
        &[
            "background-processing".to_string(),
            "persistent-state".to_string(),
        ],
    );
    assert_eq!(
        updated, again,
        "overlay reconciliation must be byte-stable after the first apply"
    );
}
