//! Local-first run trajectory artifacts.
//!
//! A trajectory records the evidence needed to inspect an agent run beyond
//! its final diff. It is deliberately explicit and append-by-update: agents
//! declare intent, boundaries, context, actions, checks, assumptions, and
//! completion claims while Decapod computes proof status and lightweight
//! verdicts from the recorded evidence.

use crate::core::error;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

pub const TRAJECTORY_SCHEMA_VERSION: &str = "1.0.0";
pub const TRAJECTORY_DIR: &str = ".decapod/governance/trajectories";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum TrajectoryCheckStatus {
    Passed,
    Failed,
    Partial,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct TrajectoryCheck {
    pub name: String,
    pub status: TrajectoryCheckStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrajectoryProofStatus {
    Passed,
    Failed,
    Partial,
    Unavailable,
    NoChecksRun,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrajectoryVerdict {
    Supported,
    Caution,
    Unsupported,
    Unassessed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrajectoryVerdicts {
    pub intent_alignment: TrajectoryVerdict,
    pub boundary_discipline: TrajectoryVerdict,
    pub shortcut_risk: TrajectoryVerdict,
    pub completion_proof: TrajectoryVerdict,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrajectoryArtifact {
    pub schema_version: String,
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub original_intent: String,
    pub derived_intent: String,
    pub active_boundaries: Vec<String>,
    pub repo_scope: Vec<String>,
    pub inspected_files: Vec<String>,
    pub modified_files: Vec<String>,
    pub declared_commands: Vec<String>,
    pub tool_calls: Vec<String>,
    pub checks: Vec<TrajectoryCheck>,
    pub evidence: Vec<String>,
    pub shortcut_risk_signals: Vec<String>,
    pub unresolved_assumptions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_claim: Option<String>,
    pub proof_status: TrajectoryProofStatus,
    pub verdicts: TrajectoryVerdicts,
    pub artifact_hash: String,
}

#[derive(Debug, Default)]
pub struct TrajectoryUpdate {
    pub active_boundaries: Vec<String>,
    pub repo_scope: Vec<String>,
    pub inspected_files: Vec<String>,
    pub modified_files: Vec<String>,
    pub declared_commands: Vec<String>,
    pub tool_calls: Vec<String>,
    pub checks: Vec<TrajectoryCheck>,
    pub evidence: Vec<String>,
    pub shortcut_risk_signals: Vec<String>,
    pub unresolved_assumptions: Vec<String>,
    pub completion_claim: Option<String>,
}

impl TrajectoryArtifact {
    pub fn canonicalized(&self) -> Self {
        let mut out = self.clone();
        for values in [
            &mut out.active_boundaries,
            &mut out.repo_scope,
            &mut out.inspected_files,
            &mut out.modified_files,
            &mut out.declared_commands,
            &mut out.tool_calls,
            &mut out.evidence,
            &mut out.shortcut_risk_signals,
            &mut out.unresolved_assumptions,
        ] {
            values.sort();
            values.dedup();
        }
        out.checks.sort();
        out.checks.dedup_by(|left, right| left.name == right.name);
        out.proof_status = compute_proof_status(&out.checks);
        out.verdicts = compute_verdicts(&out);
        out.artifact_hash.clear();
        out
    }

    pub fn canonical_json_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&self.canonicalized())
    }

    pub fn computed_hash_hex(&self) -> Result<String, serde_json::Error> {
        let mut hasher = Sha256::new();
        hasher.update(self.canonical_json_bytes()?);
        Ok(format!("sha256:{:x}", hasher.finalize()))
    }

    pub fn with_recomputed_hash(&self) -> Result<Self, serde_json::Error> {
        let mut out = self.canonicalized();
        out.artifact_hash = out.computed_hash_hex()?;
        Ok(out)
    }
}

pub fn trajectory_path(project_root: &Path, run_id: &str) -> Result<PathBuf, error::DecapodError> {
    validate_run_id(run_id)?;
    Ok(project_root
        .join(TRAJECTORY_DIR)
        .join(format!("{run_id}.json")))
}

pub fn validate_run_id(run_id: &str) -> Result<(), error::DecapodError> {
    if run_id.is_empty()
        || !run_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        })
    {
        return Err(error::DecapodError::ValidationError(format!(
            "invalid run_id '{run_id}': allowed characters are [A-Za-z0-9_-]"
        )));
    }
    Ok(())
}

pub fn init_trajectory(
    project_root: &Path,
    run_id: &str,
    task_id: Option<String>,
    original_intent: String,
    derived_intent: String,
    active_boundaries: Vec<String>,
    repo_scope: Vec<String>,
) -> Result<TrajectoryArtifact, error::DecapodError> {
    let path = trajectory_path(project_root, run_id)?;
    if path.exists() {
        return Err(error::DecapodError::ValidationError(format!(
            "trajectory '{run_id}' already exists"
        )));
    }
    if original_intent.trim().is_empty() || derived_intent.trim().is_empty() {
        return Err(error::DecapodError::ValidationError(
            "trajectory requires non-empty original_intent and derived_intent".to_string(),
        ));
    }

    let artifact = TrajectoryArtifact {
        schema_version: TRAJECTORY_SCHEMA_VERSION.to_string(),
        run_id: run_id.to_string(),
        task_id,
        original_intent,
        derived_intent,
        active_boundaries,
        repo_scope,
        inspected_files: Vec::new(),
        modified_files: Vec::new(),
        declared_commands: Vec::new(),
        tool_calls: Vec::new(),
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
        artifact_hash: String::new(),
    };
    write_trajectory(project_root, &artifact)
}

pub fn load_trajectory(
    project_root: &Path,
    run_id: &str,
) -> Result<TrajectoryArtifact, error::DecapodError> {
    let path = trajectory_path(project_root, run_id)?;
    if !path.exists() {
        return Err(error::DecapodError::NotFound(format!(
            "trajectory '{run_id}' not found at {}",
            path.display()
        )));
    }
    let raw = fs::read_to_string(&path).map_err(error::DecapodError::IoError)?;
    let artifact: TrajectoryArtifact = serde_json::from_str(&raw).map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "invalid trajectory artifact {}: {e}",
            path.display()
        ))
    })?;
    if artifact.schema_version != TRAJECTORY_SCHEMA_VERSION {
        return Err(error::DecapodError::ValidationError(format!(
            "unsupported trajectory schema version '{}'",
            artifact.schema_version
        )));
    }
    if artifact.run_id != run_id {
        return Err(error::DecapodError::ValidationError(format!(
            "trajectory artifact run_id '{}' does not match requested '{run_id}'",
            artifact.run_id
        )));
    }
    let expected_hash = artifact.computed_hash_hex().map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "failed to compute trajectory artifact hash: {e}"
        ))
    })?;
    if artifact.artifact_hash != expected_hash {
        return Err(error::DecapodError::ValidationError(format!(
            "trajectory artifact hash mismatch: expected {expected_hash}, found {}",
            artifact.artifact_hash
        )));
    }
    Ok(artifact)
}

