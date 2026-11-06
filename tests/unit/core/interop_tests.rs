use super::*;

fn sample_manifest() -> InteropManifest {
    builtin_manifest()
}

#[test]
fn builtin_manifest_validates_clean() {
    let manifest = sample_manifest();
    assert!(
        validate_manifest(&manifest).is_ok(),
        "builtin manifest must pass its own validation"
    );
}

#[test]
fn manifest_derives_entries_from_registered_ops() {
    let manifest = sample_manifest();
    for op in RPC_OPS {
        let id = format!("rpc:{op}");
        let cap = manifest
            .capabilities
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("manifest missing entry for registered op {op}"));
        assert_eq!(cap.implementation_status, ImplementationStatus::Implemented);
        let mapping = cap
            .implementation
            .as_ref()
            .expect("implemented entry has mapping");
        assert_eq!(&mapping.op, op);
    }
    // Every rpc capability maps to a registered op (no drift).
    for cap in manifest.capabilities.iter().filter(|c| c.surface == "rpc") {
        let op = cap.implementation.as_ref().unwrap().op.as_str();
        assert!(
            RPC_OPS.contains(&op),
            "manifest advertises unregistered op {op}"
        );
    }
}

#[test]
fn manifest_marks_mcp_acp_a2a_intentionally_not_used() {
    let manifest = sample_manifest();
    for id in ["transport:mcp", "transport:acp", "transport:a2a"] {
        let cap = manifest
            .capabilities
            .iter()
            .find(|c| c.id == id)
            .expect("transport entry exists");
        assert_eq!(
            cap.implementation_status,
            ImplementationStatus::IntentionallyNotUsed
        );
        assert!(cap.implementation.is_none());
    }
}

#[test]
fn validation_rejects_duplicate_ids() {
    let mut manifest = sample_manifest();
    let first = manifest.capabilities[0].clone();
    manifest.capabilities.push(first);
    let violations = validate_manifest(&manifest).unwrap_err();
    assert!(violations.iter().any(|v| v.code == "duplicate_id"));
}

#[test]
fn validation_rejects_empty_id_and_missing_owner() {
    let mut manifest = sample_manifest();
    manifest.capabilities[0].id = "   ".to_string();
    manifest.capabilities[1].owner_component = String::new();
    let violations = validate_manifest(&manifest).unwrap_err();
    assert!(violations.iter().any(|v| v.code == "empty_id"));
    assert!(violations.iter().any(|v| v.code == "missing_owner"));
}

#[test]
fn validation_rejects_implemented_without_mapping() {
    let mut manifest = sample_manifest();
    manifest.capabilities[0].implementation = None;
    let violations = validate_manifest(&manifest).unwrap_err();
    assert!(
        violations
            .iter()
            .any(|v| v.code == "missing_implementation_mapping")
    );
}

#[test]
fn validation_rejects_unregistered_op_mapping() {
    let mut manifest = sample_manifest();
    manifest.capabilities[0].implementation.as_mut().unwrap().op = "not.a.real.op".to_string();
    let violations = validate_manifest(&manifest).unwrap_err();
    assert!(
        violations
            .iter()
            .any(|v| v.code == "unknown_implementation_op")
    );
}

#[test]
fn validation_rejects_verified_without_admissible_evidence() {
    let mut manifest = sample_manifest();
    // An entry that is implemented but claims verified with a fabricated test id.
    let mut cap = manifest.capabilities[0].clone();
    cap.id = "rpc:fabricated".to_string();
    cap.evidence.conformance_test = "some_fake_test".to_string();
    cap.evidence.evidence_ref = Some("nowhere".to_string());
    manifest.capabilities.push(cap);
    let violations = validate_manifest(&manifest).unwrap_err();
    assert!(
        violations
            .iter()
            .any(|v| v.code == "verification_without_admissible_evidence")
    );

    // Verified without any evidence_ref is also rejected.
    let mut manifest = sample_manifest();
    let mut cap = manifest.capabilities[0].clone();
    cap.id = "rpc:noevidence".to_string();
    cap.evidence.conformance_test = KNOWN_CONFORMANCE_TESTS[0].to_string();
    cap.evidence.evidence_ref = None;
    manifest.capabilities.push(cap);
    let violations = validate_manifest(&manifest).unwrap_err();
    assert!(
        violations
            .iter()
            .any(|v| v.code == "verification_without_evidence_ref")
    );
}

#[test]
fn validation_rejects_planned_with_mapping_and_bad_versions() {
    let mut manifest = sample_manifest();
    manifest.schema_version = "not.a.version".to_string();
    let mut cap = manifest.capabilities[0].clone();
    cap.id = "rpc:planned-with-mapping".to_string();
    cap.implementation_status = ImplementationStatus::Planned;
    manifest.capabilities.push(cap);
    let violations = validate_manifest(&manifest).unwrap_err();
    assert!(
        violations
            .iter()
            .any(|v| v.code == "invalid_schema_version")
    );
    assert!(
        violations
            .iter()
            .any(|v| v.code == "contradictory_declaration")
    );
}

#[test]
fn negotiation_supported_required_ops_succeeds_deterministically() {
    let manifest = sample_manifest();
    let req = NegotiationRequest {
        required_ops: vec!["schema.get".to_string(), "agent.init".to_string()],
        ..Default::default()
    };
    let a = negotiate(&manifest, &req).expect("negotiates");
    let req2 = NegotiationRequest {
        required_ops: vec!["agent.init".to_string(), "schema.get".to_string()],
        ..Default::default()
    };
    let b = negotiate(&manifest, &req2).expect("negotiates");
    assert!(a.negotiated);
    assert_eq!(a.supported_required_ops, b.supported_required_ops);
    assert_eq!(a.supported_required_ops, vec!["agent.init", "schema.get"]);
}

