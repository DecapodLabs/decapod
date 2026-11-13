//! Deterministic governed-state projection and diff capabilities.
//!
//! This module implements `state.resolve` and `state.diff` — the authoritative
//! agent-facing projection of governed reality derived from canonical repository
//! and Decapod state. No shadow state is introduced; this is a read model over
//! existing authoritative sources.

use crate::core::custody::{CustodyState, IntentStatus};
use crate::core::error;
use crate::core::governance_artifacts;
use crate::core::governance_document;
use crate::core::obligation;
use crate::core::proof;
use crate::core::store::{Store, StoreKind};
use crate::core::trajectory;
use crate::core::validate;
use crate::core::validation_epoch;
use crate::core::workunit;
use crate::plan_governance;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

pub const STATE_SCHEMA_VERSION: &str = "1.0.0";

// ============================================================================
// Core Types for State Resolution
// ============================================================================

/// Unique identifier for a governed work context
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum StateIdentityKind {
    Workunit,
    Trajectory,
    Intent,
    Obligation,
}

/// Identity of the governed work being resolved
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateIdentity {
    pub kind: StateIdentityKind,
    pub id: String,
    pub intent_ref: Option<String>,
    pub task_id: Option<String>,
}

/// Git revision information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RevisionInfo {
    pub head_sha: String,
    pub base_sha: Option<String>,
    pub branch: Option<String>,
    pub is_dirty: bool,
    pub dirty_files: Vec<String>,
}

/// Objective and acceptance criteria
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObjectiveInfo {
    pub original_intent: String,
    pub derived_intent: Option<String>,
    pub acceptance_criteria: Vec<String>,
    pub done_criteria: Option<String>,
}

/// Scope and constraints
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScopeConstraints {
    pub active_boundaries: Vec<String>,
    pub repo_scope: Vec<String>,
    pub out_of_scope: Vec<String>,
    pub protected_paths: Vec<String>,
    pub risk_tier: Option<String>,
}

/// Custody state projection
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustodyProjection {
    pub intent_id: String,
    pub intent_status: IntentStatus,
    pub raw_intent: String,
    pub refined_intent: Option<String>,
    pub assumptions: Vec<String>,
    pub proof_requirements: Vec<String>,
    pub trajectory_step_count: usize,
    pub event_count: usize,
    pub blockers: Vec<String>,
}

/// A project-level decision (durable governance knowledge)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Decision {
    pub id: String,
    pub proposition: String,
    pub status: DecisionStatus,
    pub authority: String,
    pub rationale_or_evidence: Option<String>,
    pub state_or_revision: Option<String>,
    pub scope: Vec<String>,
    pub supersedes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knowledge: Option<DecisionKnowledge>,
}

/// Source labels are provenance, never authenticated approval or executable policy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecisionKnowledge {
    pub lifecycle: String,
    pub confidence: String,
    pub actor: String,
    pub sources: Vec<String>,
    pub effective_from: Option<String>,
    pub effective_to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecisionStatus {
    Proposed,
    Accepted,
    Rejected,
    Superseded,
    RequiresHuman,
}

/// A previously attempted and rejected approach (durable negative knowledge)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FailedPath {
    pub id: String,
    pub hypothesis_or_approach: String,
    pub scope: Vec<String>,
    pub attempted_at: String,
    pub rejection_reason: String,
    pub evidence: Vec<String>,
    pub superseded: bool,
}

/// Blocker with semantic taxonomy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlockerInfo {
    pub id: String,
    pub kind: BlockerKind,
    pub description: String,
    pub evidence: Vec<String>,
    pub declared_at: String,
    pub resolved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BlockerKind {
    ExternalDependency,
    AuthorityRequired,
    HumanDecision,
    Environment,
    ProofUnavailable,
    DependencyUnresolved,
    ScopeViolation,
    PolicyViolation,
}

/// Dependency with resolution state
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DependencyInfo {
    pub id: String,
    pub description: String,
    pub kind: DependencyKind,
    pub required_by: Vec<String>,
    pub status: DependencyStatus,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DependencyKind {
    ExternalService,
    InternalComponent,
    HumanApproval,
    Infrastructure,
    Data,
    Verifier,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DependencyStatus {
    Unresolved,
    InProgress,
    Resolved,
    Failed,
    Deferred,
}

/// Changed surfaces since last state commit / baseline
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChangedSurfaces {
    pub files: Vec<String>,
    pub config_surfaces: Vec<String>,
    pub proof_surfaces: Vec<String>,
    pub evaluator_surfaces: Vec<String>,
    pub spec_surfaces: Vec<String>,
    pub governance_surfaces: Vec<String>,
}

/// Proof freshness relative to current governed state
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProofFreshness {
    Valid,
    Stale,
    Unaffected,
    Unknown,
    Failed,
}

/// Individual proof state
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProofState {
    pub gate: String,
    pub name: String,
    pub required: bool,
    pub freshness: ProofFreshness,
    pub last_run_at: Option<String>,
    pub last_result: Option<String>,
    pub artifact_ref: Option<String>,
    pub evaluator_epoch: Option<String>,
    pub validation_epoch: Option<validation_epoch::ValidationEpochMetadata>,
    pub freshness_reasons: Vec<String>,
    pub assurance_level: Option<AssuranceLevel>,
    pub verifier_identity: Option<VerifierIdentity>,
}

/// Assurance/verification strength levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssuranceLevel {
    A0Structural,
    A1Behavioral,
    A2IsolatedReplayed,
    A3IndependentlyVerified,
}

/// Independent verifier identity
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerifierIdentity {
    SelfVerified,
    IsolatedProcess,
    IndependentAgent,
    IndependentProvider,
    ExternalSystem,
}

/// Verifier integrity status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerifierIntegrity {
    Unchanged,
    ChangedAuthorized,
    ChangedUntrusted,
}

