//! Versioned logical operation & evidence parity matrix for Decapod Local vs
//! Decapod Cloud (#1895).
//!
//! Guardrails enforced by construction:
//! - Descriptive only: this matrix never grants authorization, never selects or
//!   discloses tenant database placement, and never substitutes for runtime
//!   validation or server-side proof.
//! - Never claims blanket parity: every supported logical operation and
//!   invariant is explicitly enumerated alongside operations that are
//!   explicitly unsupported in Cloud mode (which fail closed without SQLite
//!   fallback).
//! - Read-only and offline-safe: inspection never initializes `.decapod/data/decapod.db`
//!   and never requires Cloud credentials or network I/O.

use crate::cli::{BackendType, DecapodProjectConfig};
use crate::core::error::DecapodError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::Path;

pub const CLOUD_PARITY_MATRIX_SCHEMA_VERSION: &str = "decapod.cloud-parity-matrix.v1";
pub const CLOUD_PARITY_REPORT_SCHEMA_VERSION: &str = "decapod.cloud-parity-report.v1";
pub const CLOUD_PARITY_MATRIX_VERSION: &str = "1.0.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParityDomain {
    TaskEventCasOrdering,
    CustodyAndLeases,
    AuthorizationScopedTransitions,
    CommitPolicyBoundEvidence,
    BackendIsolationAndPlacement,
}