#[test]
fn negotiation_refuses_unsupported_mandatory_op_before_work() {
    let manifest = sample_manifest();
    let req = NegotiationRequest {
        required_ops: vec!["rpc:transport:mcp".to_string()],
        ..Default::default()
    };
    let err = negotiate(&manifest, &req).unwrap_err();
    assert_eq!(
        err,
        NegotiationRefusal::UnsupportedRequiredOp {
            op: "rpc:transport:mcp".to_string()
        }
    );
    assert_eq!(err.code(), "negotiation_refused");
}

#[test]
fn negotiation_refuses_invalid_capability_id() {
    let manifest = sample_manifest();
    let req = NegotiationRequest {
        required_ops: vec!["totally.made.up".to_string()],
        ..Default::default()
    };
    let err = negotiate(&manifest, &req).unwrap_err();
    assert_eq!(
        err,
        NegotiationRefusal::InvalidCapabilityId {
            id: "totally.made.up".to_string()
        }
    );
}

#[test]
fn negotiation_refuses_incompatible_version() {
    let manifest = sample_manifest();
    let req = NegotiationRequest {
        profile_version: Some("v99".to_string()),
        ..Default::default()
    };
    let err = negotiate(&manifest, &req).unwrap_err();
    match err {
        NegotiationRefusal::UnsupportedProfileVersion {
            requested,
            supported,
        } => {
            assert_eq!(requested, "v99");
            assert!(supported.contains(&"v1".to_string()));
        }
        other => panic!("expected UnsupportedProfileVersion, got {other:?}"),
    }
}

#[test]
fn negotiation_refuses_unsupported_required_extension() {
    let manifest = sample_manifest();
    let req = NegotiationRequest {
        required_extensions: vec!["mind-control".to_string()],
        ..Default::default()
    };
    let err = negotiate(&manifest, &req).unwrap_err();
    assert_eq!(
        err,
        NegotiationRefusal::UnsupportedRequiredExtension {
            extension: "mind-control".to_string()
        }
    );
}

#[test]
fn negotiation_omits_unknown_optional_extensions_safely() {
    let manifest = sample_manifest();
    let req = NegotiationRequest {
        optional_extensions: vec!["zzz-unknown".to_string(), "aaa-unknown".to_string()],
        optional_ops: vec!["rpc:transport:mcp".to_string()],
        ..Default::default()
    };
    let out = negotiate(&manifest, &req).expect("optional omission is not a refusal");
    assert!(out.negotiated);
    // Sorted => ordering of independent optional inputs cannot change the result.
    assert_eq!(
        out.omitted_optional_extensions,
        vec!["aaa-unknown", "zzz-unknown"]
    );
    assert_eq!(out.omitted_optional_ops, vec!["rpc:transport:mcp"]);
}

#[test]
fn negotiation_rejects_duplicate_and_contradictory_requests() {
    let manifest = sample_manifest();
    let dup = NegotiationRequest {
        required_ops: vec!["schema.get".to_string(), "schema.get".to_string()],
        ..Default::default()
    };
    assert!(matches!(
        negotiate(&manifest, &dup).unwrap_err(),
        NegotiationRefusal::MalformedRequest { .. }
    ));
    let contradictory = NegotiationRequest {
        required_ops: vec!["schema.get".to_string()],
        optional_ops: vec!["schema.get".to_string()],
        ..Default::default()
    };
    assert!(matches!(
        negotiate(&manifest, &contradictory).unwrap_err(),
        NegotiationRefusal::MalformedRequest { .. }
    ));
}

#[test]
fn negotiation_reports_profile_status_from_manifest_not_declaration() {
    let manifest = sample_manifest();
    let req = NegotiationRequest {
        requested_profile: Some("read-only-orientation".to_string()),
        ..Default::default()
    };
    let out = negotiate(&manifest, &req).unwrap();
    assert_eq!(
        out.selected_profile.as_deref(),
        Some("read-only-orientation")
    );
    assert_eq!(out.profile_status, Some(ProfileStatus::Implemented));
    assert!(out.missing_profile_ops.is_empty());
    assert!(out.negotiated);

    let req = NegotiationRequest {
        requested_profile: Some("fleet-observation".to_string()),
        ..Default::default()
    };
    let out = negotiate(&manifest, &req).unwrap();
    assert_eq!(out.profile_status, Some(ProfileStatus::Partial));
    assert_eq!(out.missing_profile_ops, vec!["fleet.observe"]);
    assert!(!out.negotiated);
}

#[test]
fn negotiation_unknown_profile_refuses() {
    let manifest = sample_manifest();
    let req = NegotiationRequest {
        requested_profile: Some("no-such-profile".to_string()),
        ..Default::default()
    };
    assert!(matches!(
        negotiate(&manifest, &req).unwrap_err(),
        NegotiationRefusal::UnknownProfile { .. }
    ));
}

#[test]
fn negotiation_never_grants_authority() {
    let manifest = sample_manifest();
    let req = NegotiationRequest {
        required_ops: vec!["agent.init".to_string()],
        ..Default::default()
    };
    let out = negotiate(&manifest, &req).unwrap();
    assert!(out.authority_note.contains("grants no authority"));
}

#[test]
fn known_conformance_tests_match_harness_scenarios() {
    // The harness (tests/interop_conformance.rs) declares these scenario ids;
    // keep the two lists in sync deliberately.
    assert!(KNOWN_CONFORMANCE_TESTS.contains(&"interop_conformance::stale_evidence"));
    assert_eq!(KNOWN_CONFORMANCE_TESTS.len(), 8);
}
