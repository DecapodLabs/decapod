// Moved from src/decapod/core/governance_artifacts.rs
use super::*;
use crate::core::custody::CustodyState;
use crate::core::trajectory::{
    TrajectoryArtifact, TrajectoryProofStatus, TrajectoryVerdict, TrajectoryVerdicts,
};
use crate::core::validate::{ValidationCiPrediction, ValidationReceipt};
use crate::core::validation_epoch::ValidationEpochMetadata;
use std::collections::BTreeMap;

fn trajectory() -> TrajectoryArtifact {
    TrajectoryArtifact {
        schema_version: "1.1.0".to_string(),
        run_id: "run-receipt-freshness".to_string(),
        intent_id: Some("intent-receipt-freshness".to_string()),
        task_id: Some("bugs_test".to_string()),
        original_intent: "test receipt freshness".to_string(),
        derived_intent: "test receipt freshness".to_string(),
        destination: None,
        current_phase: None,
        next_transitions: Vec::new(),
        blockers: Vec::new(),
        active_boundaries: Vec::new(),
        repo_scope: Vec::new(),
        inspected_files: Vec::new(),
        modified_files: Vec::new(),
        declared_commands: Vec::new(),
        tool_calls: Vec::new(),
        loops: Vec::new(),
        checks: Vec::new(),
        evidence: Vec::new(),
        shortcut_risk_signals: Vec::new(),
        unresolved_assumptions: Vec::new(),
        completion_claim: None,
        proof_status: TrajectoryProofStatus::NoChecksRun,
        verdicts: TrajectoryVerdicts {
            intent_alignment: TrajectoryVerdict::Unassessed,
            boundary_discipline: TrajectoryVerdict::Unassessed,
            shortcut_risk: TrajectoryVerdict::Supported,
            completion_proof: TrajectoryVerdict::Unsupported,
        },
        artifact_hash: "sha256:trajectory-proof".to_string(),
        custody: CustodyState::default(),
    }
}

fn receipt_for(trajectory: &TrajectoryArtifact) -> ValidationReceipt {
    ValidationReceipt {
        schema_version: "1.0.0".to_string(),
        kind: "validation_receipt".to_string(),
        decapod_release: "0.89.2".to_string(),
        // Validation runs before the commit that carries its receipt.
        git_revision: "pre-commit-revision".to_string(),
        repo_signal_fingerprint: "sha256:repo-signal".to_string(),
        trajectory_run_id: Some(trajectory.run_id.clone()),
        trajectory_artifact_hash: Some(trajectory.artifact_hash.clone()),
        validation_epoch: ValidationEpochMetadata {
            schema_version: "1.0.0".to_string(),
            epoch_id: "epoch-receipt-freshness".to_string(),
            evaluator_identity: "test".to_string(),
            evaluator_set_hash: "sha256:evaluator-set".to_string(),
            constitution_version: "test".to_string(),
            constitution_hash: "sha256:constitution".to_string(),
            validation_profile: "test".to_string(),
            validation_profile_hash: "sha256:profile".to_string(),
            proof_rubric: "test".to_string(),
            proof_rubric_hash: "sha256:rubric".to_string(),
            generated_specs_manifest_hash: "sha256:manifest".to_string(),
            generated_specs_fingerprint: "sha256:specs".to_string(),
            material_hashes: BTreeMap::new(),
        },
        status: "ok".to_string(),
        pass_count: 1,
        fail_count: 0,
        warn_count: 0,
        elapsed_ms: 1,
        drift_findings: Vec::new(),
        temporary_artifacts_cleaned: 0,
        failures: Some(Vec::new()),
        warnings: Some(Vec::new()),
        gate_timings: Some(Vec::new()),
        parallelism: Some(1),
        ci_prediction: Some(ValidationCiPrediction {
            result: "pass".to_string(),
            confidence: "high".to_string(),
            reasons: Vec::new(),
            recommendations: Vec::new(),
        }),
        receipt_hash: String::new(),
    }
    .with_recomputed_hash()
    .expect("receipt hash")
}

#[test]
fn receipt_freshness_accepts_valid_precommit_receipt() {
    let trajectory = trajectory();
    let receipt = receipt_for(&trajectory);

    assert_eq!(
        receipt_freshness(Some(&receipt), Some(&trajectory)),
        SemanticFreshness::Current
    );
}
