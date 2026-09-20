//! Durable, provider-neutral custody for Jev observations.
//!
//! The ledger is a repository artifact, not a policy input. It groups every
//! Jev result produced during the active trajectory run so Git can preserve
//! the evidence with the PR. A new trajectory run replaces the active ledger;
//! prior committed ledgers remain recoverable through Git history.

use crate::core::atomic;
use crate::core::decision_provider::{
    DecisionObservationResult, NoObservationReason, TRAJECTORY_SATISFIES_INTENT,
};
use crate::core::error::DecapodError;
use crate::core::path_policy;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

pub const JEV_HISTORY_PATH: &str = ".decapod/governance/jev.json";
pub const JEV_HISTORY_SCHEMA_VERSION: &str = "1.0.0";
pub const JEV_HISTORY_KIND: &str = "jev_observation_ledger";
pub const JEV_HISTORY_SCHEMA_URI: &str =
    "https://decapod.dev/schemas/jev-observations-1.0.0.schema.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct JevObservationLedger {
    #[serde(rename = "$schema")]
    pub schema_uri: String,
    pub schema_version: String,
    pub kind: String,
    pub trajectory_run_id: String,
    /// Keys are stable per-run observation IDs. The values retain the typed
    /// provider result without exposing Jev's HTTP request/response schema.
    pub runs: BTreeMap<String, JevRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct JevRun {
    pub id: String,
    pub sequence: u64,
    pub recorded_at: String,
    pub operation: String,
    pub touched_paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_summary: Option<String>,
    pub result: DecisionObservationResult,
}

pub fn load_and_validate(repo_root: &Path) -> Result<Option<JevObservationLedger>, DecapodError> {
    let path = repo_root.join(JEV_HISTORY_PATH);
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path).map_err(DecapodError::IoError)?;
    let ledger: JevObservationLedger = serde_json::from_str(&raw).map_err(|error| {
        DecapodError::ValidationError(format!(
            "invalid Jev observation ledger {}: {error}",
            path.display()
        ))
    })?;
    validate(&ledger)?;
    Ok(Some(ledger))
}

/// Remove a prior run's working-tree ledger when the trajectory cookie starts
/// a new run. This is intentionally a narrow reset: Git history remains the
/// recovery surface for the committed prior ledger.
pub fn reset_for_trajectory(repo_root: &Path, trajectory_run_id: &str) -> Result<(), DecapodError> {
    if trajectory_run_id.trim().is_empty() {
        return Err(DecapodError::ValidationError(
            "Jev observation ledger requires a non-empty trajectory run id".to_string(),
        ));
    }
    let path = repo_root.join(JEV_HISTORY_PATH);
    if !path.exists() {
        return Ok(());
    }
    let Some(existing) = load_and_validate(repo_root)? else {
        return Ok(());
    };
    if existing.trajectory_run_id != trajectory_run_id {
        fs::remove_file(path).map_err(DecapodError::IoError)?;
    }
    Ok(())
}

pub fn append(
    repo_root: &Path,
    trajectory_run_id: &str,
    operation: &str,
    touched_paths: &[String],
    diff_summary: Option<&str>,
    result: DecisionObservationResult,
) -> Result<(), DecapodError> {
    if trajectory_run_id.trim().is_empty() {
        return Err(DecapodError::ValidationError(
            "Jev observation ledger requires a non-empty trajectory run id".to_string(),
        ));
    }
    let path = repo_root.join(JEV_HISTORY_PATH);
    let mut ledger = match load_and_validate(repo_root)? {
        Some(existing) if existing.trajectory_run_id == trajectory_run_id => existing,
        Some(_) | None => new_ledger(trajectory_run_id),
    };
    let id = crate::core::ulid::new_ulid();
    let sequence = ledger
        .runs
        .values()
        .map(|run| run.sequence)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    ledger.runs.insert(
        id.clone(),
        JevRun {
            id,
            sequence,
            recorded_at: crate::core::time::now_epoch_z(),
            operation: operation.to_string(),
            touched_paths: touched_paths
                .iter()
                .map(|path| path_policy::normalize_persisted_path(repo_root, path))
                .collect(),
            diff_summary: diff_summary.map(|summary| path_policy::redact_text(repo_root, summary)),
            result,
        },
    );
    validate(&ledger)?;
    let bytes = serde_json::to_vec_pretty(&ledger).map_err(|error| {
        DecapodError::ValidationError(format!("serialize Jev observation ledger: {error}"))
    })?;
    atomic::write_atomic(&path, &bytes).map_err(DecapodError::IoError)
}

fn new_ledger(trajectory_run_id: &str) -> JevObservationLedger {
    JevObservationLedger {
        schema_uri: JEV_HISTORY_SCHEMA_URI.to_string(),
        schema_version: JEV_HISTORY_SCHEMA_VERSION.to_string(),
        kind: JEV_HISTORY_KIND.to_string(),
        trajectory_run_id: trajectory_run_id.to_string(),
        runs: BTreeMap::new(),
    }
}

fn validate(ledger: &JevObservationLedger) -> Result<(), DecapodError> {
    if ledger.schema_uri != JEV_HISTORY_SCHEMA_URI
        || ledger.schema_version != JEV_HISTORY_SCHEMA_VERSION
        || ledger.kind != JEV_HISTORY_KIND
    {
        return Err(DecapodError::ValidationError(
            "Jev observation ledger schema identity is invalid".to_string(),
        ));
    }
    if ledger.trajectory_run_id.trim().is_empty() {
        return Err(DecapodError::ValidationError(
            "Jev observation ledger trajectory_run_id is empty".to_string(),
        ));
    }
    let mut sequences = HashSet::new();
    for (key, run) in &ledger.runs {
        if key != &run.id || run.id.trim().is_empty() || run.sequence == 0 {
            return Err(DecapodError::ValidationError(
                "Jev observation ledger contains an invalid run identity".to_string(),
            ));
        }
        if !sequences.insert(run.sequence) {
            return Err(DecapodError::ValidationError(
                "Jev observation ledger contains duplicate run sequence values".to_string(),
            ));
        }
        match &run.result {
            DecisionObservationResult::Observed { observation } => {
                if observation.provider != "jev"
                    || observation.kind != TRAJECTORY_SATISFIES_INTENT
                    || !observation.probability.is_finite()
                    || !(0.0..=1.0).contains(&observation.probability)
                {
                    return Err(DecapodError::ValidationError(
                        "Jev observation ledger contains an invalid observation".to_string(),
                    ));
                }
            }
            DecisionObservationResult::NoObservation { provider, reason } => {
                if provider != "jev" {
                    return Err(DecapodError::ValidationError(
                        "Jev observation ledger contains a non-Jev result".to_string(),
                    ));
                }
                if matches!(reason, NoObservationReason::Disabled) {
                    return Err(DecapodError::ValidationError(
                        "disabled provider results do not belong in the Jev ledger".to_string(),
                    ));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/unit/core/jev_history_tests.rs"]
mod tests;