pub fn write_trajectory(
    project_root: &Path,
    artifact: &TrajectoryArtifact,
) -> Result<TrajectoryArtifact, error::DecapodError> {
    let path = trajectory_path(project_root, &artifact.run_id)?;
    let canonical = artifact.with_recomputed_hash().map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "failed to serialize trajectory artifact: {e}"
        ))
    })?;
    let parent = path.parent().ok_or_else(|| {
        error::DecapodError::ValidationError("invalid trajectory parent path".to_string())
    })?;
    fs::create_dir_all(parent).map_err(error::DecapodError::IoError)?;
    let bytes = serde_json::to_vec_pretty(&canonical).map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "failed to serialize trajectory artifact: {e}"
        ))
    })?;
    fs::write(&path, bytes).map_err(error::DecapodError::IoError)?;
    Ok(canonical)
}

pub fn record_trajectory(
    project_root: &Path,
    run_id: &str,
    update: TrajectoryUpdate,
) -> Result<TrajectoryArtifact, error::DecapodError> {
    let mut artifact = load_trajectory(project_root, run_id)?;
    artifact.active_boundaries.extend(update.active_boundaries);
    artifact.repo_scope.extend(update.repo_scope);
    artifact.inspected_files.extend(update.inspected_files);
    artifact.modified_files.extend(update.modified_files);
    artifact.declared_commands.extend(update.declared_commands);
    artifact.tool_calls.extend(update.tool_calls);
    for check in &update.checks {
        artifact
            .checks
            .retain(|existing| existing.name != check.name);
    }
    artifact.checks.extend(update.checks);
    artifact.evidence.extend(update.evidence);
    artifact
        .shortcut_risk_signals
        .extend(update.shortcut_risk_signals);
    artifact
        .unresolved_assumptions
        .extend(update.unresolved_assumptions);
    if update.completion_claim.is_some() {
        artifact.completion_claim = update.completion_claim;
    }
    write_trajectory(project_root, &artifact)
}