impl ParityDomain {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TaskEventCasOrdering => "task_event_cas_ordering",
            Self::CustodyAndLeases => "custody_and_leases",
            Self::AuthorizationScopedTransitions => "authorization_scoped_transitions",
            Self::CommitPolicyBoundEvidence => "commit_policy_bound_evidence",
            Self::BackendIsolationAndPlacement => "backend_isolation_and_placement",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParitySupportClass {
    /// Preserves equivalent logical entity, ordering, CAS, and proof invariants
    /// across `local` SQLite and `cloud` Dactyl/Propodus without byte-for-byte
    /// physical equivalence.
    SupportedLogicalParity,
    /// Hosted control-plane / tenancy boundary enforced in Cloud mode with no
    /// local multi-tenant equivalent.
    SupportedCloudBoundary,
    /// Explicitly unsupported in Cloud mode; fails closed without falling back
    /// to local SQLite.
    UnsupportedInCloud,
}

impl ParitySupportClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SupportedLogicalParity => "supported_logical_parity",
            Self::SupportedCloudBoundary => "supported_cloud_boundary",
            Self::UnsupportedInCloud => "unsupported_in_cloud",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlacementBoundaryDeclaration {
    pub exposed_config_enum: Vec<String>,
    pub opaque_server_placement: bool,
    pub forbidden_client_disclosures: Vec<String>,
    pub local_db_fallback_permitted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParityOperationEntry {
    pub operation_id: String,
    pub domain: ParityDomain,
    pub support_class: ParitySupportClass,
    pub local_carrier: String,
    pub cloud_carrier: String,
    pub local_semantics: String,
    pub cloud_semantics: String,
    pub shared_invariants: Vec<String>,
    pub failure_mode_on_denial_or_offline: String,
    pub proof_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnsupportedCloudOperation {
    pub operation_id: String,
    pub domain: ParityDomain,
    pub local_supported: bool,
    pub cloud_behavior: String,
    pub reason_code: String,
    pub never_falls_back_to_sqlite: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParityInvariant {
    pub id: String,
    pub domain: ParityDomain,
    pub statement: String,
    pub applies_to_local: bool,
    pub applies_to_cloud: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParitySummary {
    pub total_supported_operations: usize,
    pub supported_logical_parity_count: usize,
    pub supported_cloud_boundary_count: usize,
    pub unsupported_cloud_count: usize,
    pub invariant_count: usize,
    pub blanket_parity_claimed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudParityMatrix {
    pub schema_version: String,
    pub matrix_version: String,
    pub authority_declaration: String,
    pub placement_boundary: PlacementBoundaryDeclaration,
    pub operations: Vec<ParityOperationEntry>,
    pub unsupported_cloud_operations: Vec<UnsupportedCloudOperation>,
    pub invariants: Vec<ParityInvariant>,
    pub summary: ParitySummary,
    pub matrix_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParityMatrixCrossCheck {
    pub consistent: bool,
    pub duplicate_operation_ids: Vec<String>,
    pub overlapping_supported_unsupported_ids: Vec<String>,
    pub missing_invariant_links: Vec<String>,
    pub blanket_parity_rejected: bool,
    pub cloud_sqlite_fallback_forbidden: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudParityInspectionReport {
    pub schema_version: String,
    pub authority_note: String,
    pub repository_backend: String,
    pub local_sqlite_path_present: bool,
    pub local_offline_usable: bool,
    pub blanket_parity_claimed: bool,
    pub matrix: CloudParityMatrix,
    pub cross_check: ParityMatrixCrossCheck,
}

/// Build the canonical, versioned Local vs Cloud logical operation & evidence
/// parity matrix.
pub fn build_parity_matrix() -> CloudParityMatrix {
    let operations = vec![
        ParityOperationEntry {
            operation_id: "todo.add".to_string(),
            domain: ParityDomain::TaskEventCasOrdering,
            support_class: ParitySupportClass::SupportedLogicalParity,
            local_carrier: "Dactyl SQLite `.decapod/data/decapod.db` (todo.db)".to_string(),
            cloud_carrier: "Dactyl `/rest/v1/rpc/dactyl_batch_v1` scoped to authorized `repository_key`".to_string(),
            local_semantics: "Inserts version=1 task row and appends ordered `task.add` event on stream `todo` inside one atomic transaction.".to_string(),
            cloud_semantics: "Executes atomic batch: inserts version=1 task row, appends monotonic `events.seq` (`task.add`) with operation_id request-fingerprint idempotency, and reads back verified task.".to_string(),
            shared_invariants: vec![
                "INV-ATOMIC-TASK-EVENT".to_string(),
                "INV-IDEMPOTENT-REPLAY-BOUND".to_string(),
                "INV-NO-SILENT-FALLBACK".to_string(),
            ],
            failure_mode_on_denial_or_offline: "Returns typed CloudAuth / CloudTodo failure (`OutcomeUnknown` on post-send uncertainty) and never creates `.decapod/data/decapod.db`.".to_string(),
            proof_refs: vec![
                "src/decapod/core/dactyl_todo.rs".to_string(),
                "tests/cloud_tenancy_parity_1895.rs".to_string(),
            ],
        },
        ParityOperationEntry {
            operation_id: "todo.list".to_string(),
            domain: ParityDomain::TaskEventCasOrdering,
            support_class: ParitySupportClass::SupportedLogicalParity,
            local_carrier: "Dactyl SQLite ReadOnly query".to_string(),
            cloud_carrier: "Dactyl `/rest/v1/rpc/dactyl_query_v1` ReadOnly query".to_string(),
            local_semantics: "Lists repository-local tasks ordered deterministically (`updated_at DESC, id ASC`).".to_string(),
            cloud_semantics: "Queries server-authorized repository storage only (`updated_at DESC, id ASC`), rejecting unassigned repos and cross-tenant reads.".to_string(),
            shared_invariants: vec![
                "INV-REPO-ISOLATION".to_string(),
                "INV-NO-SILENT-FALLBACK".to_string(),
            ],
            failure_mode_on_denial_or_offline: "Denies unassigned seat/org/offline requests without opening local SQLite.".to_string(),
            proof_refs: vec![
                "src/decapod/core/dactyl_todo.rs".to_string(),
                "tests/cloud_tenancy_parity_1895.rs".to_string(),
            ],
        },
        ParityOperationEntry {
            operation_id: "todo.get".to_string(),
            domain: ParityDomain::TaskEventCasOrdering,
            support_class: ParitySupportClass::SupportedLogicalParity,
            local_carrier: "Dactyl SQLite ReadOnly lookup".to_string(),
            cloud_carrier: "Dactyl `/rest/v1/rpc/dactyl_query_v1` ReadOnly lookup".to_string(),
            local_semantics: "Reads single task by exact ID inside repository datastore.".to_string(),
            cloud_semantics: "Reads single task by exact ID inside authorized repository scope; cross-repo/cross-org IDs return not_found or entitlement denial.".to_string(),
            shared_invariants: vec![
                "INV-REPO-ISOLATION".to_string(),
                "INV-NO-SILENT-FALLBACK".to_string(),
            ],
            failure_mode_on_denial_or_offline: "Fails closed without touching `.decapod/data/decapod.db`.".to_string(),
            proof_refs: vec![
                "src/decapod/core/dactyl_todo.rs".to_string(),
                "tests/cloud_tenancy_parity_1895.rs".to_string(),
            ],
        },
        ParityOperationEntry {
            operation_id: "todo.claim".to_string(),
            domain: ParityDomain::TaskEventCasOrdering,
            support_class: ParitySupportClass::SupportedLogicalParity,
            local_carrier: "Dactyl SQLite transaction + `authoring_bindings`".to_string(),
            cloud_carrier: "Dactyl `/rest/v1/rpc/dactyl_batch_v1` CAS update + `task.claim` event".to_string(),
            local_semantics: "Single-winner exclusive claim checks open/unassigned state, increments version, and emits `task.claim`.".to_string(),
            cloud_semantics: "Single-winner CAS update (`WHERE status IN ('open','pending') AND (assigned_to = '' OR assigned_to IS NULL)`), increments `version`, emits ordered `task.claim` event and local lease custody fence.".to_string(),
            shared_invariants: vec![
                "INV-ATOMIC-TASK-EVENT".to_string(),
                "INV-SINGLE-WINNER-CAS".to_string(),
                "INV-NO-SILENT-FALLBACK".to_string(),
            ],
            failure_mode_on_denial_or_offline: "Loser or unauthorized caller fails closed; task version and assignee remain intact.".to_string(),
            proof_refs: vec![
                "tests/cloud_wave_parity.rs".to_string(),
                "tests/cloud_tenancy_parity_1895.rs".to_string(),
            ],
        },
        ParityOperationEntry {
            operation_id: "todo.release".to_string(),
            domain: ParityDomain::TaskEventCasOrdering,
            support_class: ParitySupportClass::SupportedLogicalParity,
            local_carrier: "Dactyl SQLite transaction".to_string(),
            cloud_carrier: "Dactyl `/rest/v1/rpc/dactyl_batch_v1` CAS release + `task.release` event".to_string(),
            local_semantics: "Holder-only release transitions `in_progress -> open`, increments version, and emits `task.release`.".to_string(),
            cloud_semantics: "Holder-scoped CAS update (`WHERE status='in_progress' AND assigned_to=$actor`), increments `version`, clears custody fence, emits `task.release`.".to_string(),
            shared_invariants: vec![
                "INV-ATOMIC-TASK-EVENT".to_string(),
                "INV-HOLDER-ONLY-RELEASE".to_string(),
            ],
            failure_mode_on_denial_or_offline: "Non-holder release or revoked session fails closed without altering server state or creating local SQLite.".to_string(),
            proof_refs: vec![
                "src/decapod/core/dactyl_todo.rs".to_string(),
                "tests/cloud_tenancy_parity_1895.rs".to_string(),
            ],
        },
        ParityOperationEntry {
            operation_id: "todo.done".to_string(),
            domain: ParityDomain::TaskEventCasOrdering,
            support_class: ParitySupportClass::SupportedLogicalParity,
            local_carrier: "Dactyl SQLite transaction".to_string(),
            cloud_carrier: "Dactyl `/rest/v1/rpc/dactyl_batch_v1` CAS completion + `task.done` event".to_string(),
            local_semantics: "Holder-only completion sets status=`completed`, increments version, and emits `task.done`.".to_string(),
            cloud_semantics: "Holder-scoped CAS update (`WHERE status='in_progress' AND assigned_to=$actor`), increments `version`, emits ordered `task.done`.".to_string(),
            shared_invariants: vec![
                "INV-ATOMIC-TASK-EVENT".to_string(),
                "INV-HOLDER-ONLY-COMPLETION".to_string(),
            ],
            failure_mode_on_denial_or_offline: "Non-holder or unauthenticated completion fails closed without local SQLite fallback.".to_string(),
            proof_refs: vec![
                "src/decapod/core/dactyl_todo.rs".to_string(),
                "tests/cloud_tenancy_parity_1895.rs".to_string(),
            ],
        },
        ParityOperationEntry {
            operation_id: "workspace.custody.bind".to_string(),
            domain: ParityDomain::CustodyAndLeases,
            support_class: ParitySupportClass::SupportedLogicalParity,
            local_carrier: "`.decapod/data/todo.db` `authoring_bindings` + `.decapod/data/workspace-leases/`".to_string(),
            cloud_carrier: "`propodus_proof_acceptance_v1` + private machine `CloudCustodyStore` (outside `.decapod/data/`)".to_string(),
            local_semantics: "Binds 1-to-1 worktree <-> task lease generation without allowing concurrent double-bind.".to_string(),
            cloud_semantics: "Binds worktree custody fence (`custody_id`, `task_revision`, `fencing_generation`), reconciles `outcome_unknown` via `get_operation` + `get_custody`, never writes `.decapod/data/decapod.db`.".to_string(),
            shared_invariants: vec![
                "INV-FENCING-MONOTONIC".to_string(),
                "INV-IDEMPOTENT-REPLAY-BOUND".to_string(),
                "INV-NO-SILENT-FALLBACK".to_string(),
            ],
            failure_mode_on_denial_or_offline: "Preserves pending `operation_id` for explicit reconciliation and refuses blind replay or local SQLite creation.".to_string(),
            proof_refs: vec![
                "src/decapod/core/cloud_workspace.rs".to_string(),
                "tests/cloud_workspace_validation_1886.rs".to_string(),
                "tests/cloud_tenancy_parity_1895.rs".to_string(),
            ],
        },
        ParityOperationEntry {
            operation_id: "cloud.project.select".to_string(),
            domain: ParityDomain::AuthorizationScopedTransitions,
            support_class: ParitySupportClass::SupportedCloudBoundary,
            local_carrier: "Not applicable (local checkout is single-repo offline authority)".to_string(),
            cloud_carrier: "Propodus `account-status` + private `SessionStore` project selection (`--org` disambiguation)".to_string(),
            local_semantics: "Local repositories select no remote org/project and require no account.".to_string(),
            cloud_semantics: "Requires server-verified `organization_id` and `project_id`; refuses ambiguous names, cross-org collisions, and `admin=true, repo_assigned=false` without a paid seat (#215).".to_string(),
            shared_invariants: vec![
                "INV-CLI-SELECTION-NOT-GRANT".to_string(),
                "INV-ORTHOGONAL-ORG-ADMIN".to_string(),
                "INV-OPAQUE-PLACEMENT".to_string(),
            ],
            failure_mode_on_denial_or_offline: "Returns typed `CloudAuthDiagnostic` (e.g., `AdminApprovalRequired`, `ActionRequired`); never creates local SQLite.".to_string(),
            proof_refs: vec![
                "src/decapod/core/cloud_project.rs".to_string(),
                "tests/cloud_tenancy_parity_1895.rs".to_string(),
            ],
        },
        ParityOperationEntry {
            operation_id: "proof.lifecycle.bind".to_string(),
            domain: ParityDomain::CommitPolicyBoundEvidence,
            support_class: ParitySupportClass::SupportedLogicalParity,
            local_carrier: "Local `.decapod/governance.json` + validation history bound to exact commit SHA and policy fingerprint".to_string(),
            cloud_carrier: "Propodus `propodus_proof_acceptance_v1` (`todo done --validated`) bound to exact `source_commit`, `target_commit`, `policy_revision`, and `task_revision`".to_string(),
            local_semantics: "Binds validation/proof receipts to immutable commit SHA, target branch, and policy digest; refuses stale or mismatched commits.".to_string(),
            cloud_semantics: "Submits commit/policy-bound proof capture (`captured` state, `acceptance_supported=false` until independent server acceptance); refuses stale revisions and unassigned seats.".to_string(),
            shared_invariants: vec![
                "INV-COMMIT-POLICY-BOUND-EVIDENCE".to_string(),
                "INV-NO-SILENT-FALLBACK".to_string(),
            ],
            failure_mode_on_denial_or_offline: "Returns `CLOUD_ENTITLEMENT_DENIED` / `CLOUD_PROOF_CONFLICT` / `CLOUD_OUTCOME_UNKNOWN`; never fabricates local receipts.".to_string(),
            proof_refs: vec![
                "src/decapod/core/cloud_proof.rs".to_string(),
                "tests/cloud_tenancy_parity_1895.rs".to_string(),
            ],
        },
        ParityOperationEntry {
            operation_id: "backend.isolation.no_local_sqlite".to_string(),
            domain: ParityDomain::BackendIsolationAndPlacement,
            support_class: ParitySupportClass::SupportedCloudBoundary,
            local_carrier: "Uses `.decapod/data/decapod.db` locally offline".to_string(),
            cloud_carrier: "Uses Dactyl HTTP transport only; tenant DB placement remains completely opaque to Decapod".to_string(),
            local_semantics: "Local-first SQLite operates completely offline without Cloud identity or network.".to_string(),
            cloud_semantics: "Cloud mode never creates, opens, or syncs `.decapod/data/decapod.db` across CLI restarts, worktrees, or network cuts, and never observes tenant DB names, URLs, roles, or placement generations.".to_string(),
            shared_invariants: vec![
                "INV-NO-SILENT-FALLBACK".to_string(),
                "INV-OPAQUE-PLACEMENT".to_string(),
            ],
            failure_mode_on_denial_or_offline: "Fails closed with typed Cloud error while `.decapod/data/decapod.db` remains absent.".to_string(),
            proof_refs: vec![
                "tests/cloud_wave_parity.rs".to_string(),
                "tests/cloud_tenancy_parity_1895.rs".to_string(),
            ],
        },
    ];

    let unsupported_cloud_operations = vec![
        UnsupportedCloudOperation {
            operation_id: "todo.lease.renew".to_string(),
            domain: ParityDomain::CustodyAndLeases,
            local_supported: true,
            cloud_behavior: "Fails closed immediately with `CLOUD_LEASE_RENEW_UNSUPPORTED`; hosted task contract has no authoritative lease renewal op.".to_string(),
            reason_code: "CLOUD_LEASE_RENEW_UNSUPPORTED".to_string(),
            never_falls_back_to_sqlite: true,
        },
        UnsupportedCloudOperation {
            operation_id: "todo.claim.shared".to_string(),
            domain: ParityDomain::AuthorizationScopedTransitions,
            local_supported: true,
            cloud_behavior: "Fails closed (`governance.allow_basic_shared_claims` is local-only); Cloud-backed claims do not use cooperative local shared-claim opt-in.".to_string(),
            reason_code: "CLOUD_UNSUPPORTED_OPERATION".to_string(),
            never_falls_back_to_sqlite: true,
        },
        UnsupportedCloudOperation {
            operation_id: "todo.edit".to_string(),
            domain: ParityDomain::TaskEventCasOrdering,
            local_supported: true,
            cloud_behavior: "Returns `NotImplemented` (`this todo operation is not in the cloud Dactyl contract; the cloud backend never falls back to local SQLite`).".to_string(),
            reason_code: "CLOUD_UNSUPPORTED_OPERATION".to_string(),
            never_falls_back_to_sqlite: true,
        },
        UnsupportedCloudOperation {
            operation_id: "todo.comment".to_string(),
            domain: ParityDomain::TaskEventCasOrdering,
            local_supported: true,
            cloud_behavior: "Returns `NotImplemented` without touching `.decapod/data/decapod.db`.".to_string(),
            reason_code: "CLOUD_UNSUPPORTED_OPERATION".to_string(),
            never_falls_back_to_sqlite: true,
        },
        UnsupportedCloudOperation {
            operation_id: "todo.handoff".to_string(),
            domain: ParityDomain::AuthorizationScopedTransitions,
            local_supported: true,
            cloud_behavior: "Returns `NotImplemented` without local SQLite fallback.".to_string(),
            reason_code: "CLOUD_UNSUPPORTED_OPERATION".to_string(),
            never_falls_back_to_sqlite: true,
        },
        UnsupportedCloudOperation {
            operation_id: "todo.archive".to_string(),
            domain: ParityDomain::TaskEventCasOrdering,
            local_supported: true,
            cloud_behavior: "Returns `NotImplemented` without local SQLite fallback.".to_string(),
            reason_code: "CLOUD_UNSUPPORTED_OPERATION".to_string(),
            never_falls_back_to_sqlite: true,
        },
        UnsupportedCloudOperation {
            operation_id: "todo.transfer.export_import".to_string(),
            domain: ParityDomain::CommitPolicyBoundEvidence,
            local_supported: true,
            cloud_behavior: "Local-only sealed task transfer; Cloud tasks are server-hosted and fail closed without local SQLite fallback.".to_string(),
            reason_code: "CLOUD_UNSUPPORTED_OPERATION".to_string(),
            never_falls_back_to_sqlite: true,
        },
        UnsupportedCloudOperation {
            operation_id: "todo.journal.reconcile_legacy".to_string(),
            domain: ParityDomain::BackendIsolationAndPlacement,
            local_supported: true,
            cloud_behavior: "Local-only SQLite legacy todo journal reconciliation; never applicable or executed on Cloud backend.".to_string(),
            reason_code: "CLOUD_UNSUPPORTED_OPERATION".to_string(),
            never_falls_back_to_sqlite: true,
        },
        UnsupportedCloudOperation {
            operation_id: "qa.verify.commit".to_string(),
            domain: ParityDomain::CommitPolicyBoundEvidence,
            local_supported: true,
            cloud_behavior: "Cloud-backed historical commit completion lookup is unsupported (`todo done --validated --commit` routes only through proof capture, not historical local archive).".to_string(),
            reason_code: "CLOUD_HISTORICAL_LOOKUP_UNSUPPORTED".to_string(),
            never_falls_back_to_sqlite: true,
        },
        UnsupportedCloudOperation {
            operation_id: "data.database.maintenance".to_string(),
            domain: ParityDomain::AuthorizationScopedTransitions,
            local_supported: true,
            cloud_behavior: "Fails closed (`CLOUD_PRINCIPAL_UNVERIFIED` / `CAPABILITY_UNSUPPORTED`); Decapod has zero direct SQL/DDL or backup/recover privilege over hosted tenant databases.".to_string(),
            reason_code: "CLOUD_PRINCIPAL_UNVERIFIED".to_string(),
            never_falls_back_to_sqlite: true,
        },
        UnsupportedCloudOperation {
            operation_id: "knowledge.store".to_string(),
            domain: ParityDomain::BackendIsolationAndPlacement,
            local_supported: true,
            cloud_behavior: "Fails closed (`CLOUD_BACKEND_UNAVAILABLE`); knowledge store is local-only and never falls back to SQLite under `backend = \"cloud\"`.".to_string(),
            reason_code: "CLOUD_BACKEND_UNAVAILABLE".to_string(),
            never_falls_back_to_sqlite: true,
        },
        UnsupportedCloudOperation {
            operation_id: "federation.store".to_string(),
            domain: ParityDomain::BackendIsolationAndPlacement,
            local_supported: true,
            cloud_behavior: "Fails closed (`CLOUD_BACKEND_UNAVAILABLE`); federation graph nodes/edges remain local-only without SQLite fallback in Cloud mode.".to_string(),
            reason_code: "CLOUD_BACKEND_UNAVAILABLE".to_string(),
            never_falls_back_to_sqlite: true,
        },
        UnsupportedCloudOperation {
            operation_id: "decide.store".to_string(),
            domain: ParityDomain::BackendIsolationAndPlacement,
            local_supported: true,
            cloud_behavior: "Fails closed (`CLOUD_BACKEND_UNAVAILABLE`); decision tree sessions/records remain local-only without SQLite fallback in Cloud mode.".to_string(),
            reason_code: "CLOUD_BACKEND_UNAVAILABLE".to_string(),
            never_falls_back_to_sqlite: true,
        },
        UnsupportedCloudOperation {
            operation_id: "health.store".to_string(),
            domain: ParityDomain::BackendIsolationAndPlacement,
            local_supported: true,
            cloud_behavior: "Fails closed (`CLOUD_BACKEND_UNAVAILABLE`); health claims/proof_events remain local-only without SQLite fallback in Cloud mode.".to_string(),
            reason_code: "CLOUD_BACKEND_UNAVAILABLE".to_string(),
            never_falls_back_to_sqlite: true,
        },
        UnsupportedCloudOperation {
            operation_id: "aptitude.store".to_string(),
            domain: ParityDomain::BackendIsolationAndPlacement,
            local_supported: true,
            cloud_behavior: "Fails closed (`CLOUD_BACKEND_UNAVAILABLE`); aptitude preferences/prompts/patterns remain local-only without SQLite fallback in Cloud mode.".to_string(),
            reason_code: "CLOUD_BACKEND_UNAVAILABLE".to_string(),
            never_falls_back_to_sqlite: true,
        },
        UnsupportedCloudOperation {
            operation_id: "archive.store".to_string(),
            domain: ParityDomain::BackendIsolationAndPlacement,
            local_supported: true,
            cloud_behavior: "Fails closed (`CLOUD_BACKEND_UNAVAILABLE`); archive registration remains local-only without SQLite fallback in Cloud mode.".to_string(),
            reason_code: "CLOUD_BACKEND_UNAVAILABLE".to_string(),
            never_falls_back_to_sqlite: true,
        },
        UnsupportedCloudOperation {
            operation_id: "feedback.store".to_string(),
            domain: ParityDomain::BackendIsolationAndPlacement,
            local_supported: true,
            cloud_behavior: "Fails closed (`CLOUD_BACKEND_UNAVAILABLE`); feedback capture remains local-only without SQLite fallback in Cloud mode.".to_string(),
            reason_code: "CLOUD_BACKEND_UNAVAILABLE".to_string(),
            never_falls_back_to_sqlite: true,
        },
        UnsupportedCloudOperation {
            operation_id: "cron.store".to_string(),
            domain: ParityDomain::BackendIsolationAndPlacement,
            local_supported: true,
            cloud_behavior: "Fails closed (`CLOUD_BACKEND_UNAVAILABLE`); scheduled cron jobs remain local-only without SQLite fallback in Cloud mode.".to_string(),
            reason_code: "CLOUD_BACKEND_UNAVAILABLE".to_string(),
            never_falls_back_to_sqlite: true,
        },
        UnsupportedCloudOperation {
            operation_id: "reflex.store".to_string(),
            domain: ParityDomain::BackendIsolationAndPlacement,
            local_supported: true,
            cloud_behavior: "Fails closed (`CLOUD_BACKEND_UNAVAILABLE`); reflex rules remain local-only without SQLite fallback in Cloud mode.".to_string(),
            reason_code: "CLOUD_BACKEND_UNAVAILABLE".to_string(),
            never_falls_back_to_sqlite: true,
        },
        UnsupportedCloudOperation {
            operation_id: "lcm.store".to_string(),
            domain: ParityDomain::BackendIsolationAndPlacement,
            local_supported: true,
            cloud_behavior: "Fails closed (`CLOUD_BACKEND_UNAVAILABLE`); LCM originals/summaries remain local-only without SQLite fallback in Cloud mode.".to_string(),
            reason_code: "CLOUD_BACKEND_UNAVAILABLE".to_string(),
            never_falls_back_to_sqlite: true,
        },
        UnsupportedCloudOperation {
            operation_id: "primitives.store".to_string(),
            domain: ParityDomain::BackendIsolationAndPlacement,
            local_supported: true,
            cloud_behavior: "Fails closed (`CLOUD_BACKEND_UNAVAILABLE`); risk_zones/categories/obligations remain local-only without SQLite fallback in Cloud mode.".to_string(),
            reason_code: "CLOUD_BACKEND_UNAVAILABLE".to_string(),
            never_falls_back_to_sqlite: true,
        },
        UnsupportedCloudOperation {
            operation_id: "map.store".to_string(),
            domain: ParityDomain::BackendIsolationAndPlacement,
            local_supported: true,
            cloud_behavior: "Fails closed (`CLOUD_BACKEND_UNAVAILABLE`); map persistence remains local-only without SQLite fallback in Cloud mode.".to_string(),
            reason_code: "CLOUD_BACKEND_UNAVAILABLE".to_string(),
            never_falls_back_to_sqlite: true,
        },
        UnsupportedCloudOperation {
            operation_id: "policy.store".to_string(),
            domain: ParityDomain::BackendIsolationAndPlacement,
            local_supported: true,
            cloud_behavior: "Fails closed (`CLOUD_BACKEND_UNAVAILABLE`); local SQLite policy approvals table is skipped in Cloud mode.".to_string(),
            reason_code: "CLOUD_BACKEND_UNAVAILABLE".to_string(),
            never_falls_back_to_sqlite: true,
        },
        UnsupportedCloudOperation {
            operation_id: "data.broker".to_string(),
            domain: ParityDomain::BackendIsolationAndPlacement,
            local_supported: true,
            cloud_behavior: "Fails closed (`CLOUD_BACKEND_UNAVAILABLE`); local DbBroker audit CLI is unavailable in Cloud mode without SQLite fallback.".to_string(),
            reason_code: "CLOUD_BACKEND_UNAVAILABLE".to_string(),
            never_falls_back_to_sqlite: true,
        },
        UnsupportedCloudOperation {
            operation_id: "data.compact".to_string(),
            domain: ParityDomain::BackendIsolationAndPlacement,
            local_supported: true,
            cloud_behavior: "Fails closed (`DATA_COMPACT_CLOUD_UNSUPPORTED`); local SQLite compaction is never substituted for server-managed PostgreSQL.".to_string(),
            reason_code: "DATA_COMPACT_CLOUD_UNSUPPORTED".to_string(),
            never_falls_back_to_sqlite: true,
        },
        UnsupportedCloudOperation {
            operation_id: "store.upsert".to_string(),
            domain: ParityDomain::BackendIsolationAndPlacement,
            local_supported: true,
            cloud_behavior: "Fails closed (`CLOUD_BACKEND_UNAVAILABLE`); RPC store.upsert never falls back to `.decapod/data/decapod.db` in Cloud mode.".to_string(),
            reason_code: "CLOUD_BACKEND_UNAVAILABLE".to_string(),
            never_falls_back_to_sqlite: true,
        },
        UnsupportedCloudOperation {
            operation_id: "store.query".to_string(),
            domain: ParityDomain::BackendIsolationAndPlacement,
            local_supported: true,
            cloud_behavior: "Fails closed (`CLOUD_BACKEND_UNAVAILABLE`); RPC store.query never falls back to `.decapod/data/decapod.db` in Cloud mode.".to_string(),
            reason_code: "CLOUD_BACKEND_UNAVAILABLE".to_string(),
            never_falls_back_to_sqlite: true,
        },
    ];

    let invariants = vec![
        ParityInvariant {
            id: "INV-ATOMIC-TASK-EVENT".to_string(),
            domain: ParityDomain::TaskEventCasOrdering,
            statement: "Every supported task mutation commits task state change and ordered stream event atomically; partial writes are never reported successful.".to_string(),
            applies_to_local: true,
            applies_to_cloud: true,
        },
        ParityInvariant {
            id: "INV-SINGLE-WINNER-CAS".to_string(),
            domain: ParityDomain::TaskEventCasOrdering,
            statement: "Concurrent claims on one open task yield at most one winner via monotonic version CAS.".to_string(),
            applies_to_local: true,
            applies_to_cloud: true,
        },
        ParityInvariant {
            id: "INV-HOLDER-ONLY-RELEASE".to_string(),
            domain: ParityDomain::CustodyAndLeases,
            statement: "Only the active task holder may release an in-progress task.".to_string(),
            applies_to_local: true,
            applies_to_cloud: true,
        },
        ParityInvariant {
            id: "INV-HOLDER-ONLY-COMPLETION".to_string(),
            domain: ParityDomain::TaskEventCasOrdering,
            statement: "Only the active task holder may complete an in-progress task.".to_string(),
            applies_to_local: true,
            applies_to_cloud: true,
        },
        ParityInvariant {
            id: "INV-FENCING-MONOTONIC".to_string(),
            domain: ParityDomain::CustodyAndLeases,
            statement: "Workspace custody bindings enforce monotonic fencing generation and revision matching.".to_string(),
            applies_to_local: true,
            applies_to_cloud: true,
        },
        ParityInvariant {
            id: "INV-IDEMPOTENT-REPLAY-BOUND".to_string(),
            domain: ParityDomain::TaskEventCasOrdering,
            statement: "Operation IDs bind to exact repository and payload fingerprints; cross-repo or changed-payload reuse fails closed.".to_string(),
            applies_to_local: true,
            applies_to_cloud: true,
        },
        ParityInvariant {
            id: "INV-REPO-ISOLATION".to_string(),
            domain: ParityDomain::AuthorizationScopedTransitions,
            statement: "Repositories in the same organization or local machine remain strictly isolated by repository scope.".to_string(),
            applies_to_local: true,
            applies_to_cloud: true,
        },
        ParityInvariant {
            id: "INV-CLI-SELECTION-NOT-GRANT".to_string(),
            domain: ParityDomain::AuthorizationScopedTransitions,
            statement: "Selecting an organization or project via CLI never grants authorization; server entitlement and seat assignment govern every read and mutation.".to_string(),
            applies_to_local: false,
            applies_to_cloud: true,
        },
        ParityInvariant {
            id: "INV-ORTHOGONAL-ORG-ADMIN".to_string(),
            domain: ParityDomain::AuthorizationScopedTransitions,
            statement: "Organization administrator privilege (`admin=true`) grants zero implicit Decapod governance data seats (`repo_assigned=false` fails closed on governance reads/writes, #215).".to_string(),
            applies_to_local: false,
            applies_to_cloud: true,
        },
        ParityInvariant {
            id: "INV-COMMIT-POLICY-BOUND-EVIDENCE".to_string(),
            domain: ParityDomain::CommitPolicyBoundEvidence,
            statement: "Validation and proof capture remain bound to exact source/target commit SHAs and policy revisions.".to_string(),
            applies_to_local: true,
            applies_to_cloud: true,
        },
        ParityInvariant {
            id: "INV-NO-SILENT-FALLBACK".to_string(),
            domain: ParityDomain::BackendIsolationAndPlacement,
            statement: "Backend `cloud` never creates, reads, writes, or falls back to `.decapod/data/decapod.db` across CLI restarts, worktrees, denials, or network cuts.".to_string(),
            applies_to_local: false,
            applies_to_cloud: true,
        },
        ParityInvariant {
            id: "INV-OPAQUE-PLACEMENT".to_string(),
            domain: ParityDomain::BackendIsolationAndPlacement,
            statement: "Propodus tenant database placement (DB name, connection URL, DB role, placement generation) remains strictly hidden from Decapod.".to_string(),
            applies_to_local: false,
            applies_to_cloud: true,
        },
    ];

    let supported_logical_parity_count = operations
        .iter()
        .filter(|op| op.support_class == ParitySupportClass::SupportedLogicalParity)
        .count();
    let supported_cloud_boundary_count = operations
        .iter()
        .filter(|op| op.support_class == ParitySupportClass::SupportedCloudBoundary)
        .count();

    let summary = ParitySummary {
        total_supported_operations: operations.len(),
        supported_logical_parity_count,
        supported_cloud_boundary_count,
        unsupported_cloud_count: unsupported_cloud_operations.len(),
        invariant_count: invariants.len(),
        blanket_parity_claimed: false,
    };

    let placement_boundary = PlacementBoundaryDeclaration {
        exposed_config_enum: vec!["local".to_string(), "cloud".to_string()],
        opaque_server_placement: true,
        forbidden_client_disclosures: vec![
            "tenant_db_name".to_string(),
            "connection_url".to_string(),
            "db_role".to_string(),
            "placement_generation".to_string(),
        ],
        local_db_fallback_permitted: false,
    };

    let mut matrix = CloudParityMatrix {
        schema_version: CLOUD_PARITY_MATRIX_SCHEMA_VERSION.to_string(),
        matrix_version: CLOUD_PARITY_MATRIX_VERSION.to_string(),
        authority_declaration: "Descriptive logical parity matrix only. Does not grant authorization, prove live remote execution, expose tenant placement, or claim blanket local/cloud parity.".to_string(),
        placement_boundary,
        operations,
        unsupported_cloud_operations,
        invariants,
        summary,
        matrix_sha256: String::new(),
    };
    matrix.matrix_sha256 = compute_matrix_sha256(&matrix);
    matrix
}

fn compute_matrix_sha256(matrix: &CloudParityMatrix) -> String {
    let mut normalized = matrix.clone();
    normalized.matrix_sha256.clear();
    let bytes = serde_json::to_vec(&normalized).unwrap_or_default();
    format!("sha256:{:x}", Sha256::digest(&bytes))
}

pub fn cross_check_parity_matrix(matrix: &CloudParityMatrix) -> ParityMatrixCrossCheck {
    let mut seen_ops = BTreeSet::new();
    let mut duplicate_operation_ids = Vec::new();
    let mut supported_ids = BTreeSet::new();

    for entry in &matrix.operations {
        if !seen_ops.insert(entry.operation_id.clone()) {
            duplicate_operation_ids.push(entry.operation_id.clone());
        }
        supported_ids.insert(entry.operation_id.clone());
    }

    let mut unsupported_ids = BTreeSet::new();
    for entry in &matrix.unsupported_cloud_operations {
        if !seen_ops.insert(entry.operation_id.clone()) {
            duplicate_operation_ids.push(entry.operation_id.clone());
        }
        unsupported_ids.insert(entry.operation_id.clone());
    }

    let overlapping_supported_unsupported_ids: Vec<String> = supported_ids
        .intersection(&unsupported_ids)
        .cloned()
        .collect();

    let invariant_ids: BTreeSet<&str> = matrix.invariants.iter().map(|i| i.id.as_str()).collect();
    let mut missing_invariant_links = Vec::new();
    for entry in &matrix.operations {
        for inv in &entry.shared_invariants {
            if !invariant_ids.contains(inv.as_str()) {
                missing_invariant_links.push(format!("{}:{inv}", entry.operation_id));
            }
        }
    }

    let blanket_parity_rejected =
        !matrix.summary.blanket_parity_claimed && !matrix.unsupported_cloud_operations.is_empty();
    let cloud_sqlite_fallback_forbidden = !matrix.placement_boundary.local_db_fallback_permitted
        && matrix
            .unsupported_cloud_operations
            .iter()
            .all(|op| op.never_falls_back_to_sqlite);

    let consistent = duplicate_operation_ids.is_empty()
        && overlapping_supported_unsupported_ids.is_empty()
        && missing_invariant_links.is_empty()
        && blanket_parity_rejected
        && cloud_sqlite_fallback_forbidden
        && matrix.matrix_sha256 == compute_matrix_sha256(matrix);

    ParityMatrixCrossCheck {
        consistent,
        duplicate_operation_ids,
        overlapping_supported_unsupported_ids,
        missing_invariant_links,
        blanket_parity_rejected,
        cloud_sqlite_fallback_forbidden,
    }
}

/// Inspect the parity matrix for `repo_root` without mutating any state,
/// without initializing `.decapod/data/decapod.db`, and without network calls.
pub fn inspect_cloud_parity(repo_root: &Path) -> Result<CloudParityInspectionReport, DecapodError> {
    let config_path = repo_root.join(".decapod").join("config.toml");
    let repository_backend = if config_path.exists() {
        match DecapodProjectConfig::load(repo_root)?
            .repo
            .effective_backend()
        {
            BackendType::Local => "local",
            BackendType::Cloud => "cloud",
        }
    } else {
        "unconfigured"
    }
    .to_string();

    let local_sqlite_path_present = repo_root
        .join(".decapod")
        .join("data")
        .join("decapod.db")
        .exists();
    let matrix = build_parity_matrix();
    let cross_check = cross_check_parity_matrix(&matrix);

    Ok(CloudParityInspectionReport {
        schema_version: CLOUD_PARITY_REPORT_SCHEMA_VERSION.to_string(),
        authority_note: "Descriptive parity matrix only; never grants authorization, never proves live remote execution, and never substitutes for runtime proof.".to_string(),
        repository_backend,
        local_sqlite_path_present,
        local_offline_usable: true,
        blanket_parity_claimed: false,
        matrix,
        cross_check,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn parity_matrix_is_consistent_deterministic_and_never_claims_blanket_parity() {
        let first = build_parity_matrix();
        let second = build_parity_matrix();
        assert_eq!(first, second);
        assert_eq!(first.schema_version, CLOUD_PARITY_MATRIX_SCHEMA_VERSION);
        assert!(!first.summary.blanket_parity_claimed);
        assert!(!first.placement_boundary.local_db_fallback_permitted);
        assert!(first.placement_boundary.opaque_server_placement);
        assert_eq!(
            first.placement_boundary.exposed_config_enum,
            vec!["local".to_string(), "cloud".to_string()]
        );
        let check = cross_check_parity_matrix(&first);
        assert!(check.consistent, "parity matrix check failed: {check:?}");
        assert!(check.blanket_parity_rejected);
        assert!(check.cloud_sqlite_fallback_forbidden);
        assert!(
            first
                .unsupported_cloud_operations
                .iter()
                .any(|op| op.operation_id == "todo.lease.renew")
        );
        assert!(
            first
                .unsupported_cloud_operations
                .iter()
                .any(|op| op.operation_id == "data.database.maintenance")
        );
        assert!(
            first
                .unsupported_cloud_operations
                .iter()
                .any(|op| op.operation_id == "qa.verify.commit")
        );
        for op_id in [
            "knowledge.store",
            "federation.store",
            "decide.store",
            "health.store",
            "aptitude.store",
            "archive.store",
            "feedback.store",
            "cron.store",
            "reflex.store",
            "lcm.store",
            "primitives.store",
            "map.store",
            "policy.store",
            "data.broker",
            "data.compact",
            "store.upsert",
            "store.query",
        ] {
            assert!(
                first
                    .unsupported_cloud_operations
                    .iter()
                    .any(|op| op.operation_id == op_id && op.never_falls_back_to_sqlite),
                "missing fail-closed unsupported cloud boundary entry: {op_id}"
            );
        }
    }

    #[test]
    fn inspect_cloud_parity_is_read_only_and_creates_no_sqlite_db() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".decapod")).unwrap();
        std::fs::write(
            tmp.path().join(".decapod/config.toml"),
            "schema_version = \"1.0.0\"\n[init]\ndiagram_style = \"mermaid\"\nentrypoints = []\n[repo]\nbackend = \"cloud\"\n",
        )
        .unwrap();

        let report = inspect_cloud_parity(tmp.path()).unwrap();
        assert_eq!(report.repository_backend, "cloud");
        assert!(!report.local_sqlite_path_present);
        assert!(!report.blanket_parity_claimed);
        assert!(report.local_offline_usable);
        assert!(report.cross_check.consistent);
        assert!(!tmp.path().join(".decapod/data/decapod.db").exists());
    }
}
