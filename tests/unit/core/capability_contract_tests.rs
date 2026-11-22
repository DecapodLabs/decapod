use super::*;

#[test]
fn embedded_schema_matches_contract_constants() {
    let schema: serde_json::Value =
        serde_json::from_str(CAPABILITY_CONTRACT_SCHEMA_DOCUMENT).expect("parse capability schema");
    assert_eq!(schema["$id"], CAPABILITY_CONTRACT_SCHEMA_ID);
    assert_eq!(
        schema["properties"]["schema_version"]["const"],
        CAPABILITY_CONTRACT_SCHEMA_VERSION
    );
    assert_eq!(schema["additionalProperties"], false);
}

#[test]
fn self_verified_descriptor_cannot_certify_itself() {
    let mut bundle = canonical_cloud_task_bundle();
    bundle.contract.declared_status = ContractStatus::CurrentlyVerified;
    bundle.contract.evidence.is_self_reported = true;
    let report = evaluate_composed_capability(&bundle);
    assert_eq!(report.status, ContractStatus::Incompatible);
    assert!(!report.observed_behavior_verified);
    assert!(!report.user_intent_correctness_established);
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.kind == DriftKind::SelfVerificationRejected),
        "expected SelfVerificationRejected, got {:?}",
        report.findings
    );
}

#[test]
fn self_reported_observation_is_rejected_even_with_valid_schema() {
    let mut bundle = canonical_cloud_task_bundle();
    bundle.observed_evidence.as_mut().unwrap().self_reported = true;
    let report = evaluate_composed_capability(&bundle);
    assert_eq!(report.status, ContractStatus::Incompatible);
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.kind == DriftKind::SelfVerificationRejected)
    );
}

#[test]
fn multi_repo_compatibility_fixture_passes_all_negative_and_repaired_traces() {
    let fixture = run_compatibility_fixture_suite();
    assert!(
        fixture.total_scenarios >= 16,
        "expected exhaustive negative and repaired traces, got {}",
        fixture.total_scenarios
    );
    for scenario in &fixture.scenarios {
        assert!(
            scenario.passed,
            "scenario '{}' failed: expected status={:?} consequence={:?} goal_completed={:?}, actual status={:?} consequence={:?} findings={:?}",
            scenario.scenario_id,
            scenario.expected_status,
            scenario.expected_consequence_state,
            scenario.expected_goal_completed,
            scenario.report.status,
            scenario.report.consequence,
            scenario.report.findings
        );
    }
    assert_eq!(fixture.failed_scenarios, 0);
}

#[test]
fn dactyl_field_and_error_drift_not_cleared_by_http_200_response() {
    let mut bundle = canonical_cloud_task_bundle();
    bundle.observed_evidence = None;
    bundle.consequence_trace = None;
    let propodus = bundle.propodus_descriptor.as_mut().unwrap();
    propodus.client_reported_http_ok = true;
    propodus
        .output_schema
        .required_fields
        .retain(|f| f != "readback_digest");
    propodus
        .error_schema
        .error_classes
        .retain(|e| e != "prestate_conflict");

    let report = evaluate_composed_capability(&bundle);
    assert_eq!(report.status, ContractStatus::Incompatible);
    assert!(!report.syntactically_compatible);
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.kind == DriftKind::UndeclaredResponseField)
    );
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.kind == DriftKind::RemovedErrorClass)
    );
}

#[test]
fn four_composite_consequence_states_remain_distinct_and_honest() {
    // 1. Prepared: eligible, no effect yet
    let mut prepared = canonical_cloud_task_bundle();
    prepared.observed_evidence = None;
    prepared.consequence_trace.as_mut().unwrap().steps.clear();
    prepared
        .consequence_trace
        .as_mut()
        .unwrap()
        .readback_observation = None;
    let rep = evaluate_composed_capability(&prepared);
    let c = rep.consequence.unwrap();
    assert_eq!(c.state, CompositeConsequenceState::Prepared);
    assert!(c.eligible);
    assert!(!c.effect_established);
    assert!(!c.goal_completed);

    // 2. Observed: owner poststate + independent readback
    let observed = canonical_cloud_task_bundle();
    let rep = evaluate_composed_capability(&observed);
    assert_eq!(rep.status, ContractStatus::CurrentlyVerified);
    let c = rep.consequence.unwrap();
    assert_eq!(c.state, CompositeConsequenceState::Observed);
    assert!(c.effect_established);
    assert!(c.goal_completed);
    assert!(!rep.user_intent_correctness_established);

    // 3. Held: severed ack after step 1 with blind replay attempted -> goal_completed=false
    let mut held = canonical_cloud_task_bundle();
    held.observed_evidence = None;
    let trace = held.consequence_trace.as_mut().unwrap();
    trace.steps = vec![
        TraceStepRecord {
            step_index: 1,
            operation: "task.claim".to_string(),
            succeeded: true,
            uncertain_after_send: false,
            error_code: None,
        },
        TraceStepRecord {
            step_index: 2,
            operation: "proof_acceptance.record".to_string(),
            succeeded: false,
            uncertain_after_send: true,
            error_code: Some("severed_ack".to_string()),
        },
    ];
    trace.severed_ack_after_possible_commit = true;
    trace.blind_replay_attempted = true;
    trace.readback_observation = None;
    let rep = evaluate_composed_capability(&held);
    let c = rep.consequence.unwrap();
    assert_eq!(c.state, CompositeConsequenceState::Held);
    assert!(c.containment_preserved);
    assert!(!c.goal_completed);
    assert!(c.blind_replay_rejected);
    assert!(
        c.preserved_outcomes
            .contains(&PreservedOutcomeClass::Uncertain)
    );

    // 4. Compensated: supported corrective op verified, original fault retained
    let mut comp = canonical_cloud_task_bundle();
    let trace = comp.consequence_trace.as_mut().unwrap();
    trace.steps[1].succeeded = false;
    trace.steps[1].error_code = Some("proof_rejected".to_string());
    trace.original_fault = Some(FaultRecord {
        step: "step_2".to_string(),
        code: "proof_rejected".to_string(),
        detail: "step 2 failed".to_string(),
        outcome_class: PreservedOutcomeClass::Failed,
    });
    trace.compensation = Some(CompensationRecord {
        operation: "task.release".to_string(),
        executed: true,
        final_readback_verified: true,
        original_fault_retained: true,
        poststate_digest: Some("sha256:released".to_string()),
    });
    let rep = evaluate_composed_capability(&comp);
    let c = rep.consequence.unwrap();
    assert_eq!(c.state, CompositeConsequenceState::Compensated);
    assert!(c.containment_preserved);
    assert!(!c.goal_completed);
    assert!(c.original_fault_retained.is_some());
}

#[test]
fn compensation_that_erases_original_fault_is_rejected() {
    let mut comp = canonical_cloud_task_bundle();
    let trace = comp.consequence_trace.as_mut().unwrap();
    trace.steps[1].succeeded = false;
    trace.original_fault = None;
    trace.compensation = Some(CompensationRecord {
        operation: "task.release".to_string(),
        executed: true,
        final_readback_verified: true,
        original_fault_retained: false,
        poststate_digest: Some("sha256:released".to_string()),
    });
    let rep = evaluate_composed_capability(&comp);
    assert_eq!(rep.status, ContractStatus::Incompatible);
    assert_eq!(
        rep.consequence.unwrap().state,
        CompositeConsequenceState::Held
    );
    assert!(
        rep.findings
            .iter()
            .any(|f| f.kind == DriftKind::FaultErasedByCompensation)
    );
}