pub fn parse_check_spec(spec: &str) -> Result<TrajectoryCheck, error::DecapodError> {
    let (name, status) = spec.split_once('=').ok_or_else(|| {
        error::DecapodError::ValidationError(format!(
            "invalid trajectory check '{spec}': expected name=status"
        ))
    })?;
    let name = name.trim();
    if name.is_empty() {
        return Err(error::DecapodError::ValidationError(
            "trajectory check name cannot be empty".to_string(),
        ));
    }
    let status = match status.trim().to_ascii_lowercase().as_str() {
        "pass" | "passed" => TrajectoryCheckStatus::Passed,
        "fail" | "failed" => TrajectoryCheckStatus::Failed,
        "partial" => TrajectoryCheckStatus::Partial,
        "unavailable" => TrajectoryCheckStatus::Unavailable,
        other => {
            return Err(error::DecapodError::ValidationError(format!(
                "invalid trajectory check status '{other}': expected passed|failed|partial|unavailable"
            )));
        }
    };
    Ok(TrajectoryCheck {
        name: name.to_string(),
        status,
    })
}

fn compute_proof_status(checks: &[TrajectoryCheck]) -> TrajectoryProofStatus {
    if checks.is_empty() {
        return TrajectoryProofStatus::NoChecksRun;
    }
    if checks
        .iter()
        .all(|check| check.status == TrajectoryCheckStatus::Passed)
    {
        return TrajectoryProofStatus::Passed;
    }
    if checks
        .iter()
        .any(|check| check.status == TrajectoryCheckStatus::Failed)
    {
        return TrajectoryProofStatus::Failed;
    }
    if checks
        .iter()
        .any(|check| check.status == TrajectoryCheckStatus::Partial)
    {
        return TrajectoryProofStatus::Partial;
    }
    if checks
        .iter()
        .all(|check| check.status == TrajectoryCheckStatus::Unavailable)
    {
        return TrajectoryProofStatus::Unavailable;
    }
    TrajectoryProofStatus::Partial
}

fn compute_verdicts(artifact: &TrajectoryArtifact) -> TrajectoryVerdicts {
    let intent_alignment = if artifact.original_intent.trim().is_empty()
        || artifact.derived_intent.trim().is_empty()
    {
        TrajectoryVerdict::Unassessed
    } else {
        TrajectoryVerdict::Supported
    };
    let boundary_discipline =
        if artifact.active_boundaries.is_empty() || artifact.repo_scope.is_empty() {
            TrajectoryVerdict::Unassessed
        } else {
            TrajectoryVerdict::Supported
        };
    let shortcut_risk = if artifact.shortcut_risk_signals.is_empty() {
        TrajectoryVerdict::Supported
    } else {
        TrajectoryVerdict::Caution
    };
    let completion_proof = match artifact.proof_status {
        TrajectoryProofStatus::Passed => TrajectoryVerdict::Supported,
        TrajectoryProofStatus::Partial | TrajectoryProofStatus::Unavailable => {
            TrajectoryVerdict::Caution
        }
        TrajectoryProofStatus::Failed | TrajectoryProofStatus::NoChecksRun => {
            TrajectoryVerdict::Unsupported
        }
    };
    TrajectoryVerdicts {
        intent_alignment,
        boundary_discipline,
        shortcut_risk,
        completion_proof,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn trajectory_creation_is_inspectable_and_unproven_without_checks() {
        let temp = tempdir().unwrap();
        let artifact = init_trajectory(
            temp.path(),
            "run_1",
            Some("task_1".to_string()),
            "original intent".to_string(),
            "derived intent".to_string(),
            vec!["src/**".to_string()],
            vec!["src/lib.rs".to_string()],
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
    fn trajectory_proof_status_distinguishes_check_outcomes() {
        let temp = tempdir().unwrap();
        init_trajectory(
            temp.path(),
            "run_2",
            None,
            "original".to_string(),
            "derived".to_string(),
            vec!["src/**".to_string()],
            vec!["src/lib.rs".to_string()],
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
            "run_3",
            None,
            "original".to_string(),
            "derived".to_string(),
            vec!["src/**".to_string()],
            vec!["src/lib.rs".to_string()],
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
}