/// Validation and proof projection
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationProofProjection {
    pub proof_states: Vec<ProofState>,
    pub proof_plan: Vec<String>,
    pub validation_receipt: Option<ValidationReceiptSummary>,
    pub verifier_integrity: VerifierIntegrity,
    pub assurance_policy: Option<AssurancePolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationReceiptSummary {
    pub status: String,
    pub pass_count: u32,
    pub fail_count: u32,
    pub git_revision: String,
    pub trajectory_run_id: Option<String>,
    pub receipt_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvaluatorChange {
    pub surface: String,
    pub change_type: EvaluatorChangeType,
    pub detected_at: String,
    pub authorization: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvaluatorChangeType {
    ValidationConfig,
    AcceptanceTests,
    ValidationScripts,
    CiWorkflow,
    ProofGenerationCode,
    PolicyConfiguration,
    GovernedRules,
    DecapodOverride,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssurancePolicy {
    pub minimum_level: AssuranceLevel,
    pub required_for_publication: bool,
    pub required_for_completion: bool,
}

/// Unresolved obligations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnresolvedObligation {
    pub id: String,
    pub intent_ref: String,
    pub risk_tier: String,
    pub required_proofs: Vec<String>,
    pub dependencies_satisfied: bool,
    pub proofs_satisfied: bool,
    pub commit_present: bool,
    pub blocking_dependencies: Vec<String>,
    pub missing_proofs: Vec<String>,
}

/// Publication eligibility
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicationEligibility {
    pub eligible: bool,
    pub reason: String,
    pub required_proofs_current: bool,
    pub verifier_integrity_ok: bool,
    pub state_commit_present: bool,
    pub assurance_requirements_met: bool,
    pub blocking_factors: Vec<String>,
}

/// Admissible actions from current state
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdmissibleActions {
    pub allowed: Vec<AllowedAction>,
    pub required: Vec<RequiredAction>,
    pub forbidden: Vec<ForbiddenAction>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AllowedAction {
    pub action: String,
    pub reason: String,
    pub scope_constraints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RequiredAction {
    pub action: String,
    pub reason: String,
    pub blocking_obligation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForbiddenAction {
    pub action: String,
    pub reason: String,
    pub blocker: Option<String>,
}

/// Provenance information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProvenanceInfo {
    pub state_commit_root: Option<String>,
    pub scope_record_hash: Option<String>,
    pub governance_document_hash: Option<String>,
    pub trajectory_artifact_hash: Option<String>,
    pub workunit_manifest_hash: Option<String>,
    pub validation_receipt_hash: Option<String>,
    pub resolved_at: String,
    pub decapod_version: String,
}

/// Complete state resolution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateResolution {
    pub identity: StateIdentity,
    pub revision: RevisionInfo,
    pub objective: ObjectiveInfo,
    pub acceptance: ObjectiveInfo,
    pub scope: ScopeConstraints,
    pub constraints: ScopeConstraints,
    pub custody: CustodyProjection,
    pub decisions: Vec<Decision>,
    pub failed_paths: Vec<FailedPath>,
    pub blockers: Vec<BlockerInfo>,
    pub dependencies: Vec<DependencyInfo>,
    pub changed_surfaces: ChangedSurfaces,
    pub validation: ValidationProofProjection,
    pub proof: ValidationProofProjection,
    pub unresolved_obligations: Vec<UnresolvedObligation>,
    pub publication: PublicationEligibility,
    pub admissible_actions: AdmissibleActions,
    pub provenance: ProvenanceInfo,
}

// ============================================================================
// State Diff Types
// ============================================================================

/// Diff between two governed states
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateDiff {
    pub from: StateResolution,
    pub to: StateResolution,
    pub intent_changes: Vec<IntentChange>,
    pub decision_changes: Vec<DecisionChange>,
    pub failed_path_changes: Vec<FailedPathChange>,
    pub custody_changes: Vec<CustodyChange>,
    pub scope_changes: Vec<ScopeChange>,
    pub dependency_changes: Vec<DependencyChange>,
    pub blocker_changes: Vec<BlockerChange>,
    pub changed_surfaces: ChangedSurfaces,
    pub proof_added: Vec<ProofState>,
    pub proof_preserved: Vec<ProofState>,
    pub proof_invalidated: Vec<ProofState>,
    pub proof_unknown: Vec<ProofState>,
    pub publication_transition: PublicationTransition,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IntentChange {
    pub field: String,
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecisionChange {
    pub decision_id: String,
    pub change_type: DecisionChangeType,
    pub from_status: Option<DecisionStatus>,
    pub to_status: Option<DecisionStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecisionChangeType {
    Added,
    Removed,
    StatusChanged,
    Superseded,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FailedPathChange {
    pub failed_path_id: String,
    pub change_type: FailedPathChangeType,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FailedPathChangeType {
    Added,
    Removed,
    Superseded,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustodyChange {
    pub field: String,
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScopeChange {
    pub field: String,
    pub from: Vec<String>,
    pub to: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DependencyChange {
    pub dependency_id: String,
    pub change_type: DependencyChangeType,
    pub from_status: Option<DependencyStatus>,
    pub to_status: Option<DependencyStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DependencyChangeType {
    Added,
    Removed,
    StatusChanged,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlockerChange {
    pub blocker_id: String,
    pub change_type: BlockerChangeType,
    pub from_kind: Option<BlockerKind>,
    pub to_kind: Option<BlockerKind>,
    pub from_resolved: Option<bool>,
    pub to_resolved: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BlockerChangeType {
    Added,
    Removed,
    Resolved,
    KindChanged,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicationTransition {
    pub from_eligible: bool,
    pub to_eligible: bool,
    pub reason: String,
}

// ============================================================================
// Input Parameters
// ============================================================================

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StateResolveParams {
    pub workunit_id: Option<String>,
    pub trajectory_run_id: Option<String>,
    pub intent_id: Option<String>,
    pub obligation_id: Option<String>,
    pub include_proof_details: Option<bool>,
    pub include_obligation_graph: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StateDiffParams {
    pub from_state_commit: Option<String>,
    pub to_state_commit: Option<String>,
    pub from_revision: Option<String>,
    pub to_revision: Option<String>,
    pub from_workunit_id: Option<String>,
    pub to_workunit_id: Option<String>,
}

// ============================================================================
// State Resolution Implementation
// ============================================================================

/// Resolve the current governed state
pub fn resolve_state(
    project_root: &Path,
    params: &StateResolveParams,
) -> Result<StateResolution, error::DecapodError> {
    let store_root = project_root.join(".decapod").join("data");
    let store = Store {
        kind: StoreKind::Repo,
        root: store_root.clone(),
    };

    // Determine the identity source
    let identity = determine_identity(project_root, &store, params)?;

    // Resolve all projections
    let revision = resolve_revision(project_root)?;
    let objective = resolve_objective(project_root, &identity)?;
    let acceptance = resolve_acceptance(project_root, &identity)?;
    let (scope, constraints) = resolve_scope_constraints(project_root, &identity)?;
    let custody = resolve_custody(project_root, &identity)?;
    let decisions = resolve_decisions(project_root, &identity)?;
    let failed_paths = resolve_failed_paths(project_root, &identity)?;
    let blockers = resolve_blockers(project_root, &identity)?;
    let dependencies = resolve_dependencies(project_root, &identity)?;
    let changed_surfaces = resolve_changed_surfaces(project_root, &identity)?;
    let (validation, proof) = resolve_validation_proof(project_root, &identity, params)?;
    let unresolved_obligations = resolve_unresolved_obligations(project_root, &identity, params)?;
    let publication = resolve_publication(project_root, &identity, &validation)?;
    let admissible_actions = resolve_admissible_actions(
        project_root,
        &identity,
        &validation,
        &publication,
        &blockers,
        &unresolved_obligations,
    )?;
    let provenance = resolve_provenance(project_root, &identity)?;

    Ok(StateResolution {
        identity,
        revision,
        objective,
        acceptance,
        scope,
        constraints,
        custody,
        decisions,
        failed_paths,
        blockers,
        dependencies,
        changed_surfaces,
        validation,
        proof,
        unresolved_obligations,
        publication,
        admissible_actions,
        provenance,
    })
}

/// Diff two governed states
pub fn diff_state(
    project_root: &Path,
    params: &StateDiffParams,
) -> Result<StateDiff, error::DecapodError> {
    // Resolve "from" state
    let from_params = StateResolveParams {
        workunit_id: params.from_workunit_id.clone(),
        trajectory_run_id: None,
        intent_id: None,
        obligation_id: None,
        include_proof_details: Some(true),
        include_obligation_graph: Some(true),
    };
    let from = resolve_state(project_root, &from_params)?;

    // Resolve "to" state
    let to_params = StateResolveParams {
        workunit_id: params.to_workunit_id.clone(),
        trajectory_run_id: None,
        intent_id: None,
        obligation_id: None,
        include_proof_details: Some(true),
        include_obligation_graph: Some(true),
    };
    let to = resolve_state(project_root, &to_params)?;

    // Compute diff
    compute_state_diff(&from, &to)
}

// ============================================================================
// Identity Resolution
// ============================================================================

fn determine_identity(
    project_root: &Path,
    _store: &Store,
    params: &StateResolveParams,
) -> Result<StateIdentity, error::DecapodError> {
    // Priority: workunit_id > trajectory_run_id > intent_id > obligation_id > active
    if let Some(workunit_id) = &params.workunit_id {
        let manifest = workunit::load_workunit(project_root, workunit_id)?;
        return Ok(StateIdentity {
            kind: StateIdentityKind::Workunit,
            id: workunit_id.clone(),
            intent_ref: Some(manifest.intent_ref),
            task_id: Some(workunit_id.clone()),
        });
    }

    if let Some(trajectory_run_id) = &params.trajectory_run_id {
        let trajectory_artifact = trajectory::load_trajectory(project_root, trajectory_run_id)?;
        return Ok(StateIdentity {
            kind: StateIdentityKind::Trajectory,
            id: trajectory_run_id.clone(),
            intent_ref: trajectory_artifact.intent_id.clone(),
            task_id: trajectory_artifact.task_id.clone(),
        });
    }

    if let Some(intent_id) = &params.intent_id {
        return Ok(StateIdentity {
            kind: StateIdentityKind::Intent,
            id: intent_id.clone(),
            intent_ref: Some(intent_id.clone()),
            task_id: None,
        });
    }

    if let Some(obligation_id) = &params.obligation_id {
        return Ok(StateIdentity {
            kind: StateIdentityKind::Obligation,
            id: obligation_id.clone(),
            intent_ref: None,
            task_id: None,
        });
    }

    // Try to find active workunit from current trajectory
    if let Ok(Some(trajectory_artifact)) = trajectory::load_trajectory_cookie(project_root) {
        if let Some(task_id) = &trajectory_artifact.task_id
            && workunit::workunit_path(project_root, task_id)?.exists()
        {
            return Ok(StateIdentity {
                kind: StateIdentityKind::Workunit,
                id: task_id.clone(),
                intent_ref: trajectory_artifact.intent_id.clone(),
                task_id: Some(task_id.clone()),
            });
        }
        return Ok(StateIdentity {
            kind: StateIdentityKind::Trajectory,
            id: trajectory_artifact.run_id.clone(),
            intent_ref: trajectory_artifact.intent_id.clone(),
            task_id: trajectory_artifact.task_id.clone(),
        });
    }

    // Try to find any workunit
    let workunits_dir = project_root.join(".decapod").join("data").join("workunits");
    if workunits_dir.exists()
        && let Ok(entries) = std::fs::read_dir(&workunits_dir)
    {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str()
                && name.ends_with(".json")
            {
                let workunit_id = name.trim_end_matches(".json");
                let manifest = workunit::load_workunit(project_root, workunit_id)?;
                return Ok(StateIdentity {
                    kind: StateIdentityKind::Workunit,
                    id: workunit_id.to_string(),
                    intent_ref: Some(manifest.intent_ref),
                    task_id: Some(workunit_id.to_string()),
                });
            }
        }
    }

    Err(error::DecapodError::ValidationError(
        "No governed work identity found. Provide workunit_id, trajectory_run_id, intent_id, or obligation_id".into(),
    ))
}

// ============================================================================
// Revision Resolution
// ============================================================================

fn resolve_revision(project_root: &Path) -> Result<RevisionInfo, error::DecapodError> {
    let head_sha = git_rev_parse(project_root, "HEAD")?;
    let base_sha = git_merge_base(project_root)?;
    let branch = git_branch(project_root)?;
    let (is_dirty, dirty_files) = check_dirty(project_root)?;

    Ok(RevisionInfo {
        head_sha,
        base_sha,
        branch,
        is_dirty,
        dirty_files,
    })
}

fn git_rev_parse(project_root: &Path, ref_: &str) -> Result<String, error::DecapodError> {
    let output = std::process::Command::new("git")
        .current_dir(project_root)
        .args(["rev-parse", ref_])
        .output()
        .map_err(error::DecapodError::IoError)?;
    if !output.status.success() {
        return Err(error::DecapodError::ValidationError(format!(
            "git rev-parse failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git_merge_base(project_root: &Path) -> Result<Option<String>, error::DecapodError> {
    let base_branch = crate::core::workspace::detect_base_branch(project_root);
    let Some(base_branch) = base_branch else {
        return Ok(None);
    };
    let output = std::process::Command::new("git")
        .current_dir(project_root)
        .args(["merge-base", &base_branch, "HEAD"])
        .output()
        .map_err(error::DecapodError::IoError)?;
    if output.status.success() {
        let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !sha.is_empty() {
            return Ok(Some(sha));
        }
    }
    Ok(None)
}

fn git_branch(project_root: &Path) -> Result<Option<String>, error::DecapodError> {
    let output = std::process::Command::new("git")
        .current_dir(project_root)
        .args(["symbolic-ref", "--quiet", "--short", "HEAD"])
        .output()
        .map_err(error::DecapodError::IoError)?;
    if output.status.success() {
        let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !branch.is_empty() {
            return Ok(Some(branch));
        }
    }
    Ok(None)
}

fn check_dirty(project_root: &Path) -> Result<(bool, Vec<String>), error::DecapodError> {
    let output = std::process::Command::new("git")
        .current_dir(project_root)
        .args(["status", "--porcelain"])
        .output()
        .map_err(error::DecapodError::IoError)?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let files: Vec<String> = stdout
        .lines()
        .map(|line| {
            line.trim_start_matches(|c: char| {
                c.is_whitespace()
                    || c == 'M'
                    || c == 'A'
                    || c == 'D'
                    || c == 'R'
                    || c == 'C'
                    || c == '?'
                    || c == '!'
                    || c == ' '
            })
            .to_string()
        })
        .filter(|s| !s.is_empty())
        .collect();
    Ok((!files.is_empty(), files))
}

// ============================================================================
// Objective & Acceptance Resolution
// ============================================================================

fn resolve_objective(
    project_root: &Path,
    identity: &StateIdentity,
) -> Result<ObjectiveInfo, error::DecapodError> {
    let mut acceptance_criteria = Vec::new();
    let mut derived_intent = None;

    // Try trajectory first
    if let Ok(Some(trajectory_artifact)) = trajectory::load_trajectory_cookie(project_root)
        && (identity.intent_ref.as_deref() == trajectory_artifact.intent_id.as_deref()
            || identity.task_id.as_deref() == trajectory_artifact.task_id.as_deref())
    {
        derived_intent = Some(trajectory_artifact.derived_intent.clone());
        acceptance_criteria.extend(trajectory_artifact.active_boundaries.clone());
    }

    // Try workunit
    if let Some(task_id) = &identity.task_id
        && let Ok(manifest) = workunit::load_workunit(project_root, task_id)
    {
        let intent_id = &manifest.intent_ref;
        // Load from governance document intents
        if let Ok(Some(doc)) = governance_document::load(project_root)
            && let Some(intent_value) = doc.intents.get(intent_id)
            && let Some(criteria) = intent_value
                .get("acceptance_criteria")
                .and_then(|v| v.as_array())
        {
            for c in criteria {
                if let Some(s) = c.as_str() {
                    acceptance_criteria.push(s.to_string());
                }
            }
        }
    }

    // Try plan governance
    if let Ok(Some(plan)) = plan_governance::load_plan(project_root) {
        acceptance_criteria.extend(plan.constraints.forbidden_paths.clone());
    }

    // Try config.toml for done criteria
    let done_criteria = crate::cli::DecapodProjectConfig::load(project_root)
        .ok()
        .and_then(|c| c.repo.done_criteria);

    Ok(ObjectiveInfo {
        original_intent: derived_intent
            .clone()
            .unwrap_or_else(|| "Not established".to_string()),
        derived_intent,
        acceptance_criteria,
        done_criteria,
    })
}

fn resolve_acceptance(
    project_root: &Path,
    identity: &StateIdentity,
) -> Result<ObjectiveInfo, error::DecapodError> {
    // Acceptance is the same as objective in this model
    // but could be extended with specific acceptance gates
    resolve_objective(project_root, identity)
}

// ============================================================================
// Scope & Constraints Resolution
// ============================================================================

fn resolve_scope_constraints(
    project_root: &Path,
    identity: &StateIdentity,
) -> Result<(ScopeConstraints, ScopeConstraints), error::DecapodError> {
    let mut active_boundaries = Vec::new();
    let mut repo_scope = Vec::new();
    let mut out_of_scope = Vec::new();
    let mut protected_paths = Vec::new();
    let mut risk_tier = None;

    // From trajectory
    if let Ok(Some(trajectory_artifact)) = trajectory::load_trajectory_cookie(project_root) {
        active_boundaries.extend(trajectory_artifact.active_boundaries.clone());
        repo_scope.extend(trajectory_artifact.repo_scope.clone());
    }

    // From workunit
    if let Some(task_id) = &identity.task_id
        && let Ok(_manifest) = workunit::load_workunit(project_root, task_id)
    {
        // Workunit spec_refs and state_refs imply scope
    }

    // From config.toml
    if let Ok(config) = crate::cli::DecapodProjectConfig::load(project_root) {
        protected_paths = config.repo.capabilities.clone(); // Not exactly, but placeholder
        // Protected paths would come from init config
    }

    // From custody intent
    if let Some(intent_id) = &identity.intent_ref
        && let Ok(custody_state) = load_custody_state(project_root)
        && let Some(intent) = custody_state.intents.get(intent_id)
    {
        active_boundaries.extend(intent.boundaries.clone());
        out_of_scope.extend(intent.out_of_scope.clone());
        repo_scope.extend(intent.constraints.clone()); // Constraints as scope
        risk_tier = Some("medium".to_string()); // Would come from intent
    }

    let scope = ScopeConstraints {
        active_boundaries: dedup_sorted(active_boundaries),
        repo_scope: dedup_sorted(repo_scope),
        out_of_scope: dedup_sorted(out_of_scope),
        protected_paths: dedup_sorted(protected_paths),
        risk_tier,
    };

    Ok((scope.clone(), scope))
}

fn dedup_sorted<T: Ord + Clone>(mut v: Vec<T>) -> Vec<T> {
    v.sort();
    v.dedup();
    v
}

// ============================================================================
// Custody Resolution
// ============================================================================

fn load_custody_state(project_root: &Path) -> Result<CustodyState, error::DecapodError> {
    // Custody state is embedded in the trajectory artifact
    if let Ok(Some(trajectory_artifact)) = trajectory::load_trajectory_cookie(project_root) {
        return Ok(trajectory_artifact.custody);
    }
    Ok(CustodyState::default())
}

fn resolve_custody(
    project_root: &Path,
    identity: &StateIdentity,
) -> Result<CustodyProjection, error::DecapodError> {
    let mut blockers = Vec::new();

    // From trajectory blockers
    if let Ok(Some(trajectory_artifact)) = trajectory::load_trajectory_cookie(project_root) {
        blockers.extend(trajectory_artifact.blockers.clone());
    }

    // From custody intent
    if let Some(intent_id) = &identity.intent_ref
        && let Ok(custody_state) = load_custody_state(project_root)
        && let Some(intent) = custody_state.intents.get(intent_id)
    {
        let trajectory_step_count = custody_state
            .trajectories
            .values()
            .filter(|t| t.intent_id == *intent_id)
            .map(|t| t.steps.len())
            .sum();
        let event_count = custody_state
            .events
            .iter()
            .filter(|e| e.intent_id == *intent_id)
            .count();

        return Ok(CustodyProjection {
            intent_id: intent.id.clone(),
            intent_status: intent.status.clone(),
            raw_intent: intent.raw_intent.clone(),
            refined_intent: intent.refined_intent.clone(),
            assumptions: intent.assumptions.clone(),
            proof_requirements: intent.proof_requirements.clone(),
            trajectory_step_count,
            event_count,
            blockers,
        });
    }

    Ok(CustodyProjection {
        intent_id: identity.intent_ref.clone().unwrap_or_default(),
        intent_status: IntentStatus::Raw,
        raw_intent: "Not established".to_string(),
        refined_intent: None,
        assumptions: Vec::new(),
        proof_requirements: Vec::new(),
        trajectory_step_count: 0,
        event_count: 0,
        blockers,
    })
}

// ============================================================================
// Decisions Resolution
// ============================================================================

fn resolve_decisions(
    project_root: &Path,
    _identity: &StateIdentity,
) -> Result<Vec<Decision>, error::DecapodError> {
    let mut decisions = Vec::new();

    // From governance document claims (decisions stored as claims)
    if let Ok(Some(doc)) = governance_document::load(project_root) {
        for (id, claim) in &doc.claims {
            if claim.status == "decision"
                || claim.status == "accepted"
                || claim.status == "rejected"
            {
                decisions.push(Decision {
                    id: id.clone(),
                    proposition: claim.statement.clone(),
                    status: match claim.status.as_str() {
                        "accepted" => DecisionStatus::Accepted,
                        "rejected" => DecisionStatus::Rejected,
                        "proposed" => DecisionStatus::Proposed,
                        "superseded" => DecisionStatus::Superseded,
                        _ => DecisionStatus::Proposed,
                    },
                    authority: "governance_document".to_string(),
                    rationale_or_evidence: Some(claim.falsifier.clone()),
                    state_or_revision: doc.baseline.accepted_commit.clone(),
                    scope: claim.proof_refs.clone(),
                    supersedes: None,
                    created_at: chrono::Utc::now().to_rfc3339(), // Placeholder
                    updated_at: chrono::Utc::now().to_rfc3339(),
                    knowledge: None,
                });
            }
        }

        // From intents in governance document
        for (id, intent_value) in &doc.intents {
            if let Some(status_str) = intent_value.get("status").and_then(|v| v.as_str())
                && matches!(
                    status_str,
                    "approved" | "scope_approved" | "completed" | "failed" | "abandoned"
                )
            {
                let proposition = intent_value
                    .get("refined_intent")
                    .or_else(|| intent_value.get("raw_intent"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Decision recorded")
                    .to_string();
                decisions.push(Decision {
                    id: id.clone(),
                    proposition,
                    status: match status_str {
                        "approved" | "scope_approved" | "completed" => DecisionStatus::Accepted,
                        "failed" | "abandoned" => DecisionStatus::Rejected,
                        _ => DecisionStatus::Proposed,
                    },
                    authority: "intent_custody".to_string(),
                    rationale_or_evidence: intent_value
                        .get("rationale")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    state_or_revision: doc.baseline.accepted_commit.clone(),
                    scope: intent_value
                        .get("boundaries")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default(),
                    supersedes: intent_value
                        .get("supersedes")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    created_at: chrono::Utc::now().to_rfc3339(),
                    updated_at: chrono::Utc::now().to_rfc3339(),
                    knowledge: None,
                });
            }
        }
    }

    // From federation decisions
    if let Ok(nodes) = crate::plugins::federation_ext::list_nodes(
        &project_root.join(".decapod").join("data"),
        Some("decision".to_string()),
        None,
        None,
        None,
    ) {
        for node in nodes {
            if !decisions.iter().any(|d| d.id == node.id) {
                decisions.push(project_federation_decision(node));
            }
        }
    }

    Ok(decisions)
}

fn project_federation_decision(node: crate::plugins::federation::FederationNode) -> Decision {
    // Lifecycle precedes informal tags. Confidence remains source classification,
    // not authentication of the author or authority to execute.
    let status = match node.status.as_str() {
        "superseded" | "deprecated" => DecisionStatus::Superseded,
        "disputed" => DecisionStatus::RequiresHuman,
        "active" if node.effective_to.is_some() => DecisionStatus::Superseded,
        "active" if node.confidence == "human_confirmed" => match node.tags.as_str() {
            "accepted" => DecisionStatus::Accepted,
            "rejected" => DecisionStatus::Rejected,
            _ => DecisionStatus::Proposed,
        },
        _ => DecisionStatus::Proposed,
    };
    Decision {
        id: node.id,
        proposition: node.title,
        status,
        authority: "federation".into(),
        rationale_or_evidence: Some(node.body),
        state_or_revision: None,
        scope: if node.scope.is_empty() {
            vec![]
        } else {
            vec![node.scope]
        },
        supersedes: None,
        created_at: node.created_at,
        updated_at: node.updated_at,
        knowledge: Some(DecisionKnowledge {
            lifecycle: node.status,
            confidence: node.confidence,
            actor: node.actor,
            sources: node.sources.unwrap_or_default(),
            effective_from: node.effective_from,
            effective_to: node.effective_to,
        }),
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/core/state_knowledge_tests.rs"]
mod knowledge_tests;

// ============================================================================
// Failed Paths Resolution
// ============================================================================

fn resolve_failed_paths(
    project_root: &Path,
    identity: &StateIdentity,
) -> Result<Vec<FailedPath>, error::DecapodError> {
    let mut failed_paths = Vec::new();

    // From trajectory loops with failed verification
    if let Ok(Some(trajectory_artifact)) = trajectory::load_trajectory_cookie(project_root) {
        for loop_rec in &trajectory_artifact.loops {
            if matches!(
                loop_rec.status,
                trajectory::TrajectoryLoopStatus::Failed
                    | trajectory::TrajectoryLoopStatus::Blocked
            ) && loop_rec.loop_type == trajectory::TrajectoryLoopType::Verification
            {
                failed_paths.push(FailedPath {
                    id: format!("failed:{}", loop_rec.loop_id),
                    hypothesis_or_approach: loop_rec.observations.join("; "),
                    scope: trajectory_artifact.repo_scope.clone(),
                    attempted_at: chrono::Utc::now().to_rfc3339(),
                    rejection_reason: loop_rec.feedback.clone(),
                    evidence: loop_rec.proof_refs.clone(),
                    superseded: false,
                });
            }
        }
    }

    // From workunit proof results that failed
    if let Some(task_id) = &identity.task_id
        && let Ok(manifest) = workunit::load_workunit(project_root, task_id)
    {
        for proof_result in &manifest.proof_results {
            if proof_result.status == "fail" {
                failed_paths.push(FailedPath {
                    id: format!("failed:proof:{}", proof_result.gate),
                    hypothesis_or_approach: format!("Proof gate '{}' failed", proof_result.gate),
                    scope: vec![proof_result.gate.clone()],
                    attempted_at: chrono::Utc::now().to_rfc3339(),
                    rejection_reason: format!("Gate '{}' failed", proof_result.gate),
                    evidence: proof_result
                        .artifact_ref
                        .as_ref()
                        .map(|r| vec![r.clone()])
                        .unwrap_or_default(),
                    superseded: false,
                });
            }
        }
    }

    // From knowledge candidates with rejected status
    if let Ok(custody_state) = load_custody_state(project_root) {
        for candidate in &custody_state.knowledge_candidates {
            if candidate.status == "rejected" {
                failed_paths.push(FailedPath {
                    id: candidate.id.clone(),
                    hypothesis_or_approach: candidate.lesson.clone(),
                    scope: vec![],
                    attempted_at: chrono::Utc::now().to_rfc3339(),
                    rejection_reason: "Knowledge candidate rejected".to_string(),
                    evidence: candidate.evidence_refs.clone(),
                    superseded: false,
                });
            }
        }
    }

    Ok(failed_paths)
}

// ============================================================================
// Blockers Resolution
// ============================================================================

fn resolve_blockers(
    project_root: &Path,
    identity: &StateIdentity,
) -> Result<Vec<BlockerInfo>, error::DecapodError> {
    let mut blockers = Vec::new();

    // From trajectory blockers
    if let Ok(Some(trajectory_artifact)) = trajectory::load_trajectory_cookie(project_root) {
        for (i, blocker) in trajectory_artifact.blockers.iter().enumerate() {
            let kind = classify_blocker(blocker);
            blockers.push(BlockerInfo {
                id: format!("trajectory:blocker:{}", i),
                kind,
                description: blocker.clone(),
                evidence: Vec::new(),
                declared_at: chrono::Utc::now().to_rfc3339(),
                resolved_at: None,
            });
        }
    }

    // From workunit status
    if let Some(task_id) = &identity.task_id
        && let Ok(manifest) = workunit::load_workunit(project_root, task_id)
        && manifest.status == workunit::WorkUnitStatus::Verified
    {
        // Check if any proof plan gates are missing
        for gate in &manifest.proof_plan {
            let has_pass = manifest
                .proof_results
                .iter()
                .any(|r| &r.gate == gate && r.status == "pass");
            if !has_pass {
                blockers.push(BlockerInfo {
                    id: format!("workunit:missing_proof:{}", gate),
                    kind: BlockerKind::ProofUnavailable,
                    description: format!("Required proof gate '{}' not satisfied", gate),
                    evidence: Vec::new(),
                    declared_at: chrono::Utc::now().to_rfc3339(),
                    resolved_at: None,
                });
            }
        }
    }

    // From obligation graph
    if let Ok(obligations) = obligation::list_obligations(&Store {
        kind: StoreKind::Repo,
        root: project_root.join(".decapod").join("data"),
    }) {
        for obl in obligations {
            let result = obligation::derive_obligation_status(
                &Store {
                    kind: StoreKind::Repo,
                    root: project_root.join(".decapod").join("data"),
                },
                &obl.id,
            )?;
            if result.derived_status != obligation::ObligationStatus::Met {
                for error in &result.validation_errors {
                    let kind = if error.contains("Dependency not met") {
                        BlockerKind::DependencyUnresolved
                    } else if error.contains("Proof not satisfied") {
                        BlockerKind::ProofUnavailable
                    } else if error.contains("STATE_COMMIT root missing") {
                        BlockerKind::AuthorityRequired
                    } else {
                        BlockerKind::ExternalDependency
                    };
                    blockers.push(BlockerInfo {
                        id: format!("obligation:blocker:{}", obl.id),
                        kind,
                        description: error.clone(),
                        evidence: Vec::new(),
                        declared_at: chrono::Utc::now().to_rfc3339(),
                        resolved_at: None,
                    });
                }
            }
        }
    }

    // From governance artifact inventory
    if let Ok(inventory) = governance_artifacts::inventory(project_root, None, false) {
        for artifact in &inventory.artifacts {
            if !artifact.valid
                || artifact.semantic_freshness == governance_artifacts::SemanticFreshness::Stale
            {
                blockers.push(BlockerInfo {
                    id: format!("governance:artifact:{}", artifact.path),
                    kind: BlockerKind::AuthorityRequired,
                    description: format!(
                        "Governance artifact '{}' is invalid or stale: {}",
                        artifact.role, artifact.remediation
                    ),
                    evidence: artifact.freshness_reasons.clone(),
                    declared_at: chrono::Utc::now().to_rfc3339(),
                    resolved_at: None,
                });
            }
        }
    }

    Ok(blockers)
}

fn classify_blocker(description: &str) -> BlockerKind {
    let lower = description.to_ascii_lowercase();
    if lower.contains("approval") || lower.contains("authority") || lower.contains("permission") {
        BlockerKind::AuthorityRequired
    } else if lower.contains("human") || lower.contains("decision") || lower.contains("review") {
        BlockerKind::HumanDecision
    } else if lower.contains("environment")
        || lower.contains("env ")
        || lower.contains("docker")
        || lower.contains("container")
    {
        BlockerKind::Environment
    } else if lower.contains("proof") || lower.contains("test") || lower.contains("validation") {
        BlockerKind::ProofUnavailable
    } else if lower.contains("depend") || lower.contains("block") {
        BlockerKind::DependencyUnresolved
    } else if lower.contains("scope") || lower.contains("boundary") {
        BlockerKind::ScopeViolation
    } else if lower.contains("policy") || lower.contains("rule") || lower.contains("governance") {
        BlockerKind::PolicyViolation
    } else {
        BlockerKind::ExternalDependency
    }
}

// ============================================================================
// Dependencies Resolution
// ============================================================================

fn resolve_dependencies(
    project_root: &Path,
    identity: &StateIdentity,
) -> Result<Vec<DependencyInfo>, error::DecapodError> {
    let _ = identity;
    let mut dependencies = Vec::new();

    // From obligation graph
    if let Ok(obligations) = obligation::list_obligations(&Store {
        kind: StoreKind::Repo,
        root: project_root.join(".decapod").join("data"),
    }) {
        for obl in obligations {
            let deps = obligation::get_dependencies(
                &Store {
                    kind: StoreKind::Repo,
                    root: project_root.join(".decapod").join("data"),
                },
                &obl.id,
            )?;
            for dep in deps {
                dependencies.push(DependencyInfo {
                    id: format!("obligation:{}->{}", obl.id, dep.id),
                    description: format!("Obligation {} depends on {}", obl.id, dep.id),
                    kind: DependencyKind::InternalComponent,
                    required_by: vec![obl.id.clone()],
                    status: DependencyStatus::Unresolved, // Would need to check actual status
                    evidence: Vec::new(),
                });
            }
        }
    }

    // From trajectory scope
    if let Ok(Some(trajectory_artifact)) = trajectory::load_trajectory_cookie(project_root) {
        for (i, scope_item) in trajectory_artifact.repo_scope.iter().enumerate() {
            dependencies.push(DependencyInfo {
                id: format!("trajectory:scope:{}", i),
                description: format!("Trajectory scope: {}", scope_item),
                kind: DependencyKind::InternalComponent,
                required_by: vec![trajectory_artifact.run_id.clone()],
                status: DependencyStatus::InProgress,
                evidence: Vec::new(),
            });
        }
    }

    // From workunit spec_refs / state_refs
    if let Some(task_id) = &identity.task_id
        && let Ok(manifest) = workunit::load_workunit(project_root, task_id)
    {
        for (i, spec_ref) in manifest.spec_refs.iter().enumerate() {
            dependencies.push(DependencyInfo {
                id: format!("workunit:spec:{}", i),
                description: format!("Workunit spec reference: {}", spec_ref),
                kind: DependencyKind::Verifier,
                required_by: vec![task_id.clone()],
                status: DependencyStatus::InProgress,
                evidence: vec![spec_ref.clone()],
            });
        }
    }

    Ok(dependencies)
}

// ============================================================================
// Changed Surfaces Resolution
// ============================================================================

fn resolve_changed_surfaces(
    project_root: &Path,
    identity: &StateIdentity,
) -> Result<ChangedSurfaces, error::DecapodError> {
    let _ = identity;
    let mut files = Vec::new();
    let mut config_surfaces = Vec::new();
    let mut proof_surfaces = Vec::new();
    let mut evaluator_surfaces = Vec::new();
    let mut spec_surfaces = Vec::new();
    let mut governance_surfaces = Vec::new();

    // From git diff against base
    if let Ok(Some(base)) = git_merge_base(project_root) {
        let diff_output = std::process::Command::new("git")
            .current_dir(project_root)
            .args(["diff", "--name-only", &base, "HEAD"])
            .output()
            .map_err(error::DecapodError::IoError)?;
        files = String::from_utf8_lossy(&diff_output.stdout)
            .lines()
            .map(|s| s.to_string())
            .collect();
    }

    // Classify changed files
    for file in &files {
        if file.contains(".decapod/config.toml")
            || file.ends_with("config.toml")
            || file.ends_with("proofs.toml")
        {
            config_surfaces.push(file.clone());
        }
        if file.contains("proof") || file.contains("test") || file.contains("spec") {
            proof_surfaces.push(file.clone());
        }
        if file.contains(".decapod/") && (file.contains("override") || file.contains("policy")) {
            evaluator_surfaces.push(file.clone());
        }
        if file.contains("managed/specs/")
            || file.contains("INTENT.md")
            || file.contains("ARCHITECTURE.md")
            || file.contains("INTERFACES.md")
        {
            spec_surfaces.push(file.clone());
        }
        if file.contains("governance.json") || file.contains(".decapod/governance/") {
            governance_surfaces.push(file.clone());
        }
    }

    // From trajectory modified_files
    if let Ok(Some(trajectory_artifact)) = trajectory::load_trajectory_cookie(project_root) {
        for file in &trajectory_artifact.modified_files {
            if !files.contains(file) {
                files.push(file.clone());
            }
        }
    }

    Ok(ChangedSurfaces {
        files: dedup_sorted(files),
        config_surfaces: dedup_sorted(config_surfaces),
        proof_surfaces: dedup_sorted(proof_surfaces),
        evaluator_surfaces: dedup_sorted(evaluator_surfaces),
        spec_surfaces: dedup_sorted(spec_surfaces),
        governance_surfaces: dedup_sorted(governance_surfaces),
    })
}

// ============================================================================
// Validation & Proof Resolution
// ============================================================================

fn resolve_validation_proof(
    project_root: &Path,
    identity: &StateIdentity,
    _params: &StateResolveParams,
) -> Result<(ValidationProofProjection, ValidationProofProjection), error::DecapodError> {
    let mut proof_states = Vec::new();
    let mut proof_plan = Vec::new();
    let verifier_integrity = resolve_verifier_integrity(project_root)?;
    let assurance_policy = resolve_assurance_policy(project_root)?;

    // From workunit proof plan and results
    if let Some(task_id) = &identity.task_id
        && let Ok(manifest) = workunit::load_workunit(project_root, task_id)
    {
        proof_plan = manifest.proof_plan.clone();
        for gate in &manifest.proof_plan {
            let result = manifest.proof_results.iter().find(|r| &r.gate == gate);
            let (freshness, freshness_reasons) =
                compute_proof_freshness(project_root, gate, result, identity)?;
            proof_states.push(ProofState {
                gate: gate.clone(),
                name: gate.clone(),
                required: true, // Would come from proof config
                freshness,
                last_run_at: None, // Would come from proof events
                last_result: result.map(|r| r.status.clone()),
                artifact_ref: result.and_then(|r| r.artifact_ref.clone()),
                evaluator_epoch: result.and_then(|r| r.evaluator_epoch.clone()),
                validation_epoch: result.and_then(|r| r.validation_epoch.clone()),
                freshness_reasons,
                assurance_level: None,
                verifier_identity: None,
            });
        }
    }

    // From proof config
    if let Ok(resolved) = proof::resolve_proof_config(project_root) {
        for proof_def in &resolved.config.proof {
            if !proof_plan.contains(&proof_def.name) {
                proof_plan.push(proof_def.name.clone());
            }
            let existing = proof_states.iter().find(|p| p.gate == proof_def.name);
            if existing.is_none() {
                let (freshness, freshness_reasons) =
                    compute_proof_freshness(project_root, &proof_def.name, None, identity)?;
                proof_states.push(ProofState {
                    gate: proof_def.name.clone(),
                    name: proof_def.name.clone(),
                    required: proof_def.required,
                    freshness,
                    last_run_at: None,
                    last_result: None,
                    artifact_ref: None,
                    evaluator_epoch: None,
                    validation_epoch: None,
                    freshness_reasons,
                    assurance_level: None,
                    verifier_identity: None,
                });
            }
        }
    }

    // Validation receipt
    let validation_receipt = if let Ok(Some(receipt)) =
        governance_document::read_section(project_root, "validation")
    {
        if let Ok(receipt_obj) = serde_json::from_value::<validate::ValidationReceipt>(receipt) {
            Some(ValidationReceiptSummary {
                status: receipt_obj.status,
                pass_count: receipt_obj.pass_count,
                fail_count: receipt_obj.fail_count,
                git_revision: receipt_obj.git_revision,
                trajectory_run_id: receipt_obj.trajectory_run_id,
                receipt_hash: receipt_obj.receipt_hash,
            })
        } else {
            None
        }
    } else {
        None
    };

    let projection = ValidationProofProjection {
        proof_states: proof_states.clone(),
        proof_plan: proof_plan.clone(),
        validation_receipt,
        verifier_integrity: verifier_integrity.clone(),
        assurance_policy: assurance_policy.clone(),
    };

    Ok((projection.clone(), projection))
}

fn compute_proof_freshness(
    project_root: &Path,
    gate: &str,
    result: Option<&workunit::WorkUnitProofResult>,
    identity: &StateIdentity,
) -> Result<(ProofFreshness, Vec<String>), error::DecapodError> {
    let mut reasons = Vec::new();

    // If no result ever recorded
    let Some(result) = result else {
        return Ok((
            ProofFreshness::Unknown,
            vec!["No proof result recorded".to_string()],
        ));
    };

    // If result is fail
    if result.status == "fail" {
        return Ok((
            ProofFreshness::Failed,
            vec!["Proof previously failed".to_string()],
        ));
    }

    // Check if governed state has changed since proof was recorded
    // This is a simplified version - in practice would use state commit / impact prediction
    let has_relevant_changes = check_relevant_changes_since_proof(project_root, gate, identity)?;
    if has_relevant_changes {
        reasons.push("Relevant governed surfaces changed since proof".to_string());
        return Ok((ProofFreshness::Stale, reasons));
    }

    // Check if evaluator changed
    if evaluator_changed_since_proof(project_root, gate)? {
        reasons.push("Evaluator surface changed since proof".to_string());
        return Ok((ProofFreshness::Stale, reasons));
    }

    // Check if validation epoch matches
    if let Some(epoch) = &result.validation_epoch
        && let Ok(active_epoch) = validation_epoch::active_validation_epoch(project_root)
        && epoch != &active_epoch
    {
        reasons.push("Validation epoch advanced since proof".to_string());
        return Ok((ProofFreshness::Stale, reasons));
    }

    Ok((ProofFreshness::Valid, reasons))
}

fn check_relevant_changes_since_proof(
    project_root: &Path,
    gate: &str,
    identity: &StateIdentity,
) -> Result<bool, error::DecapodError> {
    // Simplified: check if any files in the proof's scope have changed
    // In reality would use impact prediction / bloom filters / Laya
    if let Ok(changed) = resolve_changed_surfaces(project_root, identity) {
        // Check if proof config references any changed surfaces
        if let Ok(resolved) = proof::resolve_proof_config(project_root) {
            for proof_def in &resolved.config.proof {
                if proof_def.name == gate {
                    // Proof command might reference files - simplified check
                    return Ok(!changed.files.is_empty());
                }
            }
        }
    }
    Ok(false)
}

fn evaluator_changed_since_proof(
    project_root: &Path,
    _gate: &str,
) -> Result<bool, error::DecapodError> {
    // Check if validation config, CI workflows, proof scripts changed since base
    if let Ok(Some(base)) = git_merge_base(project_root) {
        let output = std::process::Command::new("git")
            .current_dir(project_root)
            .args(["diff", "--name-only", &base, "HEAD"])
            .output()
            .map_err(error::DecapodError::IoError)?;
        let files = String::from_utf8_lossy(&output.stdout);
        for line in files.lines() {
            if line.contains(".github/workflows/")
                || line.contains(".decapod/config.toml")
                || line.contains("proofs.toml")
                || line.contains(".decapod/override/")
                || line.contains("validate")
                || line.contains("test")
                || line.ends_with(".yml")
                || line.ends_with(".yaml")
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn resolve_verifier_integrity(
    project_root: &Path,
) -> Result<VerifierIntegrity, error::DecapodError> {
    if let Ok(Some(base)) = git_merge_base(project_root) {
        let output = std::process::Command::new("git")
            .current_dir(project_root)
            .args(["diff", "--name-only", &base, "HEAD"])
            .output()
            .map_err(error::DecapodError::IoError)?;
        let files = String::from_utf8_lossy(&output.stdout);
        let mut evaluator_changed = false;
        let mut authorized = true;
        for line in files.lines() {
            if line.contains(".github/workflows/")
                || line.contains(".decapod/config.toml")
                || line.contains("proofs.toml")
                || line.contains(".decapod/override/")
                || line.contains("validate")
            {
                evaluator_changed = true;
                // Check if change is authorized (in governance plan/approvals)
                // Simplified: assume unauthorized unless in plan
                authorized = false;
            }
        }
        if evaluator_changed {
            return Ok(if authorized {
                VerifierIntegrity::ChangedAuthorized
            } else {
                VerifierIntegrity::ChangedUntrusted
            });
        }
    }
    Ok(VerifierIntegrity::Unchanged)
}

fn resolve_assurance_policy(
    project_root: &Path,
) -> Result<Option<AssurancePolicy>, error::DecapodError> {
    let _ = crate::cli::DecapodProjectConfig::load(project_root);
    // Would read from config if present
    // For now, return None (no policy enforced)
    Ok(None)
}

// ============================================================================
// Unresolved Obligations Resolution
// ============================================================================

fn resolve_unresolved_obligations(
    project_root: &Path,
    identity: &StateIdentity,
    params: &StateResolveParams,
) -> Result<Vec<UnresolvedObligation>, error::DecapodError> {
    let _ = identity;
    let mut obligations = Vec::new();

    if params.include_obligation_graph.unwrap_or(false) {
        let store = Store {
            kind: StoreKind::Repo,
            root: project_root.join(".decapod").join("data"),
        };
        if let Ok(obl_list) = obligation::list_obligations(&store) {
            for obl in obl_list {
                let result = obligation::derive_obligation_status(&store, &obl.id)?;
                if result.derived_status != obligation::ObligationStatus::Met {
                    obligations.push(UnresolvedObligation {
                        id: obl.id.clone(),
                        intent_ref: obl.intent_ref.clone(),
                        risk_tier: obl.risk_tier.clone(),
                        required_proofs: obl.required_proofs.clone(),
                        dependencies_satisfied: result.dependencies_satisfied,
                        proofs_satisfied: result.proofs_satisfied,
                        commit_present: result.commit_present,
                        blocking_dependencies: Vec::new(), // Would compute from graph
                        missing_proofs: Vec::new(),        // Would compute from health
                    });
                }
            }
        }
    }

    Ok(obligations)
}

// ============================================================================
// Publication Eligibility Resolution
// ============================================================================

fn resolve_publication(
    project_root: &Path,
    identity: &StateIdentity,
    validation: &ValidationProofProjection,
) -> Result<PublicationEligibility, error::DecapodError> {
    let mut blocking_factors = Vec::new();

    // Check required proofs current
    let required_proofs_current = validation
        .proof_states
        .iter()
        .filter(|p| p.required)
        .all(|p| p.freshness == ProofFreshness::Valid || p.freshness == ProofFreshness::Unaffected);

    if !required_proofs_current {
        blocking_factors.push("Required proofs are not current".to_string());
    }

    // Check verifier integrity
    let verifier_integrity_ok = matches!(
        validation.verifier_integrity,
        VerifierIntegrity::Unchanged | VerifierIntegrity::ChangedAuthorized
    );
    if !verifier_integrity_ok {
        blocking_factors.push(
            "Verifier integrity compromised: evaluator changed without authorization".to_string(),
        );
    }

    // Check state commit present
    let state_commit_present = check_state_commit_present(project_root, identity)?;

    if !state_commit_present {
        blocking_factors.push("State commit not present".to_string());
    }

    // Check assurance requirements
    let assurance_requirements_met = if let Some(policy) = &validation.assurance_policy {
        validation
            .proof_states
            .iter()
            .filter(|p| p.required)
            .all(|p| {
                p.assurance_level.as_ref().is_some_and(|level| {
                    matches!(
                        (level, &policy.minimum_level),
                        (AssuranceLevel::A0Structural, AssuranceLevel::A0Structural)
                            | (AssuranceLevel::A1Behavioral, AssuranceLevel::A0Structural)
                            | (AssuranceLevel::A1Behavioral, AssuranceLevel::A1Behavioral)
                            | (
                                AssuranceLevel::A2IsolatedReplayed,
                                AssuranceLevel::A0Structural
                            )
                            | (
                                AssuranceLevel::A2IsolatedReplayed,
                                AssuranceLevel::A1Behavioral
                            )
                            | (
                                AssuranceLevel::A2IsolatedReplayed,
                                AssuranceLevel::A2IsolatedReplayed
                            )
                            | (AssuranceLevel::A3IndependentlyVerified, _)
                    )
                })
            })
    } else {
        true // No policy = met
    };

    if !assurance_requirements_met {
        blocking_factors.push("Assurance level requirements not met".to_string());
    }

    let eligible = required_proofs_current
        && verifier_integrity_ok
        && state_commit_present
        && assurance_requirements_met;

    let reason = if eligible {
        "All publication requirements satisfied".to_string()
    } else {
        blocking_factors.join("; ")
    };

    Ok(PublicationEligibility {
        eligible,
        reason,
        required_proofs_current,
        verifier_integrity_ok,
        state_commit_present,
        assurance_requirements_met,
        blocking_factors,
    })
}

fn check_state_commit_present(
    project_root: &Path,
    identity: &StateIdentity,
) -> Result<bool, error::DecapodError> {
    // Check workunit for state commit
    if let Some(task_id) = &identity.task_id
        && let Ok(_manifest) = workunit::load_workunit(project_root, task_id)
    {
        // Workunit doesn't directly store state commit, but obligation does
    }

    // Check obligation for state commit
    let store = Store {
        kind: StoreKind::Repo,
        root: project_root.join(".decapod").join("data"),
    };
    if let Ok(obligations) = obligation::list_obligations(&store) {
        for obl in obligations {
            if obl.state_commit_root.is_some()
                && !obl.state_commit_root.as_ref().unwrap().is_empty()
            {
                return Ok(true);
            }
        }
    }

    // Check for state commit artifact
    let state_commit_path = project_root
        .join(".decapod")
        .join("artifacts")
        .join("state_commit.json");
    Ok(state_commit_path.exists())
}

// ============================================================================
// Admissible Actions Resolution
// ============================================================================

fn resolve_admissible_actions(
    project_root: &Path,
    identity: &StateIdentity,
    validation: &ValidationProofProjection,
    publication: &PublicationEligibility,
    blockers: &[BlockerInfo],
    unresolved_obligations: &[UnresolvedObligation],
) -> Result<AdmissibleActions, error::DecapodError> {
    let _ = identity;
    let mut allowed = Vec::new();
    let mut required = Vec::new();
    let mut forbidden = Vec::new();

    // Base allowed actions
    allowed.push(AllowedAction {
        action: "inspect".to_string(),
        reason: "Read-only inspection is always permitted".to_string(),
        scope_constraints: Vec::new(),
    });

    allowed.push(AllowedAction {
        action: "modify within scope".to_string(),
        reason: "Modifications within active boundaries are permitted".to_string(),
        scope_constraints: validation
            .proof_states
            .iter()
            .map(|p| p.gate.clone())
            .collect(),
    });

    allowed.push(AllowedAction {
        action: "validate".to_string(),
        reason: "Running validation is always permitted".to_string(),
        scope_constraints: Vec::new(),
    });

    allowed.push(AllowedAction {
        action: "submit evidence".to_string(),
        reason: "Submitting proof evidence is permitted".to_string(),
        scope_constraints: Vec::new(),
    });

    allowed.push(AllowedAction {
        action: "request approval".to_string(),
        reason: "Requesting human approval is permitted".to_string(),
        scope_constraints: Vec::new(),
    });

    // Required actions based on blockers
    for blocker in blockers {
        match blocker.kind {
            BlockerKind::ProofUnavailable => {
                required.push(RequiredAction {
                    action: format!("resolve proof: {}", blocker.description),
                    reason: "Required proof is missing or stale".to_string(),
                    blocking_obligation: Some(blocker.id.clone()),
                });
            }
            BlockerKind::DependencyUnresolved => {
                required.push(RequiredAction {
                    action: format!("resolve dependency: {}", blocker.description),
                    reason: "Unresolved dependency blocking progress".to_string(),
                    blocking_obligation: Some(blocker.id.clone()),
                });
            }
            BlockerKind::AuthorityRequired => {
                required.push(RequiredAction {
                    action: format!("obtain authority: {}", blocker.description),
                    reason: "Authority required to proceed".to_string(),
                    blocking_obligation: Some(blocker.id.clone()),
                });
            }
            BlockerKind::HumanDecision => {
                required.push(RequiredAction {
                    action: format!("request human decision: {}", blocker.description),
                    reason: "Human decision required".to_string(),
                    blocking_obligation: Some(blocker.id.clone()),
                });
            }
            _ => {}
        }
    }

    // Required actions from unresolved obligations
    for obl in unresolved_obligations {
        if !obl.dependencies_satisfied {
            required.push(RequiredAction {
                action: format!("satisfy dependencies for obligation {}", obl.id),
                reason: "Obligation dependencies not met".to_string(),
                blocking_obligation: Some(obl.id.clone()),
            });
        }
        if !obl.proofs_satisfied {
            required.push(RequiredAction {
                action: format!("complete required proofs for obligation {}", obl.id),
                reason: "Obligation proofs not satisfied".to_string(),
                blocking_obligation: Some(obl.id.clone()),
            });
        }
        if !obl.commit_present {
            required.push(RequiredAction {
                action: format!("record state commit for obligation {}", obl.id),
                reason: "Obligation missing state commit".to_string(),
                blocking_obligation: Some(obl.id.clone()),
            });
        }
    }

    // Forbidden actions
    if !publication.eligible {
        forbidden.push(ForbiddenAction {
            action: "publish".to_string(),
            reason: publication.reason.clone(),
            blocker: Some("publication_not_eligible".to_string()),
        });
    }

    if validation.verifier_integrity == VerifierIntegrity::ChangedUntrusted {
        forbidden.push(ForbiddenAction {
            action: "claim completion".to_string(),
            reason: "Evaluator surface changed without authorization; completion not falsifiable"
                .to_string(),
            blocker: Some("verifier_integrity_compromised".to_string()),
        });
    }

    // Check if workunit status allows mutation
    if let Some(task_id) = &identity.task_id
        && let Ok(manifest) = workunit::load_workunit(project_root, task_id)
        && manifest.status == workunit::WorkUnitStatus::Verified
    {
        forbidden.push(ForbiddenAction {
            action: "modify".to_string(),
            reason: "Workunit is VERIFIED; modifications require new proof cycle".to_string(),
            blocker: Some("workunit_verified".to_string()),
        });
    }

    let reason = if publication.eligible && required.is_empty() {
        "All obligations satisfied; publication and completion are admissible".to_string()
    } else if !required.is_empty() {
        "Required actions must be completed before publication".to_string()
    } else {
        "Publication blocked by outstanding requirements".to_string()
    };

    Ok(AdmissibleActions {
        allowed,
        required,
        forbidden,
        reason,
    })
}

// ============================================================================
// Provenance Resolution
// ============================================================================

fn resolve_provenance(
    project_root: &Path,
    identity: &StateIdentity,
) -> Result<ProvenanceInfo, error::DecapodError> {
    let _ = identity;
    let mut state_commit_root = None;
    let mut scope_record_hash = None;
    let mut governance_document_hash = None;
    let mut trajectory_artifact_hash = None;
    let mut workunit_manifest_hash = None;
    let mut validation_receipt_hash = None;

    // State commit
    let state_commit_path = project_root
        .join(".decapod")
        .join("artifacts")
        .join("state_commit.json");
    if state_commit_path.exists()
        && let Ok(content) = std::fs::read_to_string(&state_commit_path)
        && let Ok(json) = serde_json::from_str::<serde_json::Value>(&content)
    {
        state_commit_root = json
            .get("state_commit_root")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        scope_record_hash = json
            .get("scope_record_hash")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
    }

    // Governance document hash
    if let Ok(Some(doc)) = governance_document::load(project_root)
        && let Ok(bytes) = serde_json::to_vec(&doc)
    {
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        governance_document_hash = Some(format!("sha256:{:x}", hasher.finalize()));
    }

    // Trajectory artifact hash
    if let Ok(Some(trajectory)) = trajectory::load_trajectory_cookie(project_root) {
        trajectory_artifact_hash = Some(trajectory.artifact_hash.clone());
    }

    // Workunit manifest hash
    if let Some(task_id) = &identity.task_id
        && let Ok(manifest) = workunit::load_workunit(project_root, task_id)
        && let Ok(hash) = manifest.canonical_hash_hex()
    {
        workunit_manifest_hash = Some(hash);
    }

    // Validation receipt hash
    if let Ok(Some(receipt)) = governance_document::read_section(project_root, "validation")
        && let Ok(receipt_obj) = serde_json::from_value::<validate::ValidationReceipt>(receipt)
    {
        validation_receipt_hash = Some(receipt_obj.receipt_hash.clone());
    }

    Ok(ProvenanceInfo {
        state_commit_root,
        scope_record_hash,
        governance_document_hash,
        trajectory_artifact_hash,
        workunit_manifest_hash,
        validation_receipt_hash,
        resolved_at: chrono::Utc::now().to_rfc3339(),
        decapod_version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Revision and provenance from the canonical resolver.
///
/// Callers that judge proof must use this snapshot. They must not invent a
/// second head, rules fingerprint, or verifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalSnapshot {
    pub revision: RevisionInfo,
    pub provenance: ProvenanceInfo,
}

pub fn canonical_snapshot(project_root: &Path) -> Result<CanonicalSnapshot, error::DecapodError> {
    let revision = resolve_revision(project_root)?;
    let identity = StateIdentity {
        kind: StateIdentityKind::Intent,
        id: "canonical".to_string(),
        intent_ref: None,
        task_id: None,
    };
    let provenance = resolve_provenance(project_root, &identity)?;
    Ok(CanonicalSnapshot {
        revision,
        provenance,
    })
}

// ============================================================================
// State Diff Computation
// ============================================================================

fn compute_state_diff(
    from: &StateResolution,
    to: &StateResolution,
) -> Result<StateDiff, error::DecapodError> {
    let mut intent_changes = Vec::new();
    let mut decision_changes = Vec::new();
    let mut failed_path_changes = Vec::new();
    let mut custody_changes = Vec::new();
    let mut scope_changes = Vec::new();
    let mut dependency_changes = Vec::new();
    let mut blocker_changes = Vec::new();
    let mut proof_added = Vec::new();
    let mut proof_preserved = Vec::new();
    let mut proof_invalidated = Vec::new();
    let mut proof_unknown = Vec::new();

    // Intent changes
    if from.objective.original_intent != to.objective.original_intent {
        intent_changes.push(IntentChange {
            field: "original_intent".to_string(),
            from: Some(from.objective.original_intent.clone()),
            to: Some(to.objective.original_intent.clone()),
        });
    }
    if from.objective.derived_intent != to.objective.derived_intent {
        intent_changes.push(IntentChange {
            field: "derived_intent".to_string(),
            from: from.objective.derived_intent.clone(),
            to: to.objective.derived_intent.clone(),
        });
    }
    if from.objective.acceptance_criteria != to.objective.acceptance_criteria {
        intent_changes.push(IntentChange {
            field: "acceptance_criteria".to_string(),
            from: Some(serde_json::to_string(&from.objective.acceptance_criteria).unwrap()),
            to: Some(serde_json::to_string(&to.objective.acceptance_criteria).unwrap()),
        });
    }

    // Decision changes
    let from_decisions: BTreeMap<_, _> = from.decisions.iter().map(|d| (d.id.clone(), d)).collect();
    let to_decisions: BTreeMap<_, _> = to.decisions.iter().map(|d| (d.id.clone(), d)).collect();

    for (id, to_dec) in &to_decisions {
        if let Some(from_dec) = from_decisions.get(id) {
            if from_dec.status != to_dec.status {
                decision_changes.push(DecisionChange {
                    decision_id: id.clone(),
                    change_type: DecisionChangeType::StatusChanged,
                    from_status: Some(from_dec.status.clone()),
                    to_status: Some(to_dec.status.clone()),
                });
            }
        } else {
            decision_changes.push(DecisionChange {
                decision_id: id.clone(),
                change_type: DecisionChangeType::Added,
                from_status: None,
                to_status: Some(to_dec.status.clone()),
            });
        }
    }
    for (id, from_dec) in &from_decisions {
        if !to_decisions.contains_key(id) {
            decision_changes.push(DecisionChange {
                decision_id: id.clone(),
                change_type: DecisionChangeType::Removed,
                from_status: Some(from_dec.status.clone()),
                to_status: None,
            });
        }
    }

    // Failed path changes
    let from_failed: BTreeMap<_, _> = from
        .failed_paths
        .iter()
        .map(|f| (f.id.clone(), f))
        .collect();
    let to_failed: BTreeMap<_, _> = to.failed_paths.iter().map(|f| (f.id.clone(), f)).collect();

    for id in to_failed.keys() {
        if !from_failed.contains_key(id) {
            failed_path_changes.push(FailedPathChange {
                failed_path_id: id.clone(),
                change_type: FailedPathChangeType::Added,
            });
        }
    }
    for id in from_failed.keys() {
        if !to_failed.contains_key(id) {
            failed_path_changes.push(FailedPathChange {
                failed_path_id: id.clone(),
                change_type: FailedPathChangeType::Removed,
            });
        }
    }

    // Custody changes
    if from.custody.intent_status != to.custody.intent_status {
        custody_changes.push(CustodyChange {
            field: "intent_status".to_string(),
            from: Some(format!("{:?}", from.custody.intent_status)),
            to: Some(format!("{:?}", to.custody.intent_status)),
        });
    }
    if from.custody.refined_intent != to.custody.refined_intent {
        custody_changes.push(CustodyChange {
            field: "refined_intent".to_string(),
            from: from.custody.refined_intent.clone(),
            to: to.custody.refined_intent.clone(),
        });
    }

    // Scope changes
    scope_changes.push(ScopeChange {
        field: "active_boundaries".to_string(),
        from: from.scope.active_boundaries.clone(),
        to: to.scope.active_boundaries.clone(),
    });
    scope_changes.push(ScopeChange {
        field: "repo_scope".to_string(),
        from: from.scope.repo_scope.clone(),
        to: to.scope.repo_scope.clone(),
    });

    // Dependency changes
    let from_deps: BTreeMap<_, _> = from
        .dependencies
        .iter()
        .map(|d| (d.id.clone(), d))
        .collect();
    let to_deps: BTreeMap<_, _> = to.dependencies.iter().map(|d| (d.id.clone(), d)).collect();

    for (id, to_dep) in &to_deps {
        if let Some(from_dep) = from_deps.get(id) {
            if from_dep.status != to_dep.status {
                dependency_changes.push(DependencyChange {
                    dependency_id: id.clone(),
                    change_type: DependencyChangeType::StatusChanged,
                    from_status: Some(from_dep.status.clone()),
                    to_status: Some(to_dep.status.clone()),
                });
            }
        } else {
            dependency_changes.push(DependencyChange {
                dependency_id: id.clone(),
                change_type: DependencyChangeType::Added,
                from_status: None,
                to_status: Some(to_dep.status.clone()),
            });
        }
    }
    for (id, from_dep) in &from_deps {
        if !to_deps.contains_key(id) {
            dependency_changes.push(DependencyChange {
                dependency_id: id.clone(),
                change_type: DependencyChangeType::Removed,
                from_status: Some(from_dep.status.clone()),
                to_status: None,
            });
        }
    }

    // Blocker changes
    let from_blockers: BTreeMap<_, _> = from.blockers.iter().map(|b| (b.id.clone(), b)).collect();
    let to_blockers: BTreeMap<_, _> = to.blockers.iter().map(|b| (b.id.clone(), b)).collect();

    for (id, to_blocker) in &to_blockers {
        if let Some(from_blocker) = from_blockers.get(id) {
            if from_blocker.kind != to_blocker.kind {
                blocker_changes.push(BlockerChange {
                    blocker_id: id.clone(),
                    change_type: BlockerChangeType::KindChanged,
                    from_kind: Some(from_blocker.kind.clone()),
                    to_kind: Some(to_blocker.kind.clone()),
                    from_resolved: Some(from_blocker.resolved_at.is_some()),
                    to_resolved: Some(to_blocker.resolved_at.is_some()),
                });
            } else if from_blocker.resolved_at.is_some() && to_blocker.resolved_at.is_none() {
                blocker_changes.push(BlockerChange {
                    blocker_id: id.clone(),
                    change_type: BlockerChangeType::Resolved,
                    from_kind: Some(from_blocker.kind.clone()),
                    to_kind: Some(to_blocker.kind.clone()),
                    from_resolved: Some(true),
                    to_resolved: Some(false),
                });
            } else if from_blocker.resolved_at.is_none() && to_blocker.resolved_at.is_some() {
                blocker_changes.push(BlockerChange {
                    blocker_id: id.clone(),
                    change_type: BlockerChangeType::Resolved,
                    from_kind: Some(from_blocker.kind.clone()),
                    to_kind: Some(to_blocker.kind.clone()),
                    from_resolved: Some(false),
                    to_resolved: Some(true),
                });
            }
        } else {
            blocker_changes.push(BlockerChange {
                blocker_id: id.clone(),
                change_type: BlockerChangeType::Added,
                from_kind: None,
                to_kind: Some(to_blocker.kind.clone()),
                from_resolved: None,
                to_resolved: Some(to_blocker.resolved_at.is_some()),
            });
        }
    }
    for (id, from_blocker) in &from_blockers {
        if !to_blockers.contains_key(id) {
            blocker_changes.push(BlockerChange {
                blocker_id: id.clone(),
                change_type: BlockerChangeType::Removed,
                from_kind: Some(from_blocker.kind.clone()),
                to_kind: None,
                from_resolved: Some(from_blocker.resolved_at.is_some()),
                to_resolved: None,
            });
        }
    }

    // Proof changes
    let from_proofs: BTreeMap<_, _> = from
        .validation
        .proof_states
        .iter()
        .map(|p| (p.gate.clone(), p.clone()))
        .collect();
    let to_proofs: BTreeMap<_, _> = to
        .validation
        .proof_states
        .iter()
        .map(|p| (p.gate.clone(), p.clone()))
        .collect();

    for (gate, to_proof) in &to_proofs {
        if let Some(from_proof) = from_proofs.get(gate) {
            match (from_proof.freshness, to_proof.freshness) {
                (ProofFreshness::Valid, ProofFreshness::Stale) => {
                    proof_invalidated.push(to_proof.clone());
                }
                (ProofFreshness::Stale, ProofFreshness::Valid) => {
                    proof_preserved.push(to_proof.clone()); // Was stale, now valid
                }
                (ProofFreshness::Valid, ProofFreshness::Valid)
                | (ProofFreshness::Stale, ProofFreshness::Stale)
                | (ProofFreshness::Unaffected, ProofFreshness::Unaffected) => {
                    proof_preserved.push(to_proof.clone());
                }
                (ProofFreshness::Unknown, ProofFreshness::Valid)
                | (ProofFreshness::Unknown, ProofFreshness::Stale) => {
                    proof_added.push(to_proof.clone());
                }
                (ProofFreshness::Failed, ProofFreshness::Valid)
                | (ProofFreshness::Failed, ProofFreshness::Stale) => {
                    proof_preserved.push(to_proof.clone());
                }
                _ => {
                    proof_unknown.push(to_proof.clone());
                }
            }
        } else {
            proof_added.push(to_proof.clone());
        }
    }
    for (gate, from_proof) in &from_proofs {
        if !to_proofs.contains_key(gate) {
            proof_invalidated.push(from_proof.clone());
        }
    }

    // Publication transition
    let publication_transition = PublicationTransition {
        from_eligible: from.publication.eligible,
        to_eligible: to.publication.eligible,
        reason: if from.publication.eligible != to.publication.eligible {
            if to.publication.eligible {
                "Publication became eligible".to_string()
            } else {
                "Publication became ineligible".to_string()
            }
        } else {
            "Publication eligibility unchanged".to_string()
        },
    };

    Ok(StateDiff {
        from: from.clone(),
        to: to.clone(),
        intent_changes,
        decision_changes,
        failed_path_changes,
        custody_changes,
        scope_changes,
        dependency_changes,
        blocker_changes,
        changed_surfaces: to.changed_surfaces.clone(),
        proof_added,
        proof_preserved,
        proof_invalidated,
        proof_unknown,
        publication_transition,
    })
}

// ============================================================================
// Schema
// ============================================================================

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "state",
        "version": STATE_SCHEMA_VERSION,
        "description": "Deterministic governed-state projection and diff",
        "operations": ["state.resolve", "state.diff"],
        "types": {
            "StateResolution": {
                "identity": "StateIdentity",
                "revision": "RevisionInfo",
                "objective": "ObjectiveInfo",
                "acceptance": "ObjectiveInfo",
                "scope": "ScopeConstraints",
                "constraints": "ScopeConstraints",
                "custody": "CustodyProjection",
                "decisions": ["Decision"],
                "failed_paths": ["FailedPath"],
                "blockers": ["BlockerInfo"],
                "dependencies": ["DependencyInfo"],
                "changed_surfaces": "ChangedSurfaces",
                "validation": "ValidationProofProjection",
                "proof": "ValidationProofProjection",
                "unresolved_obligations": ["UnresolvedObligation"],
                "publication": "PublicationEligibility",
                "admissible_actions": "AdmissibleActions",
                "provenance": "ProvenanceInfo"
            },
            "StateDiff": {
                "from": "StateResolution",
                "to": "StateResolution",
                "intent_changes": ["IntentChange"],
                "decision_changes": ["DecisionChange"],
                "failed_path_changes": ["FailedPathChange"],
                "custody_changes": ["CustodyChange"],
                "scope_changes": ["ScopeChange"],
                "dependency_changes": ["DependencyChange"],
                "blocker_changes": ["BlockerChange"],
                "changed_surfaces": "ChangedSurfaces",
                "proof_added": ["ProofState"],
                "proof_preserved": ["ProofState"],
                "proof_invalidated": ["ProofState"],
                "proof_unknown": ["ProofState"],
                "publication_transition": "PublicationTransition"
            }
        }
    })
}
