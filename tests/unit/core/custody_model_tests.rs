// Moved from src/decapod/core/custody_model.rs
use super::*;

#[test]
fn recorded_clarifications_preserve_origin_interpretation_and_idempotent_order() {
    let mut custody = Custody::in_memory();
    custody
        .create_intent("same-intent", "A: preserve v1".into())
        .unwrap();
    custody
        .refine(
            "same-intent",
            IntentRefinement {
                refined_intent: "B: replace it with v2".into(),
                ..Default::default()
            },
        )
        .unwrap();
    custody.approve("same-intent").unwrap();
    let before = custody.state().intents["same-intent"]
        .instruction_revision()
        .unwrap();
    let input = InstructionRevisionInput {
        revision_id: "clarification-C".into(),
        expected_revision: before.clone(),
        instruction: "C: retain v1 compatibility".into(),
        source_claim: ReportedInstructionSource {
            source_ref: "fixture:human-C".into(),
            principal_claim: Some("principal-C".into()),
        },
        kind: RecordedInstructionKind::Clarification,
        supersedes: vec![],
        derived_intent: Some("Preserve v1 as clarified".into()),
    };
    let c = custody
        .append_instruction("same-intent", input.clone())
        .unwrap();
    let after = custody.state().clone();
    let record = &after.intents["same-intent"];
    assert_eq!(record.raw_intent, "A: preserve v1");
    assert_eq!(
        c.preceding_refinement.as_deref(),
        Some("B: replace it with v2")
    );
    assert_eq!(
        record.refined_intent.as_deref(),
        Some("Preserve v1 as clarified")
    );
    assert_eq!(record.status, IntentStatus::Refined);
    assert_ne!(before, record.instruction_revision().unwrap());
    assert_eq!(
        custody
            .append_instruction("same-intent", input.clone())
            .unwrap(),
        c
    );
    assert_eq!(
        &after,
        custody.state(),
        "identical retry cannot revoke a later acceptance again"
    );
    let mut changed = input.clone();
    changed.instruction = "forged retry".into();
    assert!(
        custody
            .append_instruction("same-intent", changed)
            .unwrap_err()
            .to_string()
            .contains("INSTRUCTION_REVISION_CONFLICT")
    );
    let mut stale = input;
    stale.revision_id = "new-id-with-stale-parent".into();
    assert!(
        custody
            .append_instruction("same-intent", stale)
            .unwrap_err()
            .to_string()
            .contains("INSTRUCTION_REVISION_STALE")
    );
    assert_eq!(&after, custody.state());
    assert_eq!(
        record.instruction_revisions[0].source_claim.confidence(),
        "agent_reported"
    );
    assert!(serde_json::from_value::<ReportedInstructionSource>(serde_json::json!({"source_ref":"fixture:C","principal_claim":"principal-C","authenticated":true})).is_err());
}

fn refinement() -> IntentRefinement {
    IntentRefinement {
        refined_intent: "ship the bounded change".to_string(),
        acceptance_criteria: vec!["tests pass".to_string()],
        constraints: vec!["no orchestration".to_string()],
        assumptions: vec!["current runtime owns persistence".to_string()],
        boundaries: vec!["src/**".to_string()],
        out_of_scope: vec!["database storage".to_string()],
        proof_requirements: vec!["unit tests".to_string()],
    }
}

#[test]
fn lifecycle_preserves_raw_intent_and_orders_events() {
    let raw = "  keep this request exactly  ".to_string();
    let mut custody = Custody::in_memory();
    custody.create_intent("intent-1", raw.clone()).unwrap();
    custody.refine("intent-1", refinement()).unwrap();
    custody.approve("intent-1").unwrap();
    custody.approve_scope("intent-1").unwrap();
    custody.start_mutation("intent-1").unwrap();
    custody
        .record_proof("intent-1", "tests".to_string())
        .unwrap();
    custody
        .record_validation("intent-1", "validate".to_string())
        .unwrap();
    custody.complete("intent-1").unwrap();

    let record = &custody.state().intents["intent-1"];
    assert_eq!(record.raw_intent, raw);
    assert_eq!(record.status, IntentStatus::Completed);
    assert!(
        custody
            .state()
            .events
            .windows(2)
            .all(|events| events[0].sequence < events[1].sequence)
    );
}

#[test]
fn invalid_transition_is_typed_and_trajectory_is_evidence() {
    let mut custody = Custody::in_memory();
    custody
        .create_intent("intent-2", "request".to_string())
        .unwrap();
    assert!(matches!(
        custody.approve("intent-2"),
        Err(CustodyError::InvalidTransition { .. })
    ));
    custody.refine("intent-2", refinement()).unwrap();
    let event_id = custody
        .append_trajectory_step(
            "run-2",
            "intent-2",
            TrajectoryStepInput {
                action: "cargo test".to_string(),
                command: Some("cargo test".to_string()),
                scope: vec!["src/**".to_string()],
                proof_refs: vec!["test-output".to_string()],
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        custody.state().intents["intent-2"].status,
        IntentStatus::Refined
    );
    assert_eq!(
        custody.state().trajectories["run-2"].steps[0].custody_event_id,
        event_id
    );
}

#[test]
fn contaminated_summary_and_knowledge_candidate_remain_explicit() {
    let mut custody = Custody::in_memory();
    custody
        .create_intent("intent-3", "request".to_string())
        .unwrap();
    custody.refine("intent-3", refinement()).unwrap();
    custody
        .contaminate("intent-3", "untrusted input observed".to_string())
        .unwrap();
    let candidate = custody
        .propose_knowledge_candidate(
            "intent-3",
            "keep external artifacts quarantined".to_string(),
            vec!["event:3".to_string()],
        )
        .unwrap();
    let summary = custody.summarize("intent-3").unwrap();
    assert_eq!(summary.status, IntentStatus::Contaminated);
    assert_eq!(summary.knowledge_candidate_ids, vec![candidate.id]);
    assert_eq!(candidate.authority, "proposed");
}
