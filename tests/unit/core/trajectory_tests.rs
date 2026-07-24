// Moved from src/decapod/core/trajectory.rs
use super::*;
use tempfile::tempdir;

#[test]
fn trajectory_creation_is_inspectable_and_unproven_without_checks() {
    let temp = tempdir().unwrap();
    let artifact = init_trajectory(
        temp.path(),
        TrajectoryInit {
            run_id: "run_1".to_string(),
            task_id: Some("task_1".to_string()),
            intent_id: None,
            original_intent: "original intent".to_string(),
            derived_intent: "derived intent".to_string(),
            active_boundaries: vec!["src/**".to_string()],
            repo_scope: vec!["src/decapod/lib.rs".to_string()],
            destination: None,
            current_phase: None,
            next_transitions: Vec::new(),
            blockers: Vec::new(),
        },
    )
    .unwrap();

    assert_eq!(artifact.proof_status, TrajectoryProofStatus::NoChecksRun);
    assert_eq!(
        artifact.verdicts.completion_proof,
        TrajectoryVerdict::Unsupported
    );
    assert_eq!(load_trajectory(temp.path(), "run_1").unwrap(), artifact);
}

#[test]
fn new_run_replaces_the_single_cookie_but_same_run_is_rejected() {
    let temp = tempdir().unwrap();
    let init = |run_id: &str| TrajectoryInit {
        run_id: run_id.to_string(),
        task_id: None,
        intent_id: None,
        original_intent: "original".to_string(),
        derived_intent: "derived".to_string(),
        active_boundaries: vec!["src/**".to_string()],
        repo_scope: vec!["src/decapod/lib.rs".to_string()],
        destination: None,
        current_phase: None,
        next_transitions: Vec::new(),
        blockers: Vec::new(),
    };

    init_trajectory(temp.path(), init("run_old")).unwrap();
    let replacement = init_trajectory(temp.path(), init("run_new")).unwrap();
    assert_eq!(replacement.run_id, "run_new");
    assert!(load_trajectory(temp.path(), "run_old").is_err());
    assert_eq!(
        load_trajectory(temp.path(), "run_new").unwrap(),
        replacement
    );

    let same_run = init_trajectory(temp.path(), init("run_new"));
    assert!(same_run.is_err());
}

