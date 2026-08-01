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

pub const TRAJECTORY_SCHEMA_VERSION: &str = "1.1.0";
pub const LEGACY_TRAJECTORY_SCHEMA_VERSION: &str = "1.0.0";
pub const TRAJECTORY_PATH: &str = ".decapod/governance/trajectory.json";
pub const MAX_LOOP_FEEDBACK_BYTES: usize = 2048;

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
#[serde(rename_all = "snake_case")]
pub enum TrajectoryMotionState {
    Active,
    Blocked,
    Waiting,
    Completed,
    /// Evidence shows repeated failed verification/agent loops without progress.
    Stalled,
    /// Completion was claimed but proof failed, or blockers mark abandonment.
    Abandoned,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrajectoryVerdicts {
    pub intent_alignment: TrajectoryVerdict,
    pub boundary_discipline: TrajectoryVerdict,
    pub shortcut_risk: TrajectoryVerdict,
    pub completion_proof: TrajectoryVerdict,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum TrajectoryLoopType {
    Agent,
    Verification,
    Event,
    Improvement,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum TrajectoryTrigger {
    Human,
    Webhook,
    Cron,
    Agent,
    Grader,
    Engine,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum TrajectoryGraderResult {
    Pass,
    Fail,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum TrajectoryMutationProposal {
    None,
    Prompt,
    Tool,
    Config,
    Rubric,
    Policy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum TrajectoryLoopStatus {
    Open,
    Retrying,
    Passed,
    Failed,
    Blocked,
}

/// A bounded, typed record for one logical loop attempt.
///
/// This is deliberately an evidence envelope rather than a runtime event
/// stream. Repeated records for the same loop_id use increasing attempt
/// numbers, allowing a later verifier to reconstruct retries from Git history
/// without storing full traces or mutating the harness automatically.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrajectoryLoop {
    pub intent_id: String,
    pub trajectory_id: String,
    pub loop_id: String,
    pub loop_type: TrajectoryLoopType,
    pub attempt: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_loop_id: Option<String>,
    pub trigger: TrajectoryTrigger,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observations: Vec<String>,
    pub grader_result: TrajectoryGraderResult,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub feedback: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proof_refs: Vec<String>,
    pub mutation_proposal: TrajectoryMutationProposal,
    pub status: TrajectoryLoopStatus,
    /// Optional link into the append-only intent custody ledger.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custody_event_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrajectoryArtifact {
    pub schema_version: String,
    pub run_id: String,
    /// Stable boundary identifier shared by all loops in this trajectory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub original_intent: String,
    pub derived_intent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_phase: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next_transitions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blockers: Vec<String>,
    pub active_boundaries: Vec<String>,
    pub repo_scope: Vec<String>,
    pub inspected_files: Vec<String>,
    pub modified_files: Vec<String>,
    pub declared_commands: Vec<String>,
    pub tool_calls: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loops: Vec<TrajectoryLoop>,
    pub checks: Vec<TrajectoryCheck>,
    pub evidence: Vec<String>,
    pub shortcut_risk_signals: Vec<String>,
    pub unresolved_assumptions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_claim: Option<String>,
    pub proof_status: TrajectoryProofStatus,
    pub verdicts: TrajectoryVerdicts,
    pub artifact_hash: String,
    /// Durable intent custody embedded in the existing trajectory envelope.
    /// The trajectory remains evidence; this state owns lifecycle authority.
    #[serde(
        default,
        skip_serializing_if = "crate::core::custody::CustodyState::is_empty"
    )]
    pub custody: crate::core::custody::CustodyState,
}

#[derive(Debug, Default)]
pub struct TrajectoryUpdate {
    pub task_id: Option<String>,
    pub destination: Option<String>,
    pub current_phase: Option<String>,
    pub next_transitions: Vec<String>,
    pub blockers: Vec<String>,
    pub clear_blockers: bool,
    pub active_boundaries: Vec<String>,
    pub repo_scope: Vec<String>,
    pub inspected_files: Vec<String>,
    pub modified_files: Vec<String>,
    pub declared_commands: Vec<String>,
    pub tool_calls: Vec<String>,
    pub loops: Vec<TrajectoryLoop>,
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
            &mut out.next_transitions,
            &mut out.blockers,
            &mut out.evidence,
            &mut out.shortcut_risk_signals,
            &mut out.unresolved_assumptions,
        ] {
            values.sort();
            values.dedup();
        }
        out.checks.sort();
        out.checks.dedup_by(|left, right| left.name == right.name);
        for loop_record in &mut out.loops {
            loop_record.tool_calls.sort();
            loop_record.tool_calls.dedup();
            loop_record.observations.sort();
            loop_record.observations.dedup();
            loop_record.proof_refs.sort();
            loop_record.proof_refs.dedup();
        }
        out.loops.sort_by(|left, right| {
            left.loop_id
                .cmp(&right.loop_id)
                .then(left.attempt.cmp(&right.attempt))
        });
        out.loops
            .dedup_by(|left, right| left.loop_id == right.loop_id && left.attempt == right.attempt);
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

pub fn motion_state(artifact: &TrajectoryArtifact) -> TrajectoryMotionState {
    if artifact
        .blockers
        .iter()
        .any(|b| b.to_ascii_lowercase().contains("abandon"))
    {
        return TrajectoryMotionState::Abandoned;
    }
    if !artifact.blockers.is_empty() {
        return TrajectoryMotionState::Blocked;
    }
    if artifact.completion_claim.is_some() {
        return match artifact.proof_status {
            TrajectoryProofStatus::Passed => TrajectoryMotionState::Completed,
            TrajectoryProofStatus::Failed => TrajectoryMotionState::Abandoned,
            _ => TrajectoryMotionState::Waiting,
        };
    }
    if trajectory_is_stalled(artifact) {
        return TrajectoryMotionState::Stalled;
    }
    TrajectoryMotionState::Active
}

/// Stalled when verification or agent loops show contiguous failed attempts
/// without a later pass, or when proof_status is failed with no completion claim.
fn trajectory_is_stalled(artifact: &TrajectoryArtifact) -> bool {
    if matches!(artifact.proof_status, TrajectoryProofStatus::Failed) {
        return true;
    }
    // Group by loop_id and look for max attempt >= 2 with failed status and no pass.
    use std::collections::BTreeMap;
    let mut by_loop: BTreeMap<&str, Vec<&TrajectoryLoop>> = BTreeMap::new();
    for loop_rec in &artifact.loops {
        by_loop
            .entry(loop_rec.loop_id.as_str())
            .or_default()
            .push(loop_rec);
    }
    for attempts in by_loop.values() {
        let mut max_attempt = 0u32;
        let mut saw_pass = false;
        let mut saw_fail = false;
        for rec in attempts {
            max_attempt = max_attempt.max(rec.attempt);
            match rec.status {
                TrajectoryLoopStatus::Passed => saw_pass = true,
                TrajectoryLoopStatus::Failed | TrajectoryLoopStatus::Blocked => saw_fail = true,
                _ => {}
            }
            if matches!(rec.grader_result, TrajectoryGraderResult::Fail) {
                saw_fail = true;
            }
            if matches!(rec.grader_result, TrajectoryGraderResult::Pass) {
                saw_pass = true;
            }
        }
        if max_attempt >= 2 && saw_fail && !saw_pass {
            return true;
        }
    }
    // Failed checks with no passed checks also signal stall.
    let failed_checks = artifact
        .checks
        .iter()
        .filter(|c| matches!(c.status, TrajectoryCheckStatus::Failed))
        .count();
    let passed_checks = artifact
        .checks
        .iter()
        .filter(|c| matches!(c.status, TrajectoryCheckStatus::Passed))
        .count();
    failed_checks >= 2 && passed_checks == 0
}

pub fn trajectory_path(project_root: &Path, run_id: &str) -> Result<PathBuf, error::DecapodError> {
    validate_run_id(run_id)?;
    Ok(trajectory_cookie_path(project_root))
}

pub fn trajectory_cookie_path(project_root: &Path) -> PathBuf {
    project_root.join(TRAJECTORY_PATH)
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

pub struct TrajectoryInit {
    pub run_id: String,
    pub task_id: Option<String>,
    pub intent_id: Option<String>,
    pub original_intent: String,
    pub derived_intent: String,
    pub active_boundaries: Vec<String>,
    pub repo_scope: Vec<String>,
    pub destination: Option<String>,
    pub current_phase: Option<String>,
    pub next_transitions: Vec<String>,
    pub blockers: Vec<String>,
}

pub fn init_trajectory(
    project_root: &Path,
    input: TrajectoryInit,
) -> Result<TrajectoryArtifact, error::DecapodError> {
    let TrajectoryInit {
        run_id,
        task_id,
        intent_id,
        original_intent,
        derived_intent,
        active_boundaries,
        repo_scope,
        destination,
        current_phase,
        next_transitions,
        blockers,
    } = input;
    let path = trajectory_path(project_root, &run_id)?;
    if path.exists() {
        let existing = load_trajectory_cookie(project_root)?.ok_or_else(|| {
            error::DecapodError::ValidationError(
                "trajectory cookie exists but could not be loaded".to_string(),
            )
        })?;
        if existing.run_id == run_id {
            return Err(error::DecapodError::ValidationError(format!(
                "trajectory '{run_id}' already exists"
            )));
        }
    }
    if original_intent.trim().is_empty() || derived_intent.trim().is_empty() {
        return Err(error::DecapodError::ValidationError(
            "trajectory requires non-empty original_intent and derived_intent".to_string(),
        ));
    }

    let effective_intent_id = intent_id.unwrap_or_else(|| format!("intent:{run_id}"));
    let custody = crate::core::custody::bootstrap_intent(
        &effective_intent_id,
        original_intent.clone(),
        derived_intent.clone(),
        active_boundaries.clone(),
        repo_scope.clone(),
    )
    .map_err(|e| {
        error::DecapodError::ValidationError(format!("failed to initialize intent custody: {e}"))
    })?;

    let artifact = TrajectoryArtifact {
        schema_version: TRAJECTORY_SCHEMA_VERSION.to_string(),
        run_id: run_id.to_string(),
        intent_id: Some(effective_intent_id),
        task_id,
        original_intent,
        derived_intent,
        destination,
        current_phase,
        next_transitions,
        blockers,
        active_boundaries,
        repo_scope,
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
        artifact_hash: String::new(),
        custody,
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
    if artifact.schema_version != TRAJECTORY_SCHEMA_VERSION
        && artifact.schema_version != LEGACY_TRAJECTORY_SCHEMA_VERSION
    {
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
    validate_loops(&artifact)?;
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

pub fn load_trajectory_cookie(
    project_root: &Path,
) -> Result<Option<TrajectoryArtifact>, error::DecapodError> {
    let path = trajectory_cookie_path(project_root);
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path).map_err(error::DecapodError::IoError)?;
    let artifact: TrajectoryArtifact = serde_json::from_str(&raw).map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "invalid trajectory artifact {}: {e}",
            path.display()
        ))
    })?;
    load_trajectory(project_root, &artifact.run_id).map(Some)
}

pub fn write_trajectory(
    project_root: &Path,
    artifact: &TrajectoryArtifact,
) -> Result<TrajectoryArtifact, error::DecapodError> {
    let path = trajectory_path(project_root, &artifact.run_id)?;
    let mut candidate = artifact.clone();
    if candidate.schema_version == LEGACY_TRAJECTORY_SCHEMA_VERSION {
        candidate.schema_version = TRAJECTORY_SCHEMA_VERSION.to_string();
    }
    let canonical = candidate.with_recomputed_hash().map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "failed to serialize trajectory artifact: {e}"
        ))
    })?;
    validate_loops(&canonical)?;
    let parent = path.parent().ok_or_else(|| {
        error::DecapodError::ValidationError("invalid trajectory parent path".to_string())
    })?;
    fs::create_dir_all(parent).map_err(error::DecapodError::IoError)?;
    let bytes = serde_json::to_vec_pretty(&canonical).map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "failed to serialize trajectory artifact: {e}"
        ))
    })?;
    crate::core::atomic::write_atomic(&path, &bytes).map_err(error::DecapodError::IoError)?;
    Ok(canonical)
}

