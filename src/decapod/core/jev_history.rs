//! Read-only compatibility for historical Jev observation evidence.
//!
//! No provider, transport, credential lookup, observation writer, or policy gate
//! is retained. Existing governance documents remain readable without changing
//! their archived results or requiring a current trajectory binding.

use crate::core::error::DecapodError;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

pub(crate) const JEV_HISTORY_SCHEMA_VERSION: &str = "1.0.0";
pub(crate) const JEV_HISTORY_KIND: &str = "jev_observation_ledger";
pub(crate) const JEV_HISTORY_SCHEMA_URI: &str =
    "https://decapod.dev/schemas/jev-observations-1.0.0.schema.json";

/// Historical observation payload, retained only to read existing evidence.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub(crate) struct DecisionObservation {
    pub(crate) kind: String,
    pub(crate) probability: f64,
    pub(crate) provider: String,
    pub(crate) model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) usage: Option<DecisionUsage>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct DecisionUsage {
    pub(crate) input_tokens: u64,
    pub(crate) output_tokens: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NoObservationReason {
    Disabled,
    Unconfigured,
    MissingIntent,
    MissingTrajectory,
    Unavailable,
    MalformedResponse,
    InvalidObservation,
    Configuration,
    Persistence,
}

/// Historical result payload; never consulted for current readiness.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum DecisionObservationResult {
    Observed {
        observation: DecisionObservation,
    },
    NoObservation {
        provider: String,
        reason: NoObservationReason,
    },
}

const TRAJECTORY_SATISFIES_INTENT: &str = "trajectory_satisfies_intent";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct JevObservationLedger {
    #[serde(rename = "$schema")]
    pub(crate) schema_uri: String,
    pub(crate) schema_version: String,
    pub(crate) kind: String,
    pub(crate) trajectory_run_id: String,
    /// Keys are stable per-run observation IDs. The values retain the typed
    /// historical result without retaining a live provider interface.
    pub(crate) runs: BTreeMap<String, JevRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct JevRun {
    pub(crate) id: String,
    pub(crate) sequence: u64,
    pub(crate) recorded_at: String,
    pub(crate) operation: String,
    pub(crate) touched_paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) diff_summary: Option<String>,
    pub(crate) result: DecisionObservationResult,
}

pub(crate) fn validate(ledger: &JevObservationLedger) -> Result<(), DecapodError> {
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
