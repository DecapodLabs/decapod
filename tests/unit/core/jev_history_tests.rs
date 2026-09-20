use super::*;
use crate::core::decision_provider::{
    DecisionObservation, DecisionObservationResult, NoObservationReason,
};
use std::fs;
use tempfile::tempdir;

fn observed(probability: f64) -> DecisionObservationResult {
    DecisionObservationResult::Observed {
        observation: DecisionObservation {
            kind: TRAJECTORY_SATISFIES_INTENT.to_string(),
            probability,
            provider: "jev".to_string(),
            model: Some("jev-latest".to_string()),
            usage: None,
        },
    }
}

fn paths() -> Vec<String> {
    vec!["src/lib.rs".to_string()]
}

#[test]
fn appends_multiple_typed_results_to_one_trajectory_ledger() {
    let temp = tempdir().expect("tempdir");
    append(
        temp.path(),
        "trajectory_new",
        "build",
        &paths(),
        Some("first pass"),
        observed(0.91),
    )
    .expect("first append");
    append(
        temp.path(),
        "trajectory_new",
        "verify",
        &paths(),
        None,
        DecisionObservationResult::NoObservation {
            provider: "jev".to_string(),
            reason: NoObservationReason::Unavailable,
        },
    )
    .expect("second append");

    let ledger = load_and_validate(temp.path())
        .expect("load ledger")
        .expect("ledger present");
    assert_eq!(ledger.trajectory_run_id, "trajectory_new");
    assert_eq!(ledger.runs.len(), 2);
    assert_eq!(
        ledger
            .runs
            .values()
            .map(|run| run.sequence)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert!(
        ledger
            .runs
            .values()
            .any(|run| matches!(run.result, DecisionObservationResult::Observed { .. }))
    );
    assert!(
        ledger
            .runs
            .values()
            .any(|run| matches!(run.result, DecisionObservationResult::NoObservation { .. }))
    );
}

#[test]
fn a_new_trajectory_removes_the_prior_working_tree_ledger() {
    let temp = tempdir().expect("tempdir");
    append(
        temp.path(),
        "trajectory_old",
        "build",
        &paths(),
        None,
        observed(0.4),
    )
    .expect("append");
    assert!(temp.path().join(JEV_HISTORY_PATH).is_file());

    reset_for_trajectory(temp.path(), "trajectory_new").expect("reset");
    assert!(!temp.path().join(JEV_HISTORY_PATH).exists());
}

#[test]
fn same_trajectory_preserves_history_and_corruption_fails_closed() {
    let temp = tempdir().expect("tempdir");
    append(
        temp.path(),
        "trajectory_same",
        "build",
        &paths(),
        None,
        observed(0.4),
    )
    .expect("append");
    reset_for_trajectory(temp.path(), "trajectory_same").expect("same-run reset");
    assert!(temp.path().join(JEV_HISTORY_PATH).is_file());

    fs::write(temp.path().join(JEV_HISTORY_PATH), b"not-json").expect("corrupt ledger");
    let error = reset_for_trajectory(temp.path(), "trajectory_new")
        .expect_err("corrupt history must not be discarded");
    assert!(error.to_string().contains("invalid Jev observation ledger"));
    assert_eq!(
        fs::read_to_string(temp.path().join(JEV_HISTORY_PATH)).expect("read ledger"),
        "not-json"
    );
}
