//! Versioned, repo-native composed service capability contracts and drift
//! detection (#1906).
//!
//! Decapod uses these contracts to make cross-service capabilities (starting
//! with the authorized `Decapod -> Dactyl -> Propodus` Cloud task mutation and
//! readback slice) independently inspectable before governed admission.
//!
//! Guardrails enforced by construction:
//! - Contract declarations and generated projections are descriptive inputs,
//!   never self-issued authorization or runtime proof (`SelfVerificationRejected`).
//! - Syntactic interface compatibility, observed runtime behavior, and
//!   user-intent correctness remain explicitly separated.
//! - Composite consequences distinguish `prepared`, `observed`, `held`, and
//!   `compensated` without blind replay, erased faults, or false
//!   `goal_completed = true` under unresolved uncertainty.
//! - Local offline repositories with no Cloud binding remain unaffected and
//!   usable offline (`local_offline_usable = true`).

use crate::cli::DecapodProjectConfig;
use crate::core::error::DecapodError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Embedded JSON Schema (draft 2020-12) governing `decapod.capability-contract.v1`.
pub const CAPABILITY_CONTRACT_SCHEMA_DOCUMENT: &str =
    include_str!("../../../assets/schemas/capability-contract.schema.json");

pub const CAPABILITY_CONTRACT_SCHEMA_ID: &str =
    "https://decapod.dev/schemas/capability-contract-1.0.0.schema.json";
pub const CAPABILITY_CONTRACT_SCHEMA_VERSION: &str = "decapod.capability-contract.v1";
pub const COMPATIBILITY_REPORT_SCHEMA_VERSION: &str = "decapod.capability-compatibility-report.v1";
pub const FIXTURE_SUITE_SCHEMA_VERSION: &str = "decapod.capability-compatibility-fixture.v1";
pub const CAPABILITY_INSPECTION_SCHEMA_VERSION: &str = "decapod.capability-inspection.v1";

pub const CLOUD_TASK_CLAIM_CAPABILITY_ID: &str = "cloud.task.claim_and_readback.v1";
pub const LOCAL_TASK_OFFLINE_CAPABILITY_ID: &str = "local.task.offline.v1";
pub const CAPABILITY_CONTRACT_REL_PATH: &str = ".decapod/contracts/capability_contract.json";

const MAX_CONTRACT_FILE_BYTES: u64 = 512 * 1024;
const MAX_ID_BYTES: usize = 128;

/// Explicit status of a capability contract evaluation.
///
/// A descriptor cannot legitimately declare itself `CurrentlyVerified`; that
/// state can only emerge from current, independent, non-self-reported
/// evidence matching the composed revisions and accepted policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractStatus {
    Declared,
    Observed,
    CurrentlyVerified,
    Incompatible,
    Unsupported,
    Stale,
}

impl ContractStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Declared => "declared",
            Self::Observed => "observed",
            Self::CurrentlyVerified => "currently_verified",
            Self::Incompatible => "incompatible",
            Self::Unsupported => "unsupported",
            Self::Stale => "stale",
        }
    }
}

/// Composite consequence closure states for bounded Cloud task mutations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompositeConsequenceState {
    /// Satisfies accepted authorization and prestate requirements; no effect yet.
    Prepared,
    /// Actual owner confirmed intended poststate with independent readback.
    Observed,
    /// Protected state preserved while an uncertain write, shared-capacity
    /// exposure, or dependent async consequence remains unresolved (`goal_completed = false`).
    Held,
    /// Intermediate fault observed, supported corrective operation executed, and
    /// final selected state independently checked while retaining original fault.
    Compensated,
}

impl CompositeConsequenceState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::Observed => "observed",
            Self::Held => "held",
            Self::Compensated => "compensated",
        }
    }
}

