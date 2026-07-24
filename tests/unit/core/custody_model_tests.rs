// Moved from src/decapod/core/custody_model.rs
use super::*;

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