#[test]
fn trajectory_proof_status_distinguishes_check_outcomes() {
    let temp = tempdir().unwrap();
    init_trajectory(
        temp.path(),
        TrajectoryInit {
            run_id: "run_2".to_string(),
            task_id: None,
            intent_id: None,
            original_intent: "original".to_string(),
            derived_intent: "derived".to_string(),
            active_boundaries: vec!["src/**".to_string()],
            repo_scope: vec!["src/decapod/lib.rs".to_string()],
            destination: None,
            current_phase: None,
            next_transitions: Vec::new(),
            blockers: Vec::new(),
        },
    )
    .unwrap();

    let partial = record_trajectory(
        temp.path(),
        "run_2",
        TrajectoryUpdate {
            checks: vec![
                TrajectoryCheck {
                    name: "cargo test".to_string(),
                    status: TrajectoryCheckStatus::Passed,
                },
                TrajectoryCheck {
                    name: "cargo clippy".to_string(),
                    status: TrajectoryCheckStatus::Unavailable,
                },
            ],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(partial.proof_status, TrajectoryProofStatus::Partial);

    let failed = record_trajectory(
        temp.path(),
        "run_2",
        TrajectoryUpdate {
            checks: vec![TrajectoryCheck {
                name: "cargo test".to_string(),
                status: TrajectoryCheckStatus::Failed,
            }],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(failed.proof_status, TrajectoryProofStatus::Failed);
}

#[test]
fn shortcut_signal_emits_caution_and_completion_claim_is_not_proof() {
    let temp = tempdir().unwrap();
    init_trajectory(
        temp.path(),
        TrajectoryInit {
            run_id: "run_3".to_string(),
            task_id: None,
            intent_id: None,
            original_intent: "original".to_string(),
            derived_intent: "derived".to_string(),
            active_boundaries: vec!["src/**".to_string()],
            repo_scope: vec!["src/decapod/lib.rs".to_string()],
            destination: None,
            current_phase: None,
            next_transitions: Vec::new(),
            blockers: Vec::new(),
        },
    )
    .unwrap();
    let artifact = record_trajectory(
        temp.path(),
        "run_3",
        TrajectoryUpdate {
            completion_claim: Some("done".to_string()),
            shortcut_risk_signals: vec!["completion claimed before checks".to_string()],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(artifact.completion_claim.as_deref(), Some("done"));
    assert_eq!(artifact.proof_status, TrajectoryProofStatus::NoChecksRun);
    assert_eq!(artifact.verdicts.shortcut_risk, TrajectoryVerdict::Caution);
    assert_eq!(
        artifact.verdicts.completion_proof,
        TrajectoryVerdict::Unsupported
    );
}

#[test]
fn check_specs_are_typed_and_reject_unknown_statuses() {
    assert_eq!(
        parse_check_spec("cargo test=passed").unwrap().status,
        TrajectoryCheckStatus::Passed
    );
    assert!(parse_check_spec("cargo test=maybe").is_err());
}

#[test]
fn motion_context_is_recorded_and_state_stays_proof_backed() {
    let temp = tempdir().unwrap();
    init_trajectory(
        temp.path(),
        TrajectoryInit {
            run_id: "run_motion".to_string(),
            task_id: None,
            intent_id: None,
            original_intent: "original".to_string(),
            derived_intent: "derived".to_string(),
            active_boundaries: vec!["src/**".to_string()],
            repo_scope: vec!["src/decapod/lib.rs".to_string()],
            destination: Some("published PR".to_string()),
            current_phase: Some("implementation".to_string()),
            next_transitions: vec!["validate".to_string(), "publish".to_string()],
            blockers: vec!["awaiting CI".to_string()],
        },
    )
    .unwrap();
    let blocked = load_trajectory(temp.path(), "run_motion").unwrap();
    assert_eq!(motion_state(&blocked), TrajectoryMotionState::Blocked);

    let completed = record_trajectory(
        temp.path(),
        "run_motion",
        TrajectoryUpdate {
            blockers: Vec::new(),
            clear_blockers: true,
            checks: vec![TrajectoryCheck {
                name: "validate".to_string(),
                status: TrajectoryCheckStatus::Passed,
            }],
            completion_claim: Some("complete".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(motion_state(&completed), TrajectoryMotionState::Completed);
    assert_eq!(completed.destination.as_deref(), Some("published PR"));
    assert_eq!(completed.current_phase.as_deref(), Some("implementation"));
    assert_eq!(completed.next_transitions, vec!["publish", "validate"]);
}

#[allow(clippy::too_many_arguments)]
fn loop_record(
    run_id: &str,
    intent_id: &str,
    loop_id: &str,
    loop_type: TrajectoryLoopType,
    attempt: u32,
    grader_result: TrajectoryGraderResult,
    feedback: &str,
    proof_refs: Vec<&str>,
    mutation_proposal: TrajectoryMutationProposal,
    status: TrajectoryLoopStatus,
) -> TrajectoryLoop {
    TrajectoryLoop {
        intent_id: intent_id.to_string(),
        trajectory_id: run_id.to_string(),
        loop_id: loop_id.to_string(),
        loop_type,
        attempt,
        parent_loop_id: None,
        trigger: TrajectoryTrigger::Human,
        tool_calls: vec!["agent.call".to_string()],
        observations: vec!["bounded observation".to_string()],
        grader_result,
        feedback: feedback.to_string(),
        proof_refs: proof_refs.into_iter().map(str::to_string).collect(),
        mutation_proposal,
        status,
        custody_event_id: None,
    }
}

#[test]
fn loop_attempts_preserve_feedback_and_proof_custody() {
    let temp = tempdir().unwrap();
    let intent_id = "intent:human-request";
    init_trajectory(
        temp.path(),
        TrajectoryInit {
            run_id: "run_loops".to_string(),
            task_id: None,
            intent_id: Some(intent_id.to_string()),
            original_intent: "human request".to_string(),
            derived_intent: "bounded loop execution".to_string(),
            active_boundaries: vec!["src/**".to_string()],
            repo_scope: vec!["src/decapod/lib.rs".to_string()],
            destination: None,
            current_phase: None,
            next_transitions: Vec::new(),
            blockers: Vec::new(),
        },
    )
    .unwrap();
    let artifact = record_trajectory(
        temp.path(),
        "run_loops",
        TrajectoryUpdate {
            loops: vec![
                loop_record(
                    "run_loops",
                    intent_id,
                    "verify",
                    TrajectoryLoopType::Verification,
                    1,
                    TrajectoryGraderResult::Fail,
                    "assertion mismatch; rerun the bounded check",
                    Vec::new(),
                    TrajectoryMutationProposal::None,
                    TrajectoryLoopStatus::Retrying,
                ),
                loop_record(
                    "run_loops",
                    intent_id,
                    "verify",
                    TrajectoryLoopType::Verification,
                    2,
                    TrajectoryGraderResult::Pass,
                    "",
                    vec!["check:cargo-test", "artifact:trajectory"],
                    TrajectoryMutationProposal::None,
                    TrajectoryLoopStatus::Passed,
                ),
            ],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(artifact.loops.len(), 2);
    assert_eq!(artifact.loops[0].attempt, 1);
    assert_eq!(artifact.loops[0].feedback.len(), 43);
    assert_eq!(artifact.loops[1].proof_refs.len(), 2);
}

#[test]
fn event_and_improvement_loops_remain_inside_intent_boundary() {
    let temp = tempdir().unwrap();
    let intent_id = "intent:boundary";
    init_trajectory(
        temp.path(),
        TrajectoryInit {
            run_id: "run_event".to_string(),
            task_id: None,
            intent_id: Some(intent_id.to_string()),
            original_intent: "original".to_string(),
            derived_intent: "event follow-up".to_string(),
            active_boundaries: vec!["src/**".to_string()],
            repo_scope: vec!["src/decapod/lib.rs".to_string()],
            destination: None,
            current_phase: None,
            next_transitions: Vec::new(),
            blockers: Vec::new(),
        },
    )
    .unwrap();
    let mut event = loop_record(
        "run_event",
        intent_id,
        "event",
        TrajectoryLoopType::Event,
        1,
        TrajectoryGraderResult::Skipped,
        "",
        Vec::new(),
        TrajectoryMutationProposal::None,
        TrajectoryLoopStatus::Open,
    );
    event.trigger = TrajectoryTrigger::Webhook;
    let mut improvement = loop_record(
        "run_event",
        intent_id,
        "improve",
        TrajectoryLoopType::Improvement,
        1,
        TrajectoryGraderResult::Skipped,
        "candidate harness change",
        Vec::new(),
        TrajectoryMutationProposal::Rubric,
        TrajectoryLoopStatus::Open,
    );
    improvement.parent_loop_id = Some("event".to_string());
    let artifact = record_trajectory(
        temp.path(),
        "run_event",
        TrajectoryUpdate {
            loops: vec![event, improvement],
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        artifact
            .loops
            .iter()
            .all(|loop_record| loop_record.intent_id == intent_id)
    );
    assert_eq!(
        artifact.loops[1].mutation_proposal,
        TrajectoryMutationProposal::Rubric
    );
}

#[test]
fn loop_validation_rejects_unproven_passes_and_unbounded_feedback() {
    let temp = tempdir().unwrap();
    init_trajectory(
        temp.path(),
        TrajectoryInit {
            run_id: "run_invalid_loop".to_string(),
            task_id: None,
            intent_id: None,
            original_intent: "original".to_string(),
            derived_intent: "derived".to_string(),
            active_boundaries: vec!["src/**".to_string()],
            repo_scope: vec!["src/decapod/lib.rs".to_string()],
            destination: None,
            current_phase: None,
            next_transitions: Vec::new(),
            blockers: Vec::new(),
        },
    )
    .unwrap();
    let invalid = loop_record(
        "run_invalid_loop",
        "intent:run_invalid_loop",
        "verify",
        TrajectoryLoopType::Verification,
        1,
        TrajectoryGraderResult::Pass,
        "",
        Vec::new(),
        TrajectoryMutationProposal::None,
        TrajectoryLoopStatus::Passed,
    );
    let error = record_trajectory(
        temp.path(),
        "run_invalid_loop",
        TrajectoryUpdate {
            loops: vec![invalid],
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("requires at least one proof reference")
    );
}

#[test]
fn trajectory_execution_carries_intent_custody_across_records() {
    let temp = tempdir().unwrap();
    let intent_id = "intent:custody-flow";
    init_trajectory(
        temp.path(),
        TrajectoryInit {
            run_id: "run_custody".to_string(),
            task_id: Some("task_custody".to_string()),
            intent_id: Some(intent_id.to_string()),
            original_intent: "preserve the human request exactly".to_string(),
            derived_intent: "record one bounded execution step".to_string(),
            active_boundaries: vec!["src/**".to_string()],
            repo_scope: vec!["src/decapod/core/trajectory.rs".to_string()],
            destination: None,
            current_phase: Some("refinement".to_string()),
            next_transitions: vec!["approve".to_string()],
            blockers: Vec::new(),
        },
    )
    .unwrap();

    let artifact = record_trajectory(
        temp.path(),
        "run_custody",
        TrajectoryUpdate {
            loops: vec![loop_record(
                "run_custody",
                intent_id,
                "custody-step",
                TrajectoryLoopType::Agent,
                1,
                TrajectoryGraderResult::Skipped,
                "",
                Vec::new(),
                TrajectoryMutationProposal::None,
                TrajectoryLoopStatus::Open,
            )],
            ..Default::default()
        },
    )
    .unwrap();

    let record = &artifact.custody.intents[intent_id];
    assert_eq!(record.raw_intent, "preserve the human request exactly");
    assert_eq!(record.status, crate::core::custody::IntentStatus::Refined);
    assert_eq!(artifact.custody.events.len(), 3);
    assert_eq!(artifact.custody.trajectories["run_custody"].steps.len(), 1);
    assert_eq!(
        artifact.loops[0].custody_event_id.as_deref(),
        Some(artifact.custody.events[2].event_id.as_str())
    );
    assert_eq!(
        load_trajectory(temp.path(), "run_custody").unwrap(),
        artifact
    );
}