pub fn record_trajectory(
    project_root: &Path,
    run_id: &str,
    update: TrajectoryUpdate,
) -> Result<TrajectoryArtifact, error::DecapodError> {
    let mut artifact = load_trajectory(project_root, run_id)?;
    let loop_count = update.loops.len();
    let generic_step = if loop_count == 0
        && (!update.declared_commands.is_empty()
            || !update.tool_calls.is_empty()
            || !update.inspected_files.is_empty()
            || !update.modified_files.is_empty()
            || !update.checks.is_empty()
            || !update.evidence.is_empty()
            || !update.shortcut_risk_signals.is_empty()
            || !update.unresolved_assumptions.is_empty())
    {
        Some(crate::core::custody::TrajectoryStepInput {
            action: update
                .current_phase
                .clone()
                .unwrap_or_else(|| "trajectory.record".to_string()),
            tool: update.tool_calls.first().cloned(),
            command: update.declared_commands.first().cloned(),
            scope: artifact.repo_scope.clone(),
            observations: update
                .inspected_files
                .iter()
                .chain(update.modified_files.iter())
                .chain(update.evidence.iter())
                .cloned()
                .collect(),
            proof_refs: update
                .checks
                .iter()
                .map(|check| check.name.clone())
                .collect(),
            validation_findings: update
                .shortcut_risk_signals
                .iter()
                .chain(update.unresolved_assumptions.iter())
                .cloned()
                .collect(),
        })
    } else {
        None
    };
    if update.task_id.is_some() {
        artifact.task_id = update.task_id;
    }
    if update.destination.is_some() {
        artifact.destination = update.destination;
    }
    if update.current_phase.is_some() {
        artifact.current_phase = update.current_phase;
    }
    artifact.next_transitions.extend(update.next_transitions);
    if update.clear_blockers {
        artifact.blockers.clear();
    }
    artifact.blockers.extend(update.blockers);
    artifact.active_boundaries.extend(update.active_boundaries);
    artifact.repo_scope.extend(update.repo_scope);
    artifact.inspected_files.extend(update.inspected_files);
    artifact.modified_files.extend(update.modified_files);
    artifact.declared_commands.extend(update.declared_commands);
    artifact.tool_calls.extend(update.tool_calls);
    for loop_record in update.loops {
        let mut loop_record = loop_record;
        let input = crate::core::custody::TrajectoryStepInput {
            action: format!("loop:{}:{}", loop_record.loop_id, loop_record.attempt),
            tool: loop_record.tool_calls.first().cloned(),
            scope: artifact.repo_scope.clone(),
            observations: loop_record.observations.clone(),
            proof_refs: loop_record.proof_refs.clone(),
            validation_findings: if loop_record.feedback.is_empty() {
                Vec::new()
            } else {
                vec![loop_record.feedback.clone()]
            },
            ..Default::default()
        };
        let (custody, event_id) = crate::core::custody::append_trajectory_step(
            artifact.custody.clone(),
            &artifact.run_id,
            artifact
                .intent_id
                .as_deref()
                .unwrap_or(&format!("intent:{}", artifact.run_id)),
            input,
        )
        .map_err(|e| {
            error::DecapodError::ValidationError(format!(
                "failed to append trajectory custody step: {e}"
            ))
        })?;
        artifact.custody = custody;
        loop_record.custody_event_id = Some(event_id);
        artifact.loops.retain(|existing| {
            existing.loop_id != loop_record.loop_id || existing.attempt != loop_record.attempt
        });
        artifact.loops.push(loop_record);
    }
    if let Some(input) = generic_step {
        let (custody, _) = crate::core::custody::append_trajectory_step(
            artifact.custody.clone(),
            &artifact.run_id,
            artifact
                .intent_id
                .as_deref()
                .unwrap_or(&format!("intent:{}", artifact.run_id)),
            input,
        )
        .map_err(|e| {
            error::DecapodError::ValidationError(format!(
                "failed to append trajectory custody step: {e}"
            ))
        })?;
        artifact.custody = custody;
    }
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

pub fn parse_loop_json(spec: &str) -> Result<TrajectoryLoop, error::DecapodError> {
    serde_json::from_str(spec).map_err(|e| {
        error::DecapodError::ValidationError(format!("invalid trajectory loop JSON: {e}"))
    })
}

pub fn validate_loops(artifact: &TrajectoryArtifact) -> Result<(), error::DecapodError> {
    let mut attempts = std::collections::BTreeMap::<String, Vec<u32>>::new();
    let mut loop_ids = std::collections::BTreeSet::new();
    for loop_record in &artifact.loops {
        for (field, value) in [
            ("intent_id", loop_record.intent_id.as_str()),
            ("trajectory_id", loop_record.trajectory_id.as_str()),
            ("loop_id", loop_record.loop_id.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(error::DecapodError::ValidationError(format!(
                    "trajectory loop {field} cannot be empty"
                )));
            }
        }
        if loop_record.trajectory_id != artifact.run_id {
            return Err(error::DecapodError::ValidationError(format!(
                "trajectory loop '{}' belongs to '{}', expected '{}'",
                loop_record.loop_id, loop_record.trajectory_id, artifact.run_id
            )));
        }
        if let Some(intent_id) = artifact.intent_id.as_deref()
            && loop_record.intent_id != intent_id
        {
            return Err(error::DecapodError::ValidationError(format!(
                "trajectory loop '{}' crosses intent boundary: expected '{}', found '{}'",
                loop_record.loop_id, intent_id, loop_record.intent_id
            )));
        }
        if loop_record.attempt == 0 {
            return Err(error::DecapodError::ValidationError(format!(
                "trajectory loop '{}' attempt must be at least 1",
                loop_record.loop_id
            )));
        }
        if loop_record.feedback.len() > MAX_LOOP_FEEDBACK_BYTES {
            return Err(error::DecapodError::ValidationError(format!(
                "trajectory loop '{}' feedback exceeds the {} byte bound",
                loop_record.loop_id, MAX_LOOP_FEEDBACK_BYTES
            )));
        }
        if loop_record
            .proof_refs
            .iter()
            .any(|reference| reference.trim().is_empty())
        {
            return Err(error::DecapodError::ValidationError(format!(
                "trajectory loop '{}' contains an empty proof reference",
                loop_record.loop_id
            )));
        }
        match (&loop_record.loop_type, &loop_record.grader_result) {
            (TrajectoryLoopType::Verification, TrajectoryGraderResult::Pass)
                if loop_record.proof_refs.is_empty() =>
            {
                return Err(error::DecapodError::ValidationError(format!(
                    "passing verification loop '{}' requires at least one proof reference",
                    loop_record.loop_id
                )));
            }
            (TrajectoryLoopType::Verification, TrajectoryGraderResult::Fail)
                if loop_record.feedback.trim().is_empty() =>
            {
                return Err(error::DecapodError::ValidationError(format!(
                    "failed verification loop '{}' requires bounded feedback",
                    loop_record.loop_id
                )));
            }
            _ => {}
        }
        if !loop_ids.insert((loop_record.loop_id.clone(), loop_record.attempt)) {
            return Err(error::DecapodError::ValidationError(format!(
                "trajectory loop '{}' has duplicate attempt {}",
                loop_record.loop_id, loop_record.attempt
            )));
        }
        attempts
            .entry(loop_record.loop_id.clone())
            .or_default()
            .push(loop_record.attempt);
    }
    for (loop_id, mut values) in attempts {
        values.sort_unstable();
        for (expected, actual) in values
            .iter()
            .enumerate()
            .map(|(index, value)| ((index as u32) + 1, *value))
        {
            if expected != actual {
                return Err(error::DecapodError::ValidationError(format!(
                    "trajectory loop '{}' attempts must be contiguous from 1; expected {}, found {}",
                    loop_id, expected, actual
                )));
            }
        }
    }
    Ok(())
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
#[path = "../../../tests/unit/core/trajectory_tests.rs"]
mod tests;