/// Preserved consumer-visible outcome classifications that must never be
/// silently rewritten into current proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreservedOutcomeClass {
    Pending,
    Uncertain,
    Failed,
    Unsupported,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceParticipant {
    pub component: String,
    pub service_name: String,
    pub accepted_source_revision: String,
    pub contract_revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptedSourceRevision {
    pub repository: String,
    pub revision: String,
    pub policy_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaSpec {
    pub schema_id: String,
    pub revision: String,
    pub required_fields: Vec<String>,
    pub optional_fields: Vec<String>,
    pub field_types: BTreeMap<String, String>,
    pub readback_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorSchemaSpec {
    pub error_classes: Vec<String>,
    pub typed_status_codes: Vec<u16>,
    pub preserves_server_reason_code: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransportBinding {
    pub connector: String,
    pub connector_version: String,
    pub envelope_version: String,
    pub protocol: String,
    pub endpoint_origin_policy: String,
    pub supports_atomic_readback: bool,
    pub offline_usable_without_cloud: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthScopeContract {
    pub scheme: String,
    pub required_scopes: Vec<String>,
    pub requires_repository_binding: bool,
    pub requires_principal_delegation: bool,
    pub requires_fencing_generation: bool,
    pub forbids_client_minted_grant: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreconditionSpec {
    pub id: String,
    pub subject: String,
    pub predicate: String,
    pub recheck_at_owner: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SideEffectSpec {
    pub target: String,
    pub effect_kind: String,
    pub owner: String,
    pub mutates_durable_state: bool,
    pub requires_poststate_readback: bool,
    pub consumes_shared_capacity: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdempotencySpec {
    pub mode: String,
    pub stable_attempt_id_field: String,
    pub payload_digest_required: bool,
    pub blind_replay_forbidden: bool,
    pub reconcile_via_readback_op: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UncertainOutcomePolicy {
    pub preserves_pending: bool,
    pub preserves_uncertain: bool,
    pub preserves_stale: bool,
    pub forbids_goal_completed_on_unresolved: bool,
    pub retains_reserved_capacity_until_disproved: bool,
    pub supported_compensation_ops: Vec<String>,
    pub compensation_preserves_original_fault: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyRef {
    pub capability_id: String,
    pub component: String,
    pub contract_revision: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompatibilityConstraint {
    pub id: String,
    pub description: String,
    pub enforced_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComposedDependencies {
    pub upstream: Vec<DependencyRef>,
    pub downstream: Vec<DependencyRef>,
    pub compatibility_constraints: Vec<CompatibilityConstraint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NegativeCaseSpec {
    pub id: String,
    pub trigger: String,
    pub expected_status: ContractStatus,
    pub expected_outcome_class: PreservedOutcomeClass,
    pub forbids_blind_replay: bool,
    pub forbids_goal_completed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequiredCheckSpec {
    pub check_id: String,
    pub owner_component: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityEvidenceProvenance {
    pub producer: String,
    pub producer_version: String,
    pub observed_revision: String,
    pub check_run_id: String,
    pub observed_at: String,
    pub independent_readback_verified: bool,
    pub outcome_status: String,
    pub is_self_reported: bool,
}

/// Versioned repo-native capability contract descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityContract {
    #[serde(rename = "$schema")]
    pub schema: String,
    pub schema_version: String,
    pub capability_id: String,
    pub contract_revision: String,
    pub declared_status: ContractStatus,
    pub producer: ServiceParticipant,
    pub consumer: ServiceParticipant,
    pub owner: ServiceParticipant,
    pub accepted_source_revision: AcceptedSourceRevision,
    pub operation_name: String,
    pub input_schema: SchemaSpec,
    pub output_schema: SchemaSpec,
    pub error_schema: ErrorSchemaSpec,
    pub transport_binding: TransportBinding,
    pub auth_scope: AuthScopeContract,
    pub preconditions: Vec<PreconditionSpec>,
    pub side_effects: Vec<SideEffectSpec>,
    pub idempotency: IdempotencySpec,
    pub uncertain_outcome_behavior: UncertainOutcomePolicy,
    pub dependencies: ComposedDependencies,
    pub negative_cases: Vec<NegativeCaseSpec>,
    pub required_checks: Vec<RequiredCheckSpec>,
    pub evidence: CapabilityEvidenceProvenance,
}

impl CapabilityContract {
    /// Deterministic SHA-256 digest of the canonical serialized contract.
    pub fn canonical_digest(&self) -> String {
        let encoded = serde_json::to_string(self).unwrap_or_default();
        format!("sha256:{:x}", Sha256::digest(encoded.as_bytes()))
    }
}

/// Active interface descriptor reported for a service in a composed chain
/// (such as Dactyl transport or Propodus Cloud owner).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceInterfaceDescriptor {
    pub service_name: String,
    pub component: String,
    pub accepted_source_revision: String,
    pub contract_revision: String,
    pub operation_name: String,
    pub transport_binding: TransportBinding,
    pub auth_scope: AuthScopeContract,
    pub input_schema: SchemaSpec,
    pub output_schema: SchemaSpec,
    pub error_schema: ErrorSchemaSpec,
    pub supported_compensation_ops: Vec<String>,
    /// Even when a client observed HTTP 200-shaped success, drift or missing
    /// owner proof still fails composition.
    pub client_reported_http_ok: bool,
}

/// Accepted project/host policy governing capability contract admission.
/// Agent-edited contract copies cannot weaken these constraints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptedCapabilityPolicy {
    pub policy_digest: String,
    pub accepted_source_revision: String,
    pub pinned_contract_revision: String,
    pub pinned_contract_digest: Option<String>,
    pub required_operation_name: String,
    pub required_auth_scheme: String,
    pub required_auth_scopes: Vec<String>,
    pub required_error_classes: Vec<String>,
    pub required_negative_case_ids: Vec<String>,
    pub trusted_evidence_producers: Vec<String>,
    pub requires_independent_readback: bool,
    pub forbids_blind_replay: bool,
}

/// Independent check/readback observation supplied separately from the
/// descriptive contract copy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceObservation {
    pub producer: String,
    pub producer_version: String,
    pub observed_revision: String,
    pub contract_revision: String,
    pub operation_name: String,
    pub auth_scheme: String,
    pub auth_scopes: Vec<String>,
    pub check_run_id: String,
    pub observed_at: String,
    pub independent_readback_verified: bool,
    pub outcome_class: Option<PreservedOutcomeClass>,
    pub self_reported: bool,
    pub server_poststate_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrestateSnapshot {
    pub task_id: String,
    pub status: String,
    pub assigned_to: String,
    pub version: u64,
    pub lease_generation: u64,
    pub shared_capacity_limit: u64,
    pub reserved_capacity: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceStepRecord {
    pub step_index: u16,
    pub operation: String,
    pub succeeded: bool,
    pub uncertain_after_send: bool,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaultRecord {
    pub step: String,
    pub code: String,
    pub detail: String,
    pub outcome_class: PreservedOutcomeClass,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadbackObservation {
    pub queried_via_idempotent_op: String,
    pub owner_confirmed_committed: bool,
    pub poststate_matches_intent: bool,
    pub server_poststate_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompensationRecord {
    pub operation: String,
    pub executed: bool,
    pub final_readback_verified: bool,
    pub original_fault_retained: bool,
    pub poststate_digest: Option<String>,
}

/// Bounded execution trace exercising preparation, prestate CAS, shared
/// capacity, two-step faults, severed acks, readback, and compensation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompositeExecutionTrace {
    pub trace_id: String,
    pub attempt_id: String,
    pub prepared_against_prestate: PrestateSnapshot,
    pub current_owner_prestate: PrestateSnapshot,
    pub requested_capacity: u64,
    pub concurrent_shared_demand: u64,
    pub steps: Vec<TraceStepRecord>,
    pub async_consequence_settled: bool,
    pub severed_ack_after_possible_commit: bool,
    pub blind_replay_attempted: bool,
    pub readback_observation: Option<ReadbackObservation>,
    pub compensation: Option<CompensationRecord>,
    pub original_fault: Option<FaultRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum RepositoryCapabilityContext {
    CloudBound {
        repository: String,
        endpoint: String,
        active_profile: String,
    },
    LocalOffline {
        repository: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComposedCapabilityBundle {
    pub contract: CapabilityContract,
    pub dactyl_descriptor: Option<ServiceInterfaceDescriptor>,
    pub propodus_descriptor: Option<ServiceInterfaceDescriptor>,
    pub accepted_policy: AcceptedCapabilityPolicy,
    pub observed_evidence: Option<EvidenceObservation>,
    pub consequence_trace: Option<CompositeExecutionTrace>,
    pub repo_context: RepositoryCapabilityContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriftKind {
    SchemaValidationError,
    SelfVerificationRejected,
    AgentEditedContractCopyDrift,
    DroppedNegativeCase,
    AcceptedSourceRevisionDrift,
    ContractRevisionMismatch,
    RenamedOperation,
    AuthScopeMismatch,
    UndeclaredResponseField,
    RemovedResponseField,
    RemovedErrorClass,
    DependencyDrift,
    MissingServiceDescriptor,
    EvidenceMissing,
    EvidenceProducerUntrusted,
    EvidenceStale,
    EvidenceUncertainNotProof,
    MissingIndependentReadback,
    PrestateDriftAfterPreparation,
    SharedInvariantViolation,
    UnresolvedAsyncConsequence,
    SeveredAckBlindReplayAttempt,
    UnsupportedCompensation,
    FaultErasedByCompensation,
    UnsupportedInLocalOfflineMode,
}

impl DriftKind {
    pub fn code(self) -> &'static str {
        match self {
            Self::SchemaValidationError => "CAPABILITY_SCHEMA_INVALID",
            Self::SelfVerificationRejected => "SELF_VERIFICATION_REJECTED",
            Self::AgentEditedContractCopyDrift => "AGENT_CONTRACT_COPY_DRIFT",
            Self::DroppedNegativeCase => "REQUIRED_NEGATIVE_CASE_DROPPED",
            Self::AcceptedSourceRevisionDrift => "ACCEPTED_SOURCE_REVISION_DRIFT",
            Self::ContractRevisionMismatch => "CONTRACT_REVISION_MISMATCH",
            Self::RenamedOperation => "OPERATION_RENAMED",
            Self::AuthScopeMismatch => "AUTH_SCOPE_MISMATCH",
            Self::UndeclaredResponseField => "UNDECLARED_RESPONSE_FIELD",
            Self::RemovedResponseField => "REMOVED_RESPONSE_FIELD",
            Self::RemovedErrorClass => "REMOVED_ERROR_CLASS",
            Self::DependencyDrift => "DEPENDENCY_COMPATIBILITY_DRIFT",
            Self::MissingServiceDescriptor => "SERVICE_DESCRIPTOR_MISSING",
            Self::EvidenceMissing => "EVIDENCE_MISSING",
            Self::EvidenceProducerUntrusted => "EVIDENCE_PRODUCER_UNTRUSTED",
            Self::EvidenceStale => "EVIDENCE_STALE",
            Self::EvidenceUncertainNotProof => "UNCERTAIN_EVIDENCE_NOT_PROOF",
            Self::MissingIndependentReadback => "INDEPENDENT_READBACK_MISSING",
            Self::PrestateDriftAfterPreparation => "PRESTATE_DRIFT_AFTER_PREPARATION",
            Self::SharedInvariantViolation => "SHARED_INVARIANT_VIOLATION",
            Self::UnresolvedAsyncConsequence => "UNRESOLVED_ASYNC_CONSEQUENCE",
            Self::SeveredAckBlindReplayAttempt => "BLIND_REPLAY_FORBIDDEN",
            Self::UnsupportedCompensation => "UNSUPPORTED_COMPENSATION",
            Self::FaultErasedByCompensation => "ORIGINAL_FAULT_ERASED",
            Self::UnsupportedInLocalOfflineMode => "CLOUD_CAPABILITY_UNSUPPORTED_LOCAL_OFFLINE",
        }
    }

    fn is_stale_only(self) -> bool {
        matches!(
            self,
            Self::AcceptedSourceRevisionDrift
                | Self::EvidenceStale
                | Self::PrestateDriftAfterPreparation
                | Self::UnresolvedAsyncConsequence
        )
    }

    fn is_non_failing_advisory(self) -> bool {
        matches!(self, Self::UnsupportedInLocalOfflineMode)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriftFinding {
    pub kind: DriftKind,
    pub code: String,
    pub dimension: String,
    pub source_pointer: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckedDimension {
    pub check_id: String,
    pub dimension: String,
    pub passed: bool,
    pub source_pointer: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositeConsequenceAssessment {
    pub state: CompositeConsequenceState,
    pub eligible: bool,
    pub effect_established: bool,
    pub containment_preserved: bool,
    pub goal_completed: bool,
    pub blind_replay_rejected: bool,
    pub preserved_outcomes: Vec<PreservedOutcomeClass>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_fault_retained: Option<FaultRecord>,
    pub remaining_obligations: Vec<String>,
}

/// Bounded read-only compatibility report over composed capability contracts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatibilityReport {
    pub schema_version: String,
    pub capability_id: String,
    pub contract_revision: String,
    pub contract_digest: String,
    pub status: ContractStatus,
    pub syntactically_compatible: bool,
    pub observed_behavior_verified: bool,
    /// User-intent correctness is never inferred from schema compatibility or a
    /// single validated transaction.
    pub user_intent_correctness_established: bool,
    pub intent_correctness_boundary: String,
    pub local_offline_usable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consequence: Option<CompositeConsequenceAssessment>,
    pub checked_dimensions: Vec<CheckedDimension>,
    pub findings: Vec<DriftFinding>,
    pub source_pointers: Vec<String>,
    pub authority_note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixtureScenarioOutcome {
    pub scenario_id: String,
    pub falsifier: String,
    pub phase: String,
    pub expected_status: ContractStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_consequence_state: Option<CompositeConsequenceState>,
    pub expected_goal_completed: Option<bool>,
    pub passed: bool,
    pub report: CompatibilityReport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatibilityFixtureReport {
    pub schema_version: String,
    pub total_scenarios: usize,
    pub passed_scenarios: usize,
    pub failed_scenarios: usize,
    pub scenarios: Vec<FixtureScenarioOutcome>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityInspectionReport {
    pub schema_version: String,
    pub repository_backend: String,
    pub local_offline_usable: bool,
    pub report: CompatibilityReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixture: Option<CompatibilityFixtureReport>,
    pub authority_note: String,
}

fn push_finding(
    findings: &mut Vec<DriftFinding>,
    kind: DriftKind,
    dimension: &str,
    source_pointer: &str,
    detail: impl Into<String>,
) {
    findings.push(DriftFinding {
        kind,
        code: kind.code().to_string(),
        dimension: dimension.to_string(),
        source_pointer: source_pointer.to_string(),
        detail: detail.into(),
    });
}

fn record_check(
    checks: &mut Vec<CheckedDimension>,
    check_id: &str,
    dimension: &str,
    passed: bool,
    source_pointer: &str,
    detail: impl Into<String>,
) {
    checks.push(CheckedDimension {
        check_id: check_id.to_string(),
        dimension: dimension.to_string(),
        passed,
        source_pointer: source_pointer.to_string(),
        detail: detail.into(),
    });
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

fn check_participant(
    label: &str,
    participant: &ServiceParticipant,
    findings: &mut Vec<DriftFinding>,
) {
    if participant.component.trim().is_empty()
        || participant.service_name.trim().is_empty()
        || participant.accepted_source_revision.trim().is_empty()
        || participant.contract_revision.trim().is_empty()
    {
        push_finding(
            findings,
            DriftKind::SchemaValidationError,
            "participants",
            "src/decapod/core/capability_contract.rs::CapabilityContract",
            format!("{label} participant is missing component, service_name, or revisions"),
        );
    }
}

fn check_schema_spec(label: &str, spec: &SchemaSpec, findings: &mut Vec<DriftFinding>) {
    if spec.schema_id.trim().is_empty()
        || spec.revision.trim().is_empty()
        || (spec.required_fields.is_empty() && spec.optional_fields.is_empty())
    {
        push_finding(
            findings,
            DriftKind::SchemaValidationError,
            label,
            "assets/schemas/capability-contract.schema.json",
            format!("{label} must declare schema_id, revision, and at least one field"),
        );
    }
    for field in spec.required_fields.iter().chain(&spec.optional_fields) {
        if !spec.field_types.contains_key(field) {
            push_finding(
                findings,
                DriftKind::SchemaValidationError,
                label,
                "assets/schemas/capability-contract.schema.json",
                format!("{label} field '{field}' is missing an explicit type mapping"),
            );
        }
    }
}

/// Validate all required structural fields and self-certification invariants of a
/// [`CapabilityContract`].
pub fn validate_contract_schema(contract: &CapabilityContract) -> Vec<DriftFinding> {
    let mut findings = Vec::new();
    let ptr = "src/decapod/core/capability_contract.rs::validate_contract_schema";

    if contract.schema != CAPABILITY_CONTRACT_SCHEMA_ID {
        push_finding(
            &mut findings,
            DriftKind::SchemaValidationError,
            "schema_id",
            ptr,
            format!(
                "contract $schema '{}' does not match '{}'",
                contract.schema, CAPABILITY_CONTRACT_SCHEMA_ID
            ),
        );
    }
    if contract.schema_version != CAPABILITY_CONTRACT_SCHEMA_VERSION {
        push_finding(
            &mut findings,
            DriftKind::SchemaValidationError,
            "schema_version",
            ptr,
            format!(
                "schema_version '{}' does not match '{}'",
                contract.schema_version, CAPABILITY_CONTRACT_SCHEMA_VERSION
            ),
        );
    }
    if !valid_id(&contract.capability_id) {
        push_finding(
            &mut findings,
            DriftKind::SchemaValidationError,
            "capability_id",
            ptr,
            format!("invalid stable capability_id '{}'", contract.capability_id),
        );
    }
    if contract.contract_revision.trim().is_empty() || contract.operation_name.trim().is_empty() {
        push_finding(
            &mut findings,
            DriftKind::SchemaValidationError,
            "contract_revision",
            ptr,
            "contract_revision and operation_name must be non-empty",
        );
    }
    if contract.declared_status == ContractStatus::CurrentlyVerified
        || contract.evidence.is_self_reported
    {
        push_finding(
            &mut findings,
            DriftKind::SelfVerificationRejected,
            "self_certification",
            ptr,
            "a capability descriptor cannot mark itself currently_verified or rely on self_reported evidence",
        );
    }

    check_participant("producer", &contract.producer, &mut findings);
    check_participant("consumer", &contract.consumer, &mut findings);
    check_participant("owner", &contract.owner, &mut findings);

    if contract
        .accepted_source_revision
        .repository
        .trim()
        .is_empty()
        || contract.accepted_source_revision.revision.trim().is_empty()
        || contract
            .accepted_source_revision
            .policy_digest
            .trim()
            .is_empty()
    {
        push_finding(
            &mut findings,
            DriftKind::SchemaValidationError,
            "accepted_source_revision",
            ptr,
            "accepted_source_revision requires repository, revision, and policy_digest",
        );
    }

    check_schema_spec("input_schema", &contract.input_schema, &mut findings);
    check_schema_spec("output_schema", &contract.output_schema, &mut findings);

    if contract.error_schema.error_classes.is_empty()
        || contract.error_schema.typed_status_codes.is_empty()
        || !contract.error_schema.preserves_server_reason_code
    {
        push_finding(
            &mut findings,
            DriftKind::SchemaValidationError,
            "error_schema",
            ptr,
            "error_schema must declare error classes, typed status codes, and preserves_server_reason_code=true",
        );
    }
    if contract.transport_binding.connector.trim().is_empty()
        || contract
            .transport_binding
            .connector_version
            .trim()
            .is_empty()
        || contract
            .transport_binding
            .envelope_version
            .trim()
            .is_empty()
        || contract.transport_binding.protocol.trim().is_empty()
    {
        push_finding(
            &mut findings,
            DriftKind::SchemaValidationError,
            "transport_binding",
            ptr,
            "transport_binding fields must be non-empty",
        );
    }
    if contract.auth_scope.scheme.trim().is_empty()
        || contract.auth_scope.required_scopes.is_empty()
        || !contract.auth_scope.forbids_client_minted_grant
    {
        push_finding(
            &mut findings,
            DriftKind::SchemaValidationError,
            "auth_scope",
            ptr,
            "auth_scope must declare scheme, required_scopes, and forbids_client_minted_grant=true",
        );
    }
    if contract.preconditions.is_empty() || contract.side_effects.is_empty() {
        push_finding(
            &mut findings,
            DriftKind::SchemaValidationError,
            "preconditions_and_side_effects",
            ptr,
            "contract must explicitly declare preconditions and side_effects",
        );
    }
    if contract.idempotency.mode.trim().is_empty()
        || contract
            .idempotency
            .stable_attempt_id_field
            .trim()
            .is_empty()
        || contract
            .idempotency
            .reconcile_via_readback_op
            .trim()
            .is_empty()
    {
        push_finding(
            &mut findings,
            DriftKind::SchemaValidationError,
            "idempotency",
            ptr,
            "idempotency must declare mode, stable_attempt_id_field, and reconcile_via_readback_op",
        );
    }
    if !contract.uncertain_outcome_behavior.preserves_pending
        || !contract.uncertain_outcome_behavior.preserves_uncertain
        || !contract.uncertain_outcome_behavior.preserves_stale
        || !contract
            .uncertain_outcome_behavior
            .forbids_goal_completed_on_unresolved
        || !contract
            .uncertain_outcome_behavior
            .compensation_preserves_original_fault
    {
        push_finding(
            &mut findings,
            DriftKind::SchemaValidationError,
            "uncertain_outcome_behavior",
            ptr,
            "uncertain_outcome_behavior must preserve pending/uncertain/stale, forbid goal_completed on unresolved outcomes, and preserve original faults across compensation",
        );
    }
    if contract.dependencies.compatibility_constraints.is_empty() {
        push_finding(
            &mut findings,
            DriftKind::DependencyDrift,
            "dependencies",
            ptr,
            "dependencies must declare compatibility_constraints across composed components",
        );
    }
    if contract.negative_cases.is_empty() || contract.required_checks.is_empty() {
        push_finding(
            &mut findings,
            DriftKind::SchemaValidationError,
            "negative_cases_and_checks",
            ptr,
            "contract must declare negative_cases and required_checks",
        );
    }
    if contract.evidence.producer.trim().is_empty()
        || contract.evidence.producer_version.trim().is_empty()
    {
        push_finding(
            &mut findings,
            DriftKind::SchemaValidationError,
            "evidence_provenance",
            ptr,
            "evidence must name producer and producer_version",
        );
    }

    findings
}

fn check_against_accepted_policy(
    contract: &CapabilityContract,
    policy: &AcceptedCapabilityPolicy,
    findings: &mut Vec<DriftFinding>,
    checks: &mut Vec<CheckedDimension>,
) {
    let ptr = "src/decapod/core/capability_contract.rs::check_against_accepted_policy";
    let before = findings.len();

    if contract.accepted_source_revision.policy_digest != policy.policy_digest {
        push_finding(
            findings,
            DriftKind::AgentEditedContractCopyDrift,
            "policy_digest",
            ptr,
            format!(
                "candidate contract policy_digest '{}' does not match accepted '{}'",
                contract.accepted_source_revision.policy_digest, policy.policy_digest
            ),
        );
    }
    if let Some(pinned_digest) = &policy.pinned_contract_digest {
        let actual_digest = contract.canonical_digest();
        if &actual_digest != pinned_digest {
            push_finding(
                findings,
                DriftKind::AgentEditedContractCopyDrift,
                "pinned_contract_digest",
                ptr,
                format!(
                    "candidate contract digest '{}' differs from accepted policy digest '{}'",
                    actual_digest, pinned_digest
                ),
            );
        }
    }
    if contract.accepted_source_revision.revision != policy.accepted_source_revision {
        push_finding(
            findings,
            DriftKind::AcceptedSourceRevisionDrift,
            "accepted_source_revision",
            ptr,
            format!(
                "contract accepted_source_revision '{}' != accepted '{}'",
                contract.accepted_source_revision.revision, policy.accepted_source_revision
            ),
        );
    }
    if contract.contract_revision != policy.pinned_contract_revision {
        push_finding(
            findings,
            DriftKind::ContractRevisionMismatch,
            "contract_revision",
            ptr,
            format!(
                "contract_revision '{}' != accepted pinned '{}'",
                contract.contract_revision, policy.pinned_contract_revision
            ),
        );
    }
    if contract.operation_name != policy.required_operation_name {
        push_finding(
            findings,
            DriftKind::RenamedOperation,
            "operation_name",
            ptr,
            format!(
                "operation '{}' != accepted '{}'",
                contract.operation_name, policy.required_operation_name
            ),
        );
    }
    if contract.auth_scope.scheme != policy.required_auth_scheme
        || !policy
            .required_auth_scopes
            .iter()
            .all(|s| contract.auth_scope.required_scopes.contains(s))
    {
        push_finding(
            findings,
            DriftKind::AuthScopeMismatch,
            "auth_scope",
            ptr,
            "candidate contract auth scheme or scopes weakened relative to accepted policy",
        );
    }
    if policy.forbids_blind_replay && !contract.idempotency.blind_replay_forbidden {
        push_finding(
            findings,
            DriftKind::AgentEditedContractCopyDrift,
            "idempotency",
            ptr,
            "candidate contract weakened blind_replay_forbidden below accepted policy",
        );
    }
    if policy.requires_independent_readback && !contract.output_schema.readback_required {
        push_finding(
            findings,
            DriftKind::AgentEditedContractCopyDrift,
            "readback_required",
            ptr,
            "candidate contract disabled output_schema.readback_required",
        );
    }
    for required_err in &policy.required_error_classes {
        if !contract.error_schema.error_classes.contains(required_err) {
            push_finding(
                findings,
                DriftKind::RemovedErrorClass,
                "error_schema",
                ptr,
                format!("candidate contract dropped accepted error class '{required_err}'"),
            );
        }
    }
    let present_cases: BTreeSet<&str> = contract
        .negative_cases
        .iter()
        .map(|c| c.id.as_str())
        .collect();
    for required_case in &policy.required_negative_case_ids {
        if !present_cases.contains(required_case.as_str()) {
            push_finding(
                findings,
                DriftKind::DroppedNegativeCase,
                "negative_cases",
                ptr,
                format!(
                    "candidate contract dropped required negative case '{required_case}' mandated by accepted policy"
                ),
            );
        }
    }

    record_check(
        checks,
        "policy.accepted_baseline",
        "accepted_policy",
        findings.len() == before,
        ptr,
        if findings.len() == before {
            "candidate contract preserves accepted policy, scopes, error classes, and negative cases"
        } else {
            "candidate contract drifted from accepted project/host policy"
        },
    );
}

fn compare_schemas(
    consumer_label: &str,
    consumer: &SchemaSpec,
    producer_label: &str,
    producer: &SchemaSpec,
    client_reported_http_ok: bool,
    findings: &mut Vec<DriftFinding>,
) {
    let ptr = "src/decapod/core/capability_contract.rs::compare_schemas";
    let producer_fields: BTreeSet<&str> = producer
        .required_fields
        .iter()
        .chain(&producer.optional_fields)
        .map(String::as_str)
        .collect();
    let consumer_fields: BTreeSet<&str> = consumer
        .required_fields
        .iter()
        .chain(&consumer.optional_fields)
        .map(String::as_str)
        .collect();

    for field in consumer
        .required_fields
        .iter()
        .chain(&consumer.optional_fields)
    {
        if !producer_fields.contains(field.as_str()) {
            push_finding(
                findings,
                DriftKind::UndeclaredResponseField,
                "schema_fields",
                ptr,
                format!(
                    "{consumer_label} expects field '{field}' that {producer_label} no longer implements (client_http_ok={client_reported_http_ok} cannot clear gap)"
                ),
            );
        } else if consumer.field_types.get(field) != producer.field_types.get(field) {
            push_finding(
                findings,
                DriftKind::RemovedResponseField,
                "schema_field_types",
                ptr,
                format!(
                    "field '{field}' type mismatch between {consumer_label} ({:?}) and {producer_label} ({:?})",
                    consumer.field_types.get(field),
                    producer.field_types.get(field)
                ),
            );
        }
    }

    for field in &producer.required_fields {
        if !consumer_fields.contains(field.as_str()) {
            push_finding(
                findings,
                DriftKind::RemovedResponseField,
                "schema_fields",
                ptr,
                format!("{producer_label} requires field '{field}' omitted by {consumer_label}"),
            );
        }
    }
}

fn check_service_interface_pair(
    contract: &CapabilityContract,
    role_label: &str,
    descriptor: &ServiceInterfaceDescriptor,
    findings: &mut Vec<DriftFinding>,
    checks: &mut Vec<CheckedDimension>,
) {
    let ptr = "src/decapod/core/capability_contract.rs::check_service_interface_pair";
    let before = findings.len();

    if descriptor.contract_revision != contract.contract_revision {
        push_finding(
            findings,
            DriftKind::ContractRevisionMismatch,
            "service_contract_revision",
            ptr,
            format!(
                "{role_label} contract_revision '{}' != Decapod contract_revision '{}'",
                descriptor.contract_revision, contract.contract_revision
            ),
        );
    }

    let expected_rev = if role_label == "dactyl" {
        &contract.producer.accepted_source_revision
    } else {
        &contract.owner.accepted_source_revision
    };
    if &descriptor.accepted_source_revision != expected_rev {
        push_finding(
            findings,
            DriftKind::AcceptedSourceRevisionDrift,
            "service_source_revision",
            ptr,
            format!(
                "{role_label} accepted_source_revision '{}' != pinned '{}'",
                descriptor.accepted_source_revision, expected_rev
            ),
        );
    }

    if descriptor.operation_name != contract.operation_name {
        push_finding(
            findings,
            DriftKind::RenamedOperation,
            "service_operation_name",
            ptr,
            format!(
                "{role_label} renamed operation to '{}' (expected '{}')",
                descriptor.operation_name, contract.operation_name
            ),
        );
    }

    let consumer_scopes: BTreeSet<&str> = contract
        .auth_scope
        .required_scopes
        .iter()
        .map(String::as_str)
        .collect();
    let service_scopes: BTreeSet<&str> = descriptor
        .auth_scope
        .required_scopes
        .iter()
        .map(String::as_str)
        .collect();
    if descriptor.auth_scope.scheme != contract.auth_scope.scheme
        || consumer_scopes != service_scopes
        || descriptor.auth_scope.requires_repository_binding
            != contract.auth_scope.requires_repository_binding
        || descriptor.auth_scope.requires_fencing_generation
            != contract.auth_scope.requires_fencing_generation
    {
        push_finding(
            findings,
            DriftKind::AuthScopeMismatch,
            "service_auth_scope",
            ptr,
            format!(
                "{role_label} auth scope ({}, {:?}) drifted from contract ({}, {:?})",
                descriptor.auth_scope.scheme,
                service_scopes,
                contract.auth_scope.scheme,
                consumer_scopes
            ),
        );
    }

    if descriptor.transport_binding.envelope_version != contract.transport_binding.envelope_version
        || descriptor.transport_binding.supports_atomic_readback
            != contract.transport_binding.supports_atomic_readback
    {
        push_finding(
            findings,
            DriftKind::DependencyDrift,
            "transport_binding",
            ptr,
            format!(
                "{role_label} transport binding envelope '{}' / atomic_readback={} incompatible with contract '{}' / {}",
                descriptor.transport_binding.envelope_version,
                descriptor.transport_binding.supports_atomic_readback,
                contract.transport_binding.envelope_version,
                contract.transport_binding.supports_atomic_readback
            ),
        );
    }

    compare_schemas(
        "contract.input_schema",
        &contract.input_schema,
        role_label,
        &descriptor.input_schema,
        descriptor.client_reported_http_ok,
        findings,
    );
    compare_schemas(
        "contract.output_schema",
        &contract.output_schema,
        role_label,
        &descriptor.output_schema,
        descriptor.client_reported_http_ok,
        findings,
    );

    let service_errors: BTreeSet<&str> = descriptor
        .error_schema
        .error_classes
        .iter()
        .map(String::as_str)
        .collect();
    for err_class in &contract.error_schema.error_classes {
        if !service_errors.contains(err_class.as_str()) {
            push_finding(
                findings,
                DriftKind::RemovedErrorClass,
                "service_error_schema",
                ptr,
                format!(
                    "{role_label} no longer implements declared error class '{err_class}' (client_http_ok={} ignored)",
                    descriptor.client_reported_http_ok
                ),
            );
        }
    }
    let service_codes: BTreeSet<u16> = descriptor
        .error_schema
        .typed_status_codes
        .iter()
        .copied()
        .collect();
    for code in &contract.error_schema.typed_status_codes {
        if !service_codes.contains(code) {
            push_finding(
                findings,
                DriftKind::RemovedErrorClass,
                "service_error_codes",
                ptr,
                format!("{role_label} dropped typed HTTP/status code {code}"),
            );
        }
    }

    record_check(
        checks,
        &format!("compose.{role_label}"),
        &format!("{role_label}_interface"),
        findings.len() == before,
        ptr,
        if findings.len() == before {
            format!("{role_label} interface aligns with pinned Decapod contract")
        } else {
            format!("{role_label} interface drifted from pinned Decapod contract")
        },
    );
}

fn evaluate_evidence(
    contract: &CapabilityContract,
    policy: &AcceptedCapabilityPolicy,
    evidence: Option<&EvidenceObservation>,
    findings: &mut Vec<DriftFinding>,
    checks: &mut Vec<CheckedDimension>,
) -> bool {
    let ptr = "src/decapod/core/capability_contract.rs::evaluate_evidence";
    let Some(obs) = evidence else {
        push_finding(
            findings,
            DriftKind::EvidenceMissing,
            "evidence",
            ptr,
            "no independent execution/readback evidence provided; contract stays declared",
        );
        record_check(
            checks,
            "evidence.independent_observation",
            "evidence",
            false,
            ptr,
            "missing independent execution observation",
        );
        return false;
    };

    let before = findings.len();
    if obs.self_reported {
        push_finding(
            findings,
            DriftKind::SelfVerificationRejected,
            "evidence",
            ptr,
            "self-reported observation cannot establish verified capability status",
        );
    }
    if obs.producer.trim().is_empty()
        || obs.producer_version.trim().is_empty()
        || !policy.trusted_evidence_producers.contains(&obs.producer)
    {
        push_finding(
            findings,
            DriftKind::EvidenceProducerUntrusted,
            "evidence_producer",
            ptr,
            format!(
                "evidence producer '{}' v'{}' is not in accepted trusted_evidence_producers",
                obs.producer, obs.producer_version
            ),
        );
    }
    if obs.observed_revision != contract.accepted_source_revision.revision
        || obs.contract_revision != contract.contract_revision
        || obs.operation_name != contract.operation_name
        || obs.auth_scheme != contract.auth_scope.scheme
    {
        push_finding(
            findings,
            DriftKind::EvidenceStale,
            "evidence_freshness",
            ptr,
            format!(
                "prior evidence (rev='{}', contract='{}', op='{}', auth='{}') does not match current target (rev='{}', contract='{}', op='{}', auth='{}')",
                obs.observed_revision,
                obs.contract_revision,
                obs.operation_name,
                obs.auth_scheme,
                contract.accepted_source_revision.revision,
                contract.contract_revision,
                contract.operation_name,
                contract.auth_scope.scheme,
            ),
        );
    }
    if let Some(outcome) = obs.outcome_class
        && outcome != PreservedOutcomeClass::Pending
    {
        push_finding(
            findings,
            DriftKind::EvidenceUncertainNotProof,
            "evidence_outcome",
            ptr,
            format!(
                "prior evidence recorded outcome '{outcome:?}', which cannot certify current success"
            ),
        );
    } else if obs.outcome_class == Some(PreservedOutcomeClass::Pending) {
        push_finding(
            findings,
            DriftKind::UnresolvedAsyncConsequence,
            "evidence_outcome",
            ptr,
            "evidence recorded pending outcome; unresolved async work cannot prove completion",
        );
    }
    if contract.output_schema.readback_required && !obs.independent_readback_verified {
        push_finding(
            findings,
            DriftKind::MissingIndependentReadback,
            "independent_readback",
            ptr,
            "independent service-side readback did not verify poststate",
        );
    }

    let ok = findings.len() == before;
    record_check(
        checks,
        "evidence.independent_observation",
        "evidence",
        ok,
        ptr,
        if ok {
            format!(
                "independent readback verified by {} {}",
                obs.producer, obs.producer_version
            )
        } else {
            "evidence failed provenance, freshness, or readback verification".to_string()
        },
    );
    ok
}

fn evaluate_consequence_trace(
    contract: &CapabilityContract,
    propodus_descriptor: Option<&ServiceInterfaceDescriptor>,
    trace: &CompositeExecutionTrace,
    findings: &mut Vec<DriftFinding>,
    checks: &mut Vec<CheckedDimension>,
) -> CompositeConsequenceAssessment {
    let ptr = "src/decapod/core/capability_contract.rs::evaluate_consequence_trace";
    let mut preserved = BTreeSet::new();
    let mut remaining_obligations = Vec::new();
    let mut blind_replay_rejected = false;

    if trace.blind_replay_attempted && contract.idempotency.blind_replay_forbidden {
        blind_replay_rejected = true;
        preserved.insert(PreservedOutcomeClass::Uncertain);
        remaining_obligations.push(format!(
            "reconcile attempt '{}' via '{}' before retrying",
            trace.attempt_id, contract.idempotency.reconcile_via_readback_op
        ));
        push_finding(
            findings,
            DriftKind::SeveredAckBlindReplayAttempt,
            "idempotency",
            ptr,
            "blind replay of mutating operation after uncertain/severed outcome is forbidden",
        );
    }

    let prestate_drifted = trace.prepared_against_prestate.version
        != trace.current_owner_prestate.version
        || trace.prepared_against_prestate.lease_generation
            != trace.current_owner_prestate.lease_generation
        || trace.prepared_against_prestate.status != trace.current_owner_prestate.status
        || trace.prepared_against_prestate.assigned_to != trace.current_owner_prestate.assigned_to;

    if prestate_drifted {
        preserved.insert(PreservedOutcomeClass::Stale);
        remaining_obligations
            .push("reprepare operation against current owner prestate".to_string());
        push_finding(
            findings,
            DriftKind::PrestateDriftAfterPreparation,
            "prestate_applicability",
            ptr,
            format!(
                "owner prestate drifted after preparation (prepared v{}/gen{}, current v{}/gen{})",
                trace.prepared_against_prestate.version,
                trace.prepared_against_prestate.lease_generation,
                trace.current_owner_prestate.version,
                trace.current_owner_prestate.lease_generation,
            ),
        );
    }

    let total_demand = trace
        .current_owner_prestate
        .reserved_capacity
        .saturating_add(trace.requested_capacity)
        .saturating_add(trace.concurrent_shared_demand);
    let shared_invariant_violated =
        total_demand > trace.current_owner_prestate.shared_capacity_limit;
    if shared_invariant_violated {
        preserved.insert(PreservedOutcomeClass::Failed);
        remaining_obligations
            .push("release or fit within shared owner capacity limit before admission".to_string());
        push_finding(
            findings,
            DriftKind::SharedInvariantViolation,
            "shared_invariant",
            ptr,
            format!(
                "aggregate shared demand ({total_demand}) exceeds owner limit ({})",
                trace.current_owner_prestate.shared_capacity_limit
            ),
        );
    }

    if !trace.async_consequence_settled && !trace.steps.is_empty() {
        preserved.insert(PreservedOutcomeClass::Pending);
        remaining_obligations.push(
            "wait for acknowledged asynchronous consequence to settle and verify readback"
                .to_string(),
        );
        push_finding(
            findings,
            DriftKind::UnresolvedAsyncConsequence,
            "async_consequence",
            ptr,
            "response arrived before acknowledged asynchronous consequence settled; preserving pending",
        );
    }

    let has_uncertain_step = trace.severed_ack_after_possible_commit
        || trace.steps.iter().any(|step| step.uncertain_after_send);
    let readback_confirmed = trace.readback_observation.as_ref().is_some_and(|rb| {
        rb.queried_via_idempotent_op == contract.idempotency.reconcile_via_readback_op
            && rb.owner_confirmed_committed
            && rb.poststate_matches_intent
    });
    if has_uncertain_step && !readback_confirmed && trace.compensation.is_none() {
        preserved.insert(PreservedOutcomeClass::Uncertain);
        remaining_obligations.push(format!(
            "query '{}' for authoritative poststate truth",
            contract.idempotency.reconcile_via_readback_op
        ));
        push_finding(
            findings,
            DriftKind::EvidenceUncertainNotProof,
            "severed_acknowledgement",
            ptr,
            "severed acknowledgement after possible commit leaves outcome uncertain; containment holds reserved capacity without claiming completion",
        );
    }

    let mut supported_comp_ops: BTreeSet<&str> = contract
        .uncertain_outcome_behavior
        .supported_compensation_ops
        .iter()
        .map(String::as_str)
        .collect();
    if let Some(propodus) = propodus_descriptor {
        let prop_ops: BTreeSet<&str> = propodus
            .supported_compensation_ops
            .iter()
            .map(String::as_str)
            .collect();
        supported_comp_ops = supported_comp_ops
            .intersection(&prop_ops)
            .copied()
            .collect();
    }

    if let Some(comp) = &trace.compensation {
        let mut comp_admissible = true;
        if !supported_comp_ops.contains(comp.operation.as_str()) {
            comp_admissible = false;
            push_finding(
                findings,
                DriftKind::UnsupportedCompensation,
                "compensation",
                ptr,
                format!(
                    "compensation operation '{}' is not mutually supported by contract and active service",
                    comp.operation
                ),
            );
        }
        if has_uncertain_step && trace.readback_observation.is_none() {
            comp_admissible = false;
            preserved.insert(PreservedOutcomeClass::Uncertain);
            push_finding(
                findings,
                DriftKind::UnsupportedCompensation,
                "compensation",
                ptr,
                "cannot run compensation before authoritative readback resolves uncertain commit state",
            );
        }
        if !comp.original_fault_retained || trace.original_fault.is_none() {
            comp_admissible = false;
            push_finding(
                findings,
                DriftKind::FaultErasedByCompensation,
                "compensation_evidence",
                ptr,
                "compensation must retain original fault in historical evidence; compensation is not erasure",
            );
        }
        if comp_admissible && comp.executed && comp.final_readback_verified {
            record_check(
                checks,
                "consequence.closure",
                "composite_consequence",
                true,
                ptr,
                format!(
                    "compensated via '{}' with final readback verified and original fault retained",
                    comp.operation
                ),
            );
            return CompositeConsequenceAssessment {
                state: CompositeConsequenceState::Compensated,
                eligible: false,
                effect_established: true,
                containment_preserved: true,
                goal_completed: false,
                blind_replay_rejected,
                preserved_outcomes: preserved.into_iter().collect(),
                original_fault_retained: trace.original_fault.clone(),
                remaining_obligations,
            };
        }
        // Inadmissible or unverified compensation falls back to Held containment.
        preserved.insert(PreservedOutcomeClass::Failed);
        record_check(
            checks,
            "consequence.closure",
            "composite_consequence",
            false,
            ptr,
            "compensation refused or unverified; holding protected state with goal_completed=false",
        );
        return CompositeConsequenceAssessment {
            state: CompositeConsequenceState::Held,
            eligible: false,
            effect_established: false,
            containment_preserved: true,
            goal_completed: false,
            blind_replay_rejected,
            preserved_outcomes: preserved.into_iter().collect(),
            original_fault_retained: trace.original_fault.clone(),
            remaining_obligations,
        };
    }

    let step_failed = trace.steps.iter().any(|s| !s.succeeded);
    if step_failed {
        preserved.insert(PreservedOutcomeClass::Failed);
        remaining_obligations.push(
            "resolve failed intermediate step via supported compensation or fresh attempt"
                .to_string(),
        );
    }

    if prestate_drifted
        || shared_invariant_violated
        || (!trace.async_consequence_settled && !trace.steps.is_empty())
        || (has_uncertain_step && !readback_confirmed)
        || blind_replay_rejected
        || step_failed
    {
        record_check(
            checks,
            "consequence.closure",
            "composite_consequence",
            false,
            ptr,
            "containment held (goal_completed=false) due to unresolved drift, shared invariant, pending async, severed ack, or failed step",
        );
        return CompositeConsequenceAssessment {
            state: CompositeConsequenceState::Held,
            eligible: false,
            effect_established: trace.steps.iter().any(|s| s.succeeded),
            containment_preserved: true,
            goal_completed: false,
            blind_replay_rejected,
            preserved_outcomes: preserved.into_iter().collect(),
            original_fault_retained: trace.original_fault.clone(),
            remaining_obligations,
        };
    }

    if trace.steps.is_empty() {
        record_check(
            checks,
            "consequence.closure",
            "composite_consequence",
            true,
            ptr,
            "prepared/allowed: authorization and prestate matched, no effect established yet",
        );
        return CompositeConsequenceAssessment {
            state: CompositeConsequenceState::Prepared,
            eligible: true,
            effect_established: false,
            containment_preserved: true,
            goal_completed: false,
            blind_replay_rejected: false,
            preserved_outcomes: Vec::new(),
            original_fault_retained: None,
            remaining_obligations: vec!["execute mutation and confirm owner readback".to_string()],
        };
    }

    if !readback_confirmed {
        preserved.insert(PreservedOutcomeClass::Uncertain);
        push_finding(
            findings,
            DriftKind::MissingIndependentReadback,
            "independent_readback",
            ptr,
            "mutation steps executed without authoritative owner poststate readback",
        );
        record_check(
            checks,
            "consequence.closure",
            "composite_consequence",
            false,
            ptr,
            "missing independent readback; held with goal_completed=false",
        );
        return CompositeConsequenceAssessment {
            state: CompositeConsequenceState::Held,
            eligible: false,
            effect_established: true,
            containment_preserved: true,
            goal_completed: false,
            blind_replay_rejected: false,
            preserved_outcomes: preserved.into_iter().collect(),
            original_fault_retained: trace.original_fault.clone(),
            remaining_obligations: vec!["run independent readback at effect owner".to_string()],
        };
    }

    record_check(
        checks,
        "consequence.closure",
        "composite_consequence",
        true,
        ptr,
        "observed/settled: actual owner poststate and independent readback confirmed",
    );
    CompositeConsequenceAssessment {
        state: CompositeConsequenceState::Observed,
        eligible: true,
        effect_established: true,
        containment_preserved: true,
        goal_completed: true,
        blind_replay_rejected: false,
        preserved_outcomes: Vec::new(),
        original_fault_retained: None,
        remaining_obligations: Vec::new(),
    }
}

/// Deterministically evaluate a composed capability bundle (`Decapod -> Dactyl -> Propodus`)
/// without network I/O or hidden state mutation.
pub fn evaluate_composed_capability(bundle: &ComposedCapabilityBundle) -> CompatibilityReport {
    let mut findings = validate_contract_schema(&bundle.contract);
    let mut checks = Vec::new();
    let ptr_schema = "src/decapod/core/capability_contract.rs::validate_contract_schema";
    let schema_ok = findings.is_empty();
    record_check(
        &mut checks,
        "contract.schema",
        "schema",
        schema_ok,
        ptr_schema,
        if schema_ok {
            "contract satisfies decapod.capability-contract.v1 required fields"
        } else {
            "contract failed schema or self-certification checks"
        },
    );

    check_against_accepted_policy(
        &bundle.contract,
        &bundle.accepted_policy,
        &mut findings,
        &mut checks,
    );

    let mut source_pointers = vec![
        "src/decapod/core/capability_contract.rs".to_string(),
        "assets/schemas/capability-contract.schema.json".to_string(),
        bundle.contract.consumer.component.clone(),
        bundle.contract.producer.component.clone(),
        bundle.contract.owner.component.clone(),
    ];

    let mut consequence = None;
    let mut evidence_verified = false;
    let local_offline_usable;

    match &bundle.repo_context {
        RepositoryCapabilityContext::LocalOffline { .. } => {
            local_offline_usable = true;
            if !bundle
                .contract
                .transport_binding
                .offline_usable_without_cloud
            {
                push_finding(
                    &mut findings,
                    DriftKind::UnsupportedInLocalOfflineMode,
                    "repository_binding",
                    "src/decapod/core/backend.rs::BackendRoute",
                    "repository has no Cloud binding; Cloud capability is unsupported while local offline workflows remain usable",
                );
                record_check(
                    &mut checks,
                    "repo.offline_isolation",
                    "offline_usability",
                    true,
                    "src/decapod/core/backend.rs::BackendRoute",
                    "local offline repository remains usable without Cloud dependency",
                );
            } else {
                evidence_verified = evaluate_evidence(
                    &bundle.contract,
                    &bundle.accepted_policy,
                    bundle.observed_evidence.as_ref(),
                    &mut findings,
                    &mut checks,
                );
            }
        }
        RepositoryCapabilityContext::CloudBound { .. } => {
            local_offline_usable = true;
            if let Some(dactyl) = &bundle.dactyl_descriptor {
                check_service_interface_pair(
                    &bundle.contract,
                    "dactyl",
                    dactyl,
                    &mut findings,
                    &mut checks,
                );
                source_pointers.push(dactyl.component.clone());
            } else {
                push_finding(
                    &mut findings,
                    DriftKind::MissingServiceDescriptor,
                    "dactyl_interface",
                    "src/decapod/core/dactyl_todo.rs",
                    "missing Dactyl transport descriptor for Cloud-bound composition check",
                );
            }
            if let Some(propodus) = &bundle.propodus_descriptor {
                check_service_interface_pair(
                    &bundle.contract,
                    "propodus",
                    propodus,
                    &mut findings,
                    &mut checks,
                );
                source_pointers.push(propodus.component.clone());
            } else {
                push_finding(
                    &mut findings,
                    DriftKind::MissingServiceDescriptor,
                    "propodus_interface",
                    "src/decapod/core/propodus.rs",
                    "missing Propodus owner descriptor for Cloud-bound composition check",
                );
            }

            evidence_verified = evaluate_evidence(
                &bundle.contract,
                &bundle.accepted_policy,
                bundle.observed_evidence.as_ref(),
                &mut findings,
                &mut checks,
            );

            if let Some(trace) = &bundle.consequence_trace {
                consequence = Some(evaluate_consequence_trace(
                    &bundle.contract,
                    bundle.propodus_descriptor.as_ref(),
                    trace,
                    &mut findings,
                    &mut checks,
                ));
            }
        }
    }

    source_pointers.sort();
    source_pointers.dedup();

    let syntactically_compatible = !findings.iter().any(|f| {
        matches!(
            f.kind,
            DriftKind::SchemaValidationError
                | DriftKind::SelfVerificationRejected
                | DriftKind::AgentEditedContractCopyDrift
                | DriftKind::DroppedNegativeCase
                | DriftKind::ContractRevisionMismatch
                | DriftKind::RenamedOperation
                | DriftKind::AuthScopeMismatch
                | DriftKind::UndeclaredResponseField
                | DriftKind::RemovedResponseField
                | DriftKind::RemovedErrorClass
                | DriftKind::DependencyDrift
                | DriftKind::MissingServiceDescriptor
        )
    });

    let has_incompatible = findings.iter().any(|f| {
        !f.kind.is_non_failing_advisory()
            && !f.kind.is_stale_only()
            && !matches!(f.kind, DriftKind::EvidenceMissing)
    });
    let has_stale = findings.iter().any(|f| f.kind.is_stale_only());
    let is_local_unsupported = findings
        .iter()
        .any(|f| matches!(f.kind, DriftKind::UnsupportedInLocalOfflineMode));

    let status = if is_local_unsupported && !has_incompatible {
        ContractStatus::Unsupported
    } else if has_incompatible {
        ContractStatus::Incompatible
    } else if has_stale {
        ContractStatus::Stale
    } else if !evidence_verified {
        if consequence
            .as_ref()
            .is_some_and(|c| c.state == CompositeConsequenceState::Observed)
        {
            ContractStatus::Observed
        } else {
            ContractStatus::Declared
        }
    } else {
        match consequence.as_ref().map(|c| c.state) {
            None | Some(CompositeConsequenceState::Observed) => ContractStatus::CurrentlyVerified,
            Some(CompositeConsequenceState::Prepared)
            | Some(CompositeConsequenceState::Compensated) => ContractStatus::Observed,
            Some(CompositeConsequenceState::Held) => ContractStatus::Stale,
        }
    };

    let observed_behavior_verified = matches!(
        status,
        ContractStatus::CurrentlyVerified | ContractStatus::Observed
    ) && evidence_verified;

    CompatibilityReport {
        schema_version: COMPATIBILITY_REPORT_SCHEMA_VERSION.to_string(),
        capability_id: bundle.contract.capability_id.clone(),
        contract_revision: bundle.contract.contract_revision.clone(),
        contract_digest: bundle.contract.canonical_digest(),
        status,
        syntactically_compatible,
        observed_behavior_verified,
        user_intent_correctness_established: false,
        intent_correctness_boundary: "Syntactic interface compatibility and observed single-transaction behavior are distinct; neither establishes user-intent correctness or universal multi-system atomicity.".to_string(),
        local_offline_usable,
        consequence,
        checked_dimensions: checks,
        findings,
        source_pointers,
        authority_note: "Descriptive contract compatibility report only; declarations and fixtures are admissibility inputs, never self-issued authorization or publication proof.".to_string(),
    }
}

/// Canonical Cloud task mutation + readback contract (`Decapod -> Dactyl -> Propodus`).
pub fn canonical_cloud_task_contract() -> CapabilityContract {
    let mut input_types = BTreeMap::new();
    input_types.insert("repo_id".to_string(), "string".to_string());
    input_types.insert("task_id".to_string(), "string".to_string());
    input_types.insert("actor".to_string(), "string".to_string());
    input_types.insert("lease_ttl_seconds".to_string(), "u64".to_string());
    input_types.insert("attempt_id".to_string(), "string".to_string());
    input_types.insert("expected_version".to_string(), "i32".to_string());

    let mut output_types = BTreeMap::new();
    output_types.insert("id".to_string(), "string".to_string());
    output_types.insert("repo_id".to_string(), "string".to_string());
    output_types.insert("status".to_string(), "string".to_string());
    output_types.insert("assigned_to".to_string(), "string".to_string());
    output_types.insert("version".to_string(), "i32".to_string());
    output_types.insert("event_seq".to_string(), "i64".to_string());
    output_types.insert("readback_digest".to_string(), "sha256".to_string());
    output_types.insert("effect_status".to_string(), "enum".to_string());

    CapabilityContract {
        schema: CAPABILITY_CONTRACT_SCHEMA_ID.to_string(),
        schema_version: CAPABILITY_CONTRACT_SCHEMA_VERSION.to_string(),
        capability_id: CLOUD_TASK_CLAIM_CAPABILITY_ID.to_string(),
        contract_revision: "2026-11-21.v1".to_string(),
        declared_status: ContractStatus::Declared,
        consumer: ServiceParticipant {
            component: "src/decapod/core/dactyl_todo.rs::DactylTodoStore".to_string(),
            service_name: "decapod".to_string(),
            accepted_source_revision: "decapod@efc05d792".to_string(),
            contract_revision: "2026-11-21.v1".to_string(),
        },
        producer: ServiceParticipant {
            component: "dactyl_db::CloudConnector".to_string(),
            service_name: "dactyl".to_string(),
            accepted_source_revision: "dactyl@0.18.0".to_string(),
            contract_revision: "2026-11-21.v1".to_string(),
        },
        owner: ServiceParticipant {
            component: "propodus::dactyl_api::task_claim_v1".to_string(),
            service_name: "propodus".to_string(),
            accepted_source_revision: "propodus@9abd49e".to_string(),
            contract_revision: "2026-11-21.v1".to_string(),
        },
        accepted_source_revision: AcceptedSourceRevision {
            repository: "DecapodLabs/decapod".to_string(),
            revision: "decapod@efc05d792".to_string(),
            policy_digest: "sha256:accepted-policy-cloud-task-v1".to_string(),
        },
        operation_name: "task.claim".to_string(),
        input_schema: SchemaSpec {
            schema_id: "decapod.cloud.task_claim.request.v1".to_string(),
            revision: "1".to_string(),
            required_fields: vec![
                "repo_id".to_string(),
                "task_id".to_string(),
                "actor".to_string(),
                "attempt_id".to_string(),
                "expected_version".to_string(),
            ],
            optional_fields: vec!["lease_ttl_seconds".to_string()],
            field_types: input_types,
            readback_required: false,
        },
        output_schema: SchemaSpec {
            schema_id: "decapod.cloud.task_claim.response.v1".to_string(),
            revision: "1".to_string(),
            required_fields: vec![
                "id".to_string(),
                "repo_id".to_string(),
                "status".to_string(),
                "assigned_to".to_string(),
                "version".to_string(),
                "event_seq".to_string(),
                "readback_digest".to_string(),
            ],
            optional_fields: vec!["effect_status".to_string()],
            field_types: output_types,
            readback_required: true,
        },
        error_schema: ErrorSchemaSpec {
            error_classes: vec![
                "authentication_required".to_string(),
                "authorization_denied".to_string(),
                "prestate_conflict".to_string(),
                "quota_exceeded".to_string(),
                "outcome_unknown".to_string(),
            ],
            typed_status_codes: vec![401, 403, 409, 413, 429],
            preserves_server_reason_code: true,
        },
        transport_binding: TransportBinding {
            connector: "dactyl_db::CloudConnector".to_string(),
            connector_version: "0.18.0".to_string(),
            envelope_version: "storage_context.v1+cloud_response_metadata.v1".to_string(),
            protocol: "https+atomic_rpc_batch".to_string(),
            endpoint_origin_policy: "authenticated_tls_origin_bound".to_string(),
            supports_atomic_readback: true,
            offline_usable_without_cloud: false,
        },
        auth_scope: AuthScopeContract {
            scheme: "DPoP+StorageContext.v1".to_string(),
            required_scopes: vec![
                "repo:task:claim".to_string(),
                "repo:task:readback".to_string(),
            ],
            requires_repository_binding: true,
            requires_principal_delegation: true,
            requires_fencing_generation: true,
            forbids_client_minted_grant: true,
        },
        preconditions: vec![
            PreconditionSpec {
                id: "precondition.open_unassigned".to_string(),
                subject: "tasks".to_string(),
                predicate: "status IN ('open','pending') AND assigned_to='' AND version=expected_version".to_string(),
                recheck_at_owner: true,
            },
            PreconditionSpec {
                id: "precondition.shared_capacity".to_string(),
                subject: "org_repo_lease_capacity".to_string(),
                predicate: "reserved_capacity + requested <= shared_capacity_limit".to_string(),
                recheck_at_owner: true,
            },
        ],
        side_effects: vec![
            SideEffectSpec {
                target: "tasks".to_string(),
                effect_kind: "claim_lease_and_increment_version".to_string(),
                owner: "propodus".to_string(),
                mutates_durable_state: true,
                requires_poststate_readback: true,
                consumes_shared_capacity: true,
            },
            SideEffectSpec {
                target: "events".to_string(),
                effect_kind: "append_task_claim_event".to_string(),
                owner: "propodus".to_string(),
                mutates_durable_state: true,
                requires_poststate_readback: true,
                consumes_shared_capacity: false,
            },
        ],
        idempotency: IdempotencySpec {
            mode: "fencing_cas_and_operation_digest".to_string(),
            stable_attempt_id_field: "attempt_id".to_string(),
            payload_digest_required: true,
            blind_replay_forbidden: true,
            reconcile_via_readback_op: "propodus_proof_acceptance_get_v1".to_string(),
        },
        uncertain_outcome_behavior: UncertainOutcomePolicy {
            preserves_pending: true,
            preserves_uncertain: true,
            preserves_stale: true,
            forbids_goal_completed_on_unresolved: true,
            retains_reserved_capacity_until_disproved: true,
            supported_compensation_ops: vec!["task.release".to_string()],
            compensation_preserves_original_fault: true,
        },
        dependencies: ComposedDependencies {
            upstream: vec![DependencyRef {
                capability_id: "decapod.effective_operation_grant.v1".to_string(),
                component: "src/decapod/core/cloud_proof.rs".to_string(),
                contract_revision: "2026-11-21.v1".to_string(),
                required: true,
            }],
            downstream: vec![DependencyRef {
                capability_id: "propodus.task_claim_tx.v1".to_string(),
                component: "propodus::dactyl_api::task_claim_v1".to_string(),
                contract_revision: "2026-11-21.v1".to_string(),
                required: true,
            }],
            compatibility_constraints: vec![
                CompatibilityConstraint {
                    id: "schema_and_error_parity".to_string(),
                    description: "Dactyl and Propodus schemas, error classes, and auth scopes must agree at pinned revision".to_string(),
                    enforced_by: "capability_contract::evaluate_composed_capability".to_string(),
                },
                CompatibilityConstraint {
                    id: "no_blind_replay_on_uncertain".to_string(),
                    description: "Severed ack or uncertain step requires idempotent readback reconciliation before any retry or compensation".to_string(),
                    enforced_by: "dactyl_todo + cloud_todo_operation".to_string(),
                },
            ],
        },
        negative_cases: vec![
            NegativeCaseSpec {
                id: "neg.undeclared_or_removed_field_or_error".to_string(),
                trigger: "Dactyl declares a field or error class absent from active Propodus".to_string(),
                expected_status: ContractStatus::Incompatible,
                expected_outcome_class: PreservedOutcomeClass::Failed,
                forbids_blind_replay: true,
                forbids_goal_completed: true,
            },
            NegativeCaseSpec {
                id: "neg.renamed_op_or_auth_scope_drift".to_string(),
                trigger: "Service renames operation or narrows/changes auth scope".to_string(),
                expected_status: ContractStatus::Incompatible,
                expected_outcome_class: PreservedOutcomeClass::Stale,
                forbids_blind_replay: true,
                forbids_goal_completed: true,
            },
            NegativeCaseSpec {
                id: "neg.prestate_drift_after_preparation".to_string(),
                trigger: "Concurrent actor updates guarded task version before commit".to_string(),
                expected_status: ContractStatus::Stale,
                expected_outcome_class: PreservedOutcomeClass::Stale,
                forbids_blind_replay: true,
                forbids_goal_completed: true,
            },
            NegativeCaseSpec {
                id: "neg.severed_ack_two_step_fault".to_string(),
                trigger: "Step 1 succeeds, step 2 faults or severs ack after possible commit".to_string(),
                expected_status: ContractStatus::Incompatible,
                expected_outcome_class: PreservedOutcomeClass::Uncertain,
                forbids_blind_replay: true,
                forbids_goal_completed: true,
            },
        ],
        required_checks: vec![
            RequiredCheckSpec {
                check_id: "decapod.composition_drift".to_string(),
                owner_component: "src/decapod/core/capability_contract.rs".to_string(),
                kind: "deterministic_contract_composition".to_string(),
            },
            RequiredCheckSpec {
                check_id: "propodus.transactional_readback".to_string(),
                owner_component: "propodus::dactyl_api::task_claim_v1".to_string(),
                kind: "owner_poststate_readback".to_string(),
            },
        ],
        evidence: CapabilityEvidenceProvenance {
            producer: "decapod.capability_fixture_harness".to_string(),
            producer_version: env!("CARGO_PKG_VERSION").to_string(),
            observed_revision: "decapod@efc05d792".to_string(),
            check_run_id: String::new(),
            observed_at: String::new(),
            independent_readback_verified: false,
            outcome_status: "declared_only".to_string(),
            is_self_reported: false,
        },
    }
}

/// Accepted baseline policy for [`canonical_cloud_task_contract`].
pub fn accepted_cloud_task_policy() -> AcceptedCapabilityPolicy {
    let contract = canonical_cloud_task_contract();
    AcceptedCapabilityPolicy {
        policy_digest: contract.accepted_source_revision.policy_digest.clone(),
        accepted_source_revision: contract.accepted_source_revision.revision.clone(),
        pinned_contract_revision: contract.contract_revision.clone(),
        pinned_contract_digest: Some(contract.canonical_digest()),
        required_operation_name: contract.operation_name.clone(),
        required_auth_scheme: contract.auth_scope.scheme.clone(),
        required_auth_scopes: contract.auth_scope.required_scopes.clone(),
        required_error_classes: contract.error_schema.error_classes.clone(),
        required_negative_case_ids: contract
            .negative_cases
            .iter()
            .map(|c| c.id.clone())
            .collect(),
        trusted_evidence_producers: vec![
            "propodus.postgres_readback".to_string(),
            "decapod.capability_fixture_harness".to_string(),
        ],
        requires_independent_readback: true,
        forbids_blind_replay: true,
    }
}

/// Canonical composed `Decapod -> Dactyl -> Propodus` bundle in a valid,
/// independently readback-verified state.
pub fn canonical_cloud_task_bundle() -> ComposedCapabilityBundle {
    let contract = canonical_cloud_task_contract();
    let accepted_policy = accepted_cloud_task_policy();

    let dactyl_descriptor = ServiceInterfaceDescriptor {
        service_name: "dactyl".to_string(),
        component: contract.producer.component.clone(),
        accepted_source_revision: contract.producer.accepted_source_revision.clone(),
        contract_revision: contract.contract_revision.clone(),
        operation_name: contract.operation_name.clone(),
        transport_binding: contract.transport_binding.clone(),
        auth_scope: contract.auth_scope.clone(),
        input_schema: contract.input_schema.clone(),
        output_schema: contract.output_schema.clone(),
        error_schema: contract.error_schema.clone(),
        supported_compensation_ops: vec!["task.release".to_string()],
        client_reported_http_ok: true,
    };

    let propodus_descriptor = ServiceInterfaceDescriptor {
        service_name: "propodus".to_string(),
        component: contract.owner.component.clone(),
        accepted_source_revision: contract.owner.accepted_source_revision.clone(),
        contract_revision: contract.contract_revision.clone(),
        operation_name: contract.operation_name.clone(),
        transport_binding: contract.transport_binding.clone(),
        auth_scope: contract.auth_scope.clone(),
        input_schema: contract.input_schema.clone(),
        output_schema: contract.output_schema.clone(),
        error_schema: contract.error_schema.clone(),
        supported_compensation_ops: vec!["task.release".to_string()],
        client_reported_http_ok: true,
    };

    let observed_evidence = EvidenceObservation {
        producer: "propodus.postgres_readback".to_string(),
        producer_version: "propodus@9abd49e".to_string(),
        observed_revision: contract.accepted_source_revision.revision.clone(),
        contract_revision: contract.contract_revision.clone(),
        operation_name: contract.operation_name.clone(),
        auth_scheme: contract.auth_scope.scheme.clone(),
        auth_scopes: contract.auth_scope.required_scopes.clone(),
        check_run_id: "readback_01M80CAPABILITY".to_string(),
        observed_at: "2026-11-21T17:00:00Z".to_string(),
        independent_readback_verified: true,
        outcome_class: None,
        self_reported: false,
        server_poststate_digest: Some("sha256:owner-poststate-v2-claimed".to_string()),
    };

    let prestate = PrestateSnapshot {
        task_id: "todo_01m80cloudtask".to_string(),
        status: "open".to_string(),
        assigned_to: String::new(),
        version: 1,
        lease_generation: 1,
        shared_capacity_limit: 2,
        reserved_capacity: 0,
    };

    let trace = CompositeExecutionTrace {
        trace_id: "trace_cloud_claim_observed".to_string(),
        attempt_id: "attempt_01".to_string(),
        prepared_against_prestate: prestate.clone(),
        current_owner_prestate: prestate,
        requested_capacity: 1,
        concurrent_shared_demand: 0,
        steps: vec![
            TraceStepRecord {
                step_index: 1,
                operation: "task.claim".to_string(),
                succeeded: true,
                uncertain_after_send: false,
                error_code: None,
            },
            TraceStepRecord {
                step_index: 2,
                operation: "proof_acceptance.record".to_string(),
                succeeded: true,
                uncertain_after_send: false,
                error_code: None,
            },
        ],
        async_consequence_settled: true,
        severed_ack_after_possible_commit: false,
        blind_replay_attempted: false,
        readback_observation: Some(ReadbackObservation {
            queried_via_idempotent_op: "propodus_proof_acceptance_get_v1".to_string(),
            owner_confirmed_committed: true,
            poststate_matches_intent: true,
            server_poststate_digest: "sha256:owner-poststate-v2-claimed".to_string(),
        }),
        compensation: None,
        original_fault: None,
    };

    ComposedCapabilityBundle {
        contract,
        dactyl_descriptor: Some(dactyl_descriptor),
        propodus_descriptor: Some(propodus_descriptor),
        accepted_policy,
        observed_evidence: Some(observed_evidence),
        consequence_trace: Some(trace),
        repo_context: RepositoryCapabilityContext::CloudBound {
            repository: "DecapodLabs/decapod".to_string(),
            endpoint: "https://goyxpcmxetztjlrrcwew.supabase.co".to_string(),
            active_profile: "decapod.cloud.task.v1".to_string(),
        },
    }
}

/// Canonical local-offline capability contract verifying unrelated local
/// repositories remain completely usable offline without Cloud dependency.
pub fn canonical_local_offline_bundle(repository: &str) -> ComposedCapabilityBundle {
    let mut contract = canonical_cloud_task_contract();
    contract.capability_id = LOCAL_TASK_OFFLINE_CAPABILITY_ID.to_string();
    contract.operation_name = "local.todo.claim".to_string();
    contract.producer = ServiceParticipant {
        component: "src/decapod/core/dactyl.rs::DactylBridge".to_string(),
        service_name: "dactyl-sqlite".to_string(),
        accepted_source_revision: "decapod@efc05d792".to_string(),
        contract_revision: "2026-11-21.v1".to_string(),
    };
    contract.owner = ServiceParticipant {
        component: "src/decapod/core/todo.rs::DbBroker".to_string(),
        service_name: "decapod-local".to_string(),
        accepted_source_revision: "decapod@efc05d792".to_string(),
        contract_revision: "2026-11-21.v1".to_string(),
    };
    contract.transport_binding = TransportBinding {
        connector: "dactyl_db::sqlite".to_string(),
        connector_version: "0.18.0".to_string(),
        envelope_version: "local_sqlite.v1".to_string(),
        protocol: "local_sqlite_wal".to_string(),
        endpoint_origin_policy: "local_filesystem_only".to_string(),
        supports_atomic_readback: true,
        offline_usable_without_cloud: true,
    };
    contract.auth_scope = AuthScopeContract {
        scheme: "LocalSessionToken".to_string(),
        required_scopes: vec!["local:todo:claim".to_string()],
        requires_repository_binding: true,
        requires_principal_delegation: false,
        requires_fencing_generation: true,
        forbids_client_minted_grant: true,
    };

    let mut accepted_policy = accepted_cloud_task_policy();
    accepted_policy.policy_digest = contract.accepted_source_revision.policy_digest.clone();
    accepted_policy.pinned_contract_digest = Some(contract.canonical_digest());
    accepted_policy.required_operation_name = contract.operation_name.clone();
    accepted_policy.required_auth_scheme = contract.auth_scope.scheme.clone();
    accepted_policy.required_auth_scopes = contract.auth_scope.required_scopes.clone();
    accepted_policy.trusted_evidence_producers = vec!["decapod.local_sqlite_tx".to_string()];

    let observed_evidence = EvidenceObservation {
        producer: "decapod.local_sqlite_tx".to_string(),
        producer_version: env!("CARGO_PKG_VERSION").to_string(),
        observed_revision: contract.accepted_source_revision.revision.clone(),
        contract_revision: contract.contract_revision.clone(),
        operation_name: contract.operation_name.clone(),
        auth_scheme: contract.auth_scope.scheme.clone(),
        auth_scopes: contract.auth_scope.required_scopes.clone(),
        check_run_id: "local_sqlite_01".to_string(),
        observed_at: "2026-11-21T17:00:00Z".to_string(),
        independent_readback_verified: true,
        outcome_class: None,
        self_reported: false,
        server_poststate_digest: Some("sha256:local-sqlite-claimed".to_string()),
    };

    ComposedCapabilityBundle {
        contract,
        dactyl_descriptor: None,
        propodus_descriptor: None,
        accepted_policy,
        observed_evidence: Some(observed_evidence),
        consequence_trace: None,
        repo_context: RepositoryCapabilityContext::LocalOffline {
            repository: repository.to_string(),
        },
    }
}

struct ScenarioExpectation<'a> {
    scenario_id: &'a str,
    falsifier: &'a str,
    phase: &'a str,
    expected_status: ContractStatus,
    expected_consequence_state: Option<CompositeConsequenceState>,
    expected_goal_completed: Option<bool>,
}

fn push_scenario(
    scenarios: &mut Vec<FixtureScenarioOutcome>,
    expectation: ScenarioExpectation<'_>,
    bundle: &ComposedCapabilityBundle,
) {
    let report = evaluate_composed_capability(bundle);
    let actual_state = report.consequence.as_ref().map(|c| c.state);
    let actual_goal = report.consequence.as_ref().map(|c| c.goal_completed);
    let passed = report.status == expectation.expected_status
        && expectation
            .expected_consequence_state
            .is_none_or(|exp| actual_state == Some(exp))
        && expectation
            .expected_goal_completed
            .is_none_or(|exp| actual_goal == Some(exp));
    scenarios.push(FixtureScenarioOutcome {
        scenario_id: expectation.scenario_id.to_string(),
        falsifier: expectation.falsifier.to_string(),
        phase: expectation.phase.to_string(),
        expected_status: expectation.expected_status,
        expected_consequence_state: expectation.expected_consequence_state,
        expected_goal_completed: expectation.expected_goal_completed,
        passed,
        report,
    });
}

/// Run the full multi-repository compatibility fixture and all negative +
/// repaired traces required for `#1906`.
pub fn run_compatibility_fixture_suite() -> CompatibilityFixtureReport {
    let mut scenarios = Vec::new();
    let baseline = canonical_cloud_task_bundle();

    // 0. Prepared (eligible, no effect established yet) + Observed baseline.
    let mut prepared_bundle = baseline.clone();
    prepared_bundle.observed_evidence = None;
    if let Some(trace) = prepared_bundle.consequence_trace.as_mut() {
        trace.steps.clear();
        trace.readback_observation = None;
    }
    push_scenario(
        &mut scenarios,
        ScenarioExpectation {
            scenario_id: "consequence_prepared_only",
            falsifier: "baseline_four_state_separation",
            phase: "prepared",
            expected_status: ContractStatus::Declared,
            expected_consequence_state: Some(CompositeConsequenceState::Prepared),
            expected_goal_completed: Some(false),
        },
        &prepared_bundle,
    );
    push_scenario(
        &mut scenarios,
        ScenarioExpectation {
            scenario_id: "end_to_end_observed_and_verified",
            falsifier: "baseline_four_state_separation",
            phase: "repaired",
            expected_status: ContractStatus::CurrentlyVerified,
            expected_consequence_state: Some(CompositeConsequenceState::Observed),
            expected_goal_completed: Some(true),
        },
        &baseline,
    );

    // 1. Undeclared/removed response field & dropped error class despite HTTP 200-shaped client response.
    let mut field_drift = baseline.clone();
    field_drift.observed_evidence = None;
    field_drift.consequence_trace = None;
    if let Some(propodus) = field_drift.propodus_descriptor.as_mut() {
        propodus
            .output_schema
            .required_fields
            .retain(|f| f != "readback_digest");
        propodus
            .output_schema
            .optional_fields
            .retain(|f| f != "readback_digest");
        propodus
            .error_schema
            .error_classes
            .retain(|e| e != "quota_exceeded");
        propodus.client_reported_http_ok = true;
    }
    push_scenario(
        &mut scenarios,
        ScenarioExpectation {
            scenario_id: "undeclared_removed_field_and_error_class",
            falsifier: "dactyl_declares_field_or_error_absent_from_propodus",
            phase: "negative",
            expected_status: ContractStatus::Incompatible,
            expected_consequence_state: None,
            expected_goal_completed: None,
        },
        &field_drift,
    );
    push_scenario(
        &mut scenarios,
        ScenarioExpectation {
            scenario_id: "undeclared_removed_field_and_error_class_repaired",
            falsifier: "dactyl_declares_field_or_error_absent_from_propodus",
            phase: "repaired",
            expected_status: ContractStatus::CurrentlyVerified,
            expected_consequence_state: Some(CompositeConsequenceState::Observed),
            expected_goal_completed: Some(true),
        },
        &baseline,
    );

    // 2. Service renames operation / changes auth scope / prior evidence was uncertain.
    let mut renamed_op = baseline.clone();
    renamed_op.consequence_trace = None;
    if let Some(propodus) = renamed_op.propodus_descriptor.as_mut() {
        propodus.operation_name = "task.acquire_v2".to_string();
        propodus.auth_scope.required_scopes = vec!["repo:task:claim".to_string()];
    }
    if let Some(ev) = renamed_op.observed_evidence.as_mut() {
        ev.outcome_class = Some(PreservedOutcomeClass::Uncertain);
    }
    push_scenario(
        &mut scenarios,
        ScenarioExpectation {
            scenario_id: "renamed_op_auth_scope_and_uncertain_evidence",
            falsifier: "renamed_operation_changed_scope_or_uncertain_prior_evidence",
            phase: "negative",
            expected_status: ContractStatus::Incompatible,
            expected_consequence_state: None,
            expected_goal_completed: None,
        },
        &renamed_op,
    );
    push_scenario(
        &mut scenarios,
        ScenarioExpectation {
            scenario_id: "renamed_op_auth_scope_and_uncertain_evidence_repaired",
            falsifier: "renamed_operation_changed_scope_or_uncertain_prior_evidence",
            phase: "repaired",
            expected_status: ContractStatus::CurrentlyVerified,
            expected_consequence_state: Some(CompositeConsequenceState::Observed),
            expected_goal_completed: Some(true),
        },
        &baseline,
    );

    // 3. Client & service both type-check against different contract revisions.
    let mut rev_mismatch = baseline.clone();
    rev_mismatch.consequence_trace = None;
    if let Some(dactyl) = rev_mismatch.dactyl_descriptor.as_mut() {
        dactyl.contract_revision = "2026-11-21.v2".to_string();
    }
    push_scenario(
        &mut scenarios,
        ScenarioExpectation {
            scenario_id: "client_service_contract_revision_mismatch",
            falsifier: "client_and_service_typecheck_against_different_revisions",
            phase: "negative",
            expected_status: ContractStatus::Incompatible,
            expected_consequence_state: None,
            expected_goal_completed: None,
        },
        &rev_mismatch,
    );
    push_scenario(
        &mut scenarios,
        ScenarioExpectation {
            scenario_id: "client_service_contract_revision_mismatch_repaired",
            falsifier: "client_and_service_typecheck_against_different_revisions",
            phase: "repaired",
            expected_status: ContractStatus::CurrentlyVerified,
            expected_consequence_state: Some(CompositeConsequenceState::Observed),
            expected_goal_completed: Some(true),
        },
        &baseline,
    );

    // 4. Agent edits its own local contract copy / drops negative case / self-verifies.
    let mut tampered = baseline.clone();
    tampered.consequence_trace = None;
    tampered.contract.declared_status = ContractStatus::CurrentlyVerified;
    tampered.contract.idempotency.blind_replay_forbidden = false;
    tampered
        .contract
        .negative_cases
        .retain(|c| c.id != "neg.severed_ack_two_step_fault");
    push_scenario(
        &mut scenarios,
        ScenarioExpectation {
            scenario_id: "agent_edited_contract_copy_and_self_verified",
            falsifier: "agent_edits_contract_copy_drops_negative_case_or_self_certifies",
            phase: "negative",
            expected_status: ContractStatus::Incompatible,
            expected_consequence_state: None,
            expected_goal_completed: None,
        },
        &tampered,
    );
    push_scenario(
        &mut scenarios,
        ScenarioExpectation {
            scenario_id: "agent_edited_contract_copy_repaired",
            falsifier: "agent_edits_contract_copy_drops_negative_case_or_self_certifies",
            phase: "repaired",
            expected_status: ContractStatus::CurrentlyVerified,
            expected_consequence_state: Some(CompositeConsequenceState::Observed),
            expected_goal_completed: Some(true),
        },
        &baseline,
    );

    // 5. Accepted prestate drifts after preparation.
    let mut prestate_drift = baseline.clone();
    prestate_drift.observed_evidence = None;
    if let Some(trace) = prestate_drift.consequence_trace.as_mut() {
        trace.current_owner_prestate.version = 2;
        trace.current_owner_prestate.status = "in_progress".to_string();
        trace.current_owner_prestate.assigned_to = "competing-agent".to_string();
        trace.steps.clear();
        trace.readback_observation = None;
    }
    push_scenario(
        &mut scenarios,
        ScenarioExpectation {
            scenario_id: "prestate_drift_after_preparation",
            falsifier: "accepted_prestate_changes_after_preparation",
            phase: "negative",
            expected_status: ContractStatus::Stale,
            expected_consequence_state: Some(CompositeConsequenceState::Held),
            expected_goal_completed: Some(false),
        },
        &prestate_drift,
    );
    push_scenario(
        &mut scenarios,
        ScenarioExpectation {
            scenario_id: "prestate_drift_repaired_on_fresh_state",
            falsifier: "accepted_prestate_changes_after_preparation",
            phase: "repaired",
            expected_status: ContractStatus::CurrentlyVerified,
            expected_consequence_state: Some(CompositeConsequenceState::Observed),
            expected_goal_completed: Some(true),
        },
        &baseline,
    );

    // 6. Concurrent shared-invariant violation across individually valid requests.
    let mut shared_collision = baseline.clone();
    shared_collision.observed_evidence = None;
    if let Some(trace) = shared_collision.consequence_trace.as_mut() {
        trace.current_owner_prestate.reserved_capacity = 1;
        trace.requested_capacity = 1;
        trace.concurrent_shared_demand = 1; // 1 + 1 + 1 > 2 limit
        trace.steps.clear();
        trace.readback_observation = None;
    }
    push_scenario(
        &mut scenarios,
        ScenarioExpectation {
            scenario_id: "concurrent_shared_invariant_violation",
            falsifier: "individually_valid_op_violates_shared_invariant_with_concurrent_work",
            phase: "negative",
            expected_status: ContractStatus::Incompatible,
            expected_consequence_state: Some(CompositeConsequenceState::Held),
            expected_goal_completed: Some(false),
        },
        &shared_collision,
    );
    push_scenario(
        &mut scenarios,
        ScenarioExpectation {
            scenario_id: "concurrent_shared_invariant_repaired",
            falsifier: "individually_valid_op_violates_shared_invariant_with_concurrent_work",
            phase: "repaired",
            expected_status: ContractStatus::CurrentlyVerified,
            expected_consequence_state: Some(CompositeConsequenceState::Observed),
            expected_goal_completed: Some(true),
        },
        &baseline,
    );

    // 7. Response arrives before acknowledged async consequence settles.
    let mut async_pending = baseline.clone();
    if let Some(trace) = async_pending.consequence_trace.as_mut() {
        trace.async_consequence_settled = false;
    }
    if let Some(ev) = async_pending.observed_evidence.as_mut() {
        ev.outcome_class = Some(PreservedOutcomeClass::Pending);
    }
    push_scenario(
        &mut scenarios,
        ScenarioExpectation {
            scenario_id: "async_consequence_pending_after_response",
            falsifier: "response_arrives_before_async_consequence_settles",
            phase: "negative",
            expected_status: ContractStatus::Stale,
            expected_consequence_state: Some(CompositeConsequenceState::Held),
            expected_goal_completed: Some(false),
        },
        &async_pending,
    );
    push_scenario(
        &mut scenarios,
        ScenarioExpectation {
            scenario_id: "async_consequence_settled_repaired",
            falsifier: "response_arrives_before_async_consequence_settles",
            phase: "repaired",
            expected_status: ContractStatus::CurrentlyVerified,
            expected_consequence_state: Some(CompositeConsequenceState::Observed),
            expected_goal_completed: Some(true),
        },
        &baseline,
    );

    // 8. Two-step Cloud operation: fault after step 1 + severed ack after possible
    // commit + blind replay attempted -> Held (`Uncertain`, no blind replay, goal_completed=false).
    let mut two_step_severed = baseline.clone();
    two_step_severed.observed_evidence = None;
    if let Some(trace) = two_step_severed.consequence_trace.as_mut() {
        trace.steps = vec![
            TraceStepRecord {
                step_index: 1,
                operation: "task.claim".to_string(),
                succeeded: true,
                uncertain_after_send: false,
                error_code: None,
            },
            TraceStepRecord {
                step_index: 2,
                operation: "proof_acceptance.record".to_string(),
                succeeded: false,
                uncertain_after_send: true,
                error_code: Some("severed_ack".to_string()),
            },
        ];
        trace.severed_ack_after_possible_commit = true;
        trace.blind_replay_attempted = true;
        trace.readback_observation = None;
        trace.original_fault = Some(FaultRecord {
            step: "step_2:proof_acceptance.record".to_string(),
            code: "severed_ack".to_string(),
            detail: "transport connection severed after possible server commit".to_string(),
            outcome_class: PreservedOutcomeClass::Uncertain,
        });
    }
    push_scenario(
        &mut scenarios,
        ScenarioExpectation {
            scenario_id: "two_step_fault_severed_ack_blind_replay_held",
            falsifier: "two_step_fault_after_step1_and_severed_ack",
            phase: "negative",
            expected_status: ContractStatus::Incompatible,
            expected_consequence_state: Some(CompositeConsequenceState::Held),
            expected_goal_completed: Some(false),
        },
        &two_step_severed,
    );

    // 8b. Unsupported compensation is refused cleanly (stays Held).
    let mut unsupported_comp = two_step_severed.clone();
    if let Some(trace) = unsupported_comp.consequence_trace.as_mut() {
        trace.blind_replay_attempted = false;
        trace.compensation = Some(CompensationRecord {
            operation: "task.delete_unchecked".to_string(),
            executed: false,
            final_readback_verified: false,
            original_fault_retained: true,
            poststate_digest: None,
        });
    }
    push_scenario(
        &mut scenarios,
        ScenarioExpectation {
            scenario_id: "two_step_unsupported_compensation_refused",
            falsifier: "two_step_fault_after_step1_and_severed_ack",
            phase: "negative",
            expected_status: ContractStatus::Incompatible,
            expected_consequence_state: Some(CompositeConsequenceState::Held),
            expected_goal_completed: Some(false),
        },
        &unsupported_comp,
    );

    // 8c. Repaired via authoritative readback + supported `task.release` compensation
    // retaining original fault in evidence (`Compensated`, `goal_completed=false`).
    let mut compensated = baseline.clone();
    if let Some(trace) = compensated.consequence_trace.as_mut() {
        trace.steps = vec![
            TraceStepRecord {
                step_index: 1,
                operation: "task.claim".to_string(),
                succeeded: true,
                uncertain_after_send: false,
                error_code: None,
            },
            TraceStepRecord {
                step_index: 2,
                operation: "proof_acceptance.record".to_string(),
                succeeded: false,
                uncertain_after_send: false,
                error_code: Some("proof_record_rejected".to_string()),
            },
        ];
        trace.severed_ack_after_possible_commit = false;
        trace.blind_replay_attempted = false;
        trace.readback_observation = Some(ReadbackObservation {
            queried_via_idempotent_op: "propodus_proof_acceptance_get_v1".to_string(),
            owner_confirmed_committed: false,
            poststate_matches_intent: false,
            server_poststate_digest: "sha256:step1-claimed-step2-absent".to_string(),
        });
        trace.original_fault = Some(FaultRecord {
            step: "step_2:proof_acceptance.record".to_string(),
            code: "proof_record_rejected".to_string(),
            detail: "step 2 rejected after step 1 claim; task.release compensated".to_string(),
            outcome_class: PreservedOutcomeClass::Failed,
        });
        trace.compensation = Some(CompensationRecord {
            operation: "task.release".to_string(),
            executed: true,
            final_readback_verified: true,
            original_fault_retained: true,
            poststate_digest: Some("sha256:owner-poststate-v3-open".to_string()),
        });
    }
    push_scenario(
        &mut scenarios,
        ScenarioExpectation {
            scenario_id: "two_step_fault_compensated_with_retained_fault",
            falsifier: "two_step_fault_after_step1_and_severed_ack",
            phase: "repaired",
            expected_status: ContractStatus::Observed,
            expected_consequence_state: Some(CompositeConsequenceState::Compensated),
            expected_goal_completed: Some(false),
        },
        &compensated,
    );

    // 9. Unrelated local offline repository remains unaffected & usable without Cloud binding.
    let local_repo = canonical_local_offline_bundle("local/offline-project");
    push_scenario(
        &mut scenarios,
        ScenarioExpectation {
            scenario_id: "unrelated_local_offline_repository_usable",
            falsifier: "unrelated_local_repo_without_cloud_binding_usable_offline",
            phase: "repaired",
            expected_status: ContractStatus::CurrentlyVerified,
            expected_consequence_state: None,
            expected_goal_completed: None,
        },
        &local_repo,
    );

    // 9b. Querying a Cloud-only contract in an offline local repository reports Unsupported
    // while keeping local_offline_usable=true.
    let mut cloud_in_local = baseline;
    cloud_in_local.repo_context = RepositoryCapabilityContext::LocalOffline {
        repository: "local/offline-project".to_string(),
    };
    push_scenario(
        &mut scenarios,
        ScenarioExpectation {
            scenario_id: "cloud_contract_in_local_offline_repo_unsupported",
            falsifier: "unrelated_local_repo_without_cloud_binding_usable_offline",
            phase: "negative",
            expected_status: ContractStatus::Unsupported,
            expected_consequence_state: None,
            expected_goal_completed: None,
        },
        &cloud_in_local,
    );

    let passed_scenarios = scenarios.iter().filter(|s| s.passed).count();
    let total_scenarios = scenarios.len();
    CompatibilityFixtureReport {
        schema_version: FIXTURE_SUITE_SCHEMA_VERSION.to_string(),
        total_scenarios,
        passed_scenarios,
        failed_scenarios: total_scenarios - passed_scenarios,
        scenarios,
    }
}

fn load_contract_file(path: &Path) -> Result<CapabilityContract, DecapodError> {
    let meta = fs::metadata(path).map_err(DecapodError::IoError)?;
    if meta.len() > MAX_CONTRACT_FILE_BYTES {
        return Err(DecapodError::ValidationError(format!(
            "capability contract '{}' exceeds max {} bytes",
            path.display(),
            MAX_CONTRACT_FILE_BYTES
        )));
    }
    let raw = fs::read(path).map_err(DecapodError::IoError)?;
    serde_json::from_slice::<CapabilityContract>(&raw).map_err(|e| {
        DecapodError::ValidationError(format!(
            "invalid capability contract JSON at '{}': {e}",
            path.display()
        ))
    })
}

fn repo_accepted_policy(
    main_root: &Path,
    is_cloud: bool,
) -> Result<AcceptedCapabilityPolicy, DecapodError> {
    let accepted_path = main_root.join(CAPABILITY_CONTRACT_REL_PATH);
    if accepted_path.is_file() {
        let accepted_contract = load_contract_file(&accepted_path)?;
        let mut policy = if accepted_contract
            .transport_binding
            .offline_usable_without_cloud
        {
            canonical_local_offline_bundle("local/accepted").accepted_policy
        } else {
            accepted_cloud_task_policy()
        };
        policy.policy_digest = accepted_contract
            .accepted_source_revision
            .policy_digest
            .clone();
        policy.accepted_source_revision =
            accepted_contract.accepted_source_revision.revision.clone();
        policy.pinned_contract_revision = accepted_contract.contract_revision.clone();
        policy.pinned_contract_digest = Some(accepted_contract.canonical_digest());
        policy.required_operation_name = accepted_contract.operation_name.clone();
        policy.required_auth_scheme = accepted_contract.auth_scope.scheme.clone();
        policy.required_auth_scopes = accepted_contract.auth_scope.required_scopes.clone();
        policy.required_error_classes = accepted_contract.error_schema.error_classes.clone();
        policy.required_negative_case_ids = accepted_contract
            .negative_cases
            .iter()
            .map(|c| c.id.clone())
            .collect();
        return Ok(policy);
    }

    if is_cloud {
        Ok(accepted_cloud_task_policy())
    } else {
        Ok(canonical_local_offline_bundle("local/repository").accepted_policy)
    }
}

fn resolve_main_root_hint(repo_root: &Path) -> PathBuf {
    crate::core::workspace::get_main_repo_root(repo_root)
        .unwrap_or_else(|_| repo_root.to_path_buf())
}

fn build_candidate_bundle(
    main_root: &Path,
    is_cloud: bool,
    repo_name: &str,
    candidate_contract: Option<CapabilityContract>,
) -> Result<ComposedCapabilityBundle, DecapodError> {
    match candidate_contract {
        Some(contract) => {
            let is_cloud_contract = !contract.transport_binding.offline_usable_without_cloud;
            let accepted_policy = repo_accepted_policy(main_root, is_cloud || is_cloud_contract)?;
            let mut base = if is_cloud || is_cloud_contract {
                let mut cloud = canonical_cloud_task_bundle();
                cloud.observed_evidence = None;
                cloud.consequence_trace = None;
                if !is_cloud {
                    cloud.repo_context = RepositoryCapabilityContext::LocalOffline {
                        repository: repo_name.to_string(),
                    };
                }
                cloud
            } else {
                let mut local = canonical_local_offline_bundle(repo_name);
                local.observed_evidence = None;
                local
            };
            base.contract = contract;
            base.accepted_policy = accepted_policy;
            Ok(base)
        }
        None if is_cloud => {
            let mut base = canonical_cloud_task_bundle();
            // Live runtime verification is never inferred from a compiled fixture.
            base.observed_evidence = None;
            base.consequence_trace = None;
            Ok(base)
        }
        None => {
            let mut base = canonical_local_offline_bundle(repo_name);
            base.observed_evidence = None;
            Ok(base)
        }
    }
}

fn inspect_with_candidate(
    repo_root: &Path,
    candidate_contract: Option<CapabilityContract>,
    include_fixture: bool,
) -> Result<CapabilityInspectionReport, DecapodError> {
    let config = DecapodProjectConfig::load(repo_root).ok();
    let is_cloud = config
        .as_ref()
        .is_some_and(|c| c.repo.effective_backend().is_cloud());
    let repo_name = config
        .as_ref()
        .and_then(|c| c.repo.product_name.clone())
        .unwrap_or_else(|| "local-repository".to_string());
    let main_root = resolve_main_root_hint(repo_root);
    let bundle = build_candidate_bundle(&main_root, is_cloud, &repo_name, candidate_contract)?;
    let report = evaluate_composed_capability(&bundle);
    let fixture = if include_fixture {
        Some(run_compatibility_fixture_suite())
    } else {
        None
    };

    Ok(CapabilityInspectionReport {
        schema_version: CAPABILITY_INSPECTION_SCHEMA_VERSION.to_string(),
        repository_backend: if is_cloud {
            "cloud".to_string()
        } else {
            "local".to_string()
        },
        local_offline_usable: report.local_offline_usable,
        report,
        fixture,
        authority_note: "Read-only capability contract drift check; contract declarations are evidence inputs, never authorization or runtime proof by themselves.".to_string(),
    })
}

/// Read-only inspection of composed capability contracts for a repository.
/// Never opens SQLite, starts sessions, or performs network requests.
pub fn inspect_repo_capabilities(
    repo_root: &Path,
    contract_override: Option<&Path>,
    include_fixture: bool,
) -> Result<CapabilityInspectionReport, DecapodError> {
    let candidate_contract = match contract_override {
        Some(explicit) => Some(load_contract_file(explicit)?),
        None => {
            let repo_contract = repo_root.join(CAPABILITY_CONTRACT_REL_PATH);
            if repo_contract.is_file() {
                Some(load_contract_file(&repo_contract)?)
            } else {
                None
            }
        }
    };
    inspect_with_candidate(repo_root, candidate_contract, include_fixture)
}

/// Read-only inspection of a caller-supplied [`CapabilityContract`] against the
/// repository's accepted policy.
pub fn inspect_supplied_contract(
    repo_root: &Path,
    contract: CapabilityContract,
    include_fixture: bool,
) -> Result<CapabilityInspectionReport, DecapodError> {
    inspect_with_candidate(repo_root, Some(contract), include_fixture)
}

/// Validation gate helper owned by `decapod validate` / `govern project-check`.
///
/// Local offline repositories without an explicit capability contract file stay
/// completely unaffected (`Ok(None)`). When `.decapod/contracts/capability_contract.json`
/// is present, it is evaluated against accepted host/main policy and fails closed
/// on drift, dropped negative cases, or self-verification claims.
pub fn validate_repo_contract_boundary(
    working_root: &Path,
    main_root: &Path,
) -> Result<Option<CompatibilityReport>, DecapodError> {
    let candidate_path = working_root.join(CAPABILITY_CONTRACT_REL_PATH);
    if !candidate_path.is_file() {
        return Ok(None);
    }

    let is_cloud = DecapodProjectConfig::load(working_root)
        .ok()
        .is_some_and(|c| c.repo.effective_backend().is_cloud());
    let candidate_contract = load_contract_file(&candidate_path)?;
    let bundle = build_candidate_bundle(
        main_root,
        is_cloud,
        "local/repository",
        Some(candidate_contract),
    )?;

    let report = evaluate_composed_capability(&bundle);
    if matches!(
        report.status,
        ContractStatus::Incompatible | ContractStatus::Stale
    ) {
        let summary = report
            .findings
            .iter()
            .map(|f| format!("[{}] {}", f.code, f.detail))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(DecapodError::ValidationError(format!(
            "CAPABILITY_CONTRACT_DRIFT: composed capability contract '{}' status={}: {summary}",
            report.capability_id,
            report.status.as_str()
        )));
    }
    Ok(Some(report))
}

#[cfg(test)]
#[path = "../../../tests/unit/core/capability_contract_tests.rs"]
mod tests;
