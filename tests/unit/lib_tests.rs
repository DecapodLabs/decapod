// Moved from src/decapod/plan_governance.rs
use super::*;

#[test]
fn human_input_gate_blocks_empty_intent() {
    let dir = tempfile::tempdir().unwrap();
    let plan = init_plan(
        dir.path(),
        InitPlanInput {
            title: "Title".to_string(),
            intent: "".to_string(),
            todo_ids: vec!["T1".to_string()],
            proof_hooks: vec!["validate_passes".to_string()],
            unknowns: vec![],
            human_questions: vec![],
            stop_conditions: vec![],
            unresolved_contradictions: vec![],
            deferred_questions: vec![],
            constraints: ScopeConstraints::default(),
            phases: vec![],
        },
    )
    .unwrap();
    assert_eq!(plan.state, PlanState::Draft);
}

fn phase(id: &str) -> Phase {
    Phase {
        id: id.to_string(),
        name: id.to_string(),
        description: format!("{id} work"),
        entry_gates: vec![],
        exit_gates: vec![],
        entered: false,
        completed: false,
        entered_at: None,
        completed_at: None,
    }
}

#[test]
fn phases_are_entered_in_order_and_done_requires_all_phases() {
    let dir = tempfile::tempdir().unwrap();
    init_plan(
        dir.path(),
        InitPlanInput {
            title: "ordered".to_string(),
            intent: "prove ordered execution".to_string(),
            todo_ids: vec![],
            proof_hooks: vec!["validate_passes".to_string()],
            unknowns: vec![],
            human_questions: vec![],
            stop_conditions: vec![],
            unresolved_contradictions: vec![],
            deferred_questions: vec![],
            constraints: ScopeConstraints::default(),
            phases: vec![],
        },
    )
    .unwrap();
    add_phase(dir.path(), phase("plan")).unwrap();
    add_phase(dir.path(), phase("build")).unwrap();
    patch_plan(
        dir.path(),
        PlanPatch {
            state: Some(PlanState::Approved),
            ..Default::default()
        },
    )
    .unwrap();

    let blocked = enter_phase(dir.path(), "build").unwrap_err().to_string();
    assert!(blocked.contains("INVALID_PHASE_TRANSITION"));
    enter_phase(dir.path(), "plan").unwrap();
    complete_phase(dir.path(), "plan").unwrap();
    enter_phase(dir.path(), "build").unwrap();
    let incomplete = patch_plan(
        dir.path(),
        PlanPatch {
            state: Some(PlanState::Done),
            ..Default::default()
        },
    )
    .unwrap_err()
    .to_string();
    assert!(incomplete.contains("PHASES_INCOMPLETE"));
    complete_phase(dir.path(), "build").unwrap();
    assert_eq!(
        load_plan(dir.path()).unwrap().unwrap().state,
        PlanState::Done
    );
}
