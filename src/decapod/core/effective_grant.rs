//! Deterministic, monotonically narrowing effective operation grants (`#1905`).
//!
//! Computes an ephemeral [`EffectiveOperationGrant`] by intersecting accepted
//! repository policy, server-verified Cloud access scopes, active actor/task
//! custody, live scoped human approvals, parent-to-child delegation bounds, and
//! operation/parameter/path/resource/prestate bindings.
//!
//! No flattened mutable permission list is persisted: every evaluation recomputes
//! the effective grant from canonical authoritative state and rejects attempts
//! to widen via child requests, copied parent grants, candidate branch policy
//! edits, forged approvals, or `PlanState::Approved`.

use crate::cli::{DecapodProjectConfig, GovernanceConfig};
use crate::core::broker::DbBroker;
use crate::core::db::{Connection, OptionalExtension};
use crate::core::db_maintenance::{
    CloudRepositoryAccessStatus, RepoAccessTier, resolve_repository_override,
    resolve_repository_policy,
};
use crate::core::error::DecapodError;
use crate::core::events::{self, append_on_conn, query_all_on_conn};
use crate::core::fleet_coord::{LeaseLifecycle, LeaseState, lease_state, parse_epoch_z};
use crate::core::gatekeeper::GatekeeperConfig;
use crate::core::time::now_epoch_z;
use crate::core::todo::authoring::is_missing_or_unknown_actor;
use crate::core::todo::{COMPONENT_NAMES, todo_db_path};
use crate::core::workspace::{get_main_repo_root, resolve_base_branch};
use crate::plan_governance::{PlanState, load_plan};
use crate::plugins::policy::{
    RiskLevel, RiskMap, actor_trust_level_on_conn, check_approval_on_conn,
    configured_approval_categories_at_project, configured_approval_required, derive_fingerprint,
    eval_risk, exact_target_approval_ids_on_conn, has_live_scoped_approval_on_conn, is_high_risk,
    is_read_only_operation, policy_db_path, required_trust_for_operation, trust_level_to_int,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

pub const EFFECTIVE_GRANT_SCHEMA_VERSION: &str = "decapod.effective-grant.v1";
pub const DELEGATION_POLICY_EVENT_TYPE: &str = "task.delegation.policy";
pub const DELEGATION_RESERVE_EVENT_TYPE: &str = "task.delegation.reserve";

const MAX_DELEGATION_CHAIN_DEPTH: usize = 64;

/// Final deterministic verdict for an evaluated operation request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantDecision {
    Allowed,
    Denied,
    Unproven,
    Stale,
}

impl GrantDecision {
    pub fn is_allowed(self) -> bool {
        matches!(self, Self::Allowed)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allowed => "allowed",
            Self::Denied => "denied",
            Self::Unproven => "unproven",
            Self::Stale => "stale",
        }
    }
}

/// Truthful attribution separating request admission from external or mediated consequence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionOutcomeKind {
    NotAttempted,
    DecapodMediatedApplied {
        executor: String,
    },
    WitnessedExternal {
        witness_executor: String,
        observed: bool,
        verification_gap: Option<String>,
    },
}

/// Paired grant-decision and execution-consequence summary for audit/explanation surfaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationConsequenceRecord {
    pub decision: GrantDecision,
    pub grant_digest: String,
    pub operation: String,
    pub outcome: ExecutionOutcomeKind,
    pub note: String,
}

/// Machine-readable reason explaining why an operation was refused, unproven, or stale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum GrantRejectionReason {
    MissingMandatoryField {
        field: String,
    },
    AmbiguousScopeSpec {
        raw: String,
        detail: String,
    },
    InvalidPathPattern {
        raw: String,
        detail: String,
    },
    ActorSpoofingRejected {
        actor: String,
        detail: String,
    },
    CopiedParentGrantRejected {
        expected_task_id: Option<String>,
        presented_task_id: Option<String>,
        detail: String,
    },
    ForgedOrUnknownApproval {
        detail: String,
    },
    PlanApprovalNotAuthorization {
        detail: String,
    },
    CandidatePolicyWideningRejected {
        detail: String,
    },
    PathNotAllowed {
        path: String,
    },
    PathExplicitlyDenied {
        path: String,
        matched_rule: String,
    },
    OperationDenied {
        operation: String,
    },
    OperationNotPermitted {
        operation: String,
    },
    ResourceMismatch {
        resource: String,
    },
    DelegationRecipientMismatch {
        expected: String,
        actual: String,
    },
    DelegationDepthExceeded {
        remaining: u32,
    },
    DelegationWideningRejected {
        layer_id: String,
        detail: String,
    },
    BudgetExceeded {
        requested: u64,
        limit: u64,
    },
    CumulativeBudgetExceeded {
        parent_task_id: String,
        parent_budget: u64,
        already_reserved: u64,
        requested: u64,
    },
    CustodyNotHeld {
        task_id: String,
        holder: String,
        actor: String,
    },
    LeaseExpired {
        task_id: String,
        expired_at: String,
    },
    LeaseMissingOrUnspecified {
        task_id: String,
    },
    PrestateDrift {
        expected: String,
        actual: String,
    },
    PolicyRevisionMismatch {
        expected: String,
        actual: String,
    },
    HarnessOrSessionMismatch {
        expected: String,
        actual: String,
    },
    RepoIdentityMismatch {
        expected: String,
        actual: String,
    },
    ApprovalRequired {
        operation: String,
        target_fingerprint: String,
        scope: String,
    },
    ProtectedPathApprovalRequired {
        path: String,
    },
    TrustLevelInsufficient {
        actor: String,
        actor_trust: String,
        required_trust: String,
    },
    CloudScopeDenied {
        detail: String,
    },
    AcceptedPolicyUnavailable {
        detail: String,
    },
}

impl GrantRejectionReason {
    /// Classify a rejection into `Denied`, `Unproven`, or `Stale`.
    pub fn decision_class(&self) -> GrantDecision {
        match self {
            Self::PrestateDrift { .. }
            | Self::PolicyRevisionMismatch { .. }
            | Self::LeaseExpired { .. }
            | Self::HarnessOrSessionMismatch { .. }
            | Self::RepoIdentityMismatch { .. } => GrantDecision::Stale,
            Self::ApprovalRequired { .. }
            | Self::ProtectedPathApprovalRequired { .. }
            | Self::AcceptedPolicyUnavailable { .. }
            | Self::LeaseMissingOrUnspecified { .. } => GrantDecision::Unproven,
            _ => GrantDecision::Denied,
        }
    }

    pub fn summary(&self) -> String {
        match self {
            Self::MissingMandatoryField { field } => {
                format!("GRANT_MISSING_FIELD: mandatory field '{field}' is missing or empty")
            }
            Self::AmbiguousScopeSpec { raw, detail } => {
                format!("GRANT_AMBIGUOUS_SCOPE: '{raw}' rejected ({detail})")
            }
            Self::InvalidPathPattern { raw, detail } => {
                format!("GRANT_INVALID_PATH: '{raw}' rejected ({detail})")
            }
            Self::ActorSpoofingRejected { actor, detail } => {
                format!("GRANT_ACTOR_SPOOFING: actor '{actor}' rejected ({detail})")
            }
            Self::CopiedParentGrantRejected {
                expected_task_id,
                presented_task_id,
                detail,
            } => format!(
                "GRANT_COPIED_PARENT_REJECTED: presented task {:?} cannot authorize {:?} ({detail})",
                presented_task_id, expected_task_id
            ),
            Self::ForgedOrUnknownApproval { detail } => {
                format!("GRANT_FORGED_APPROVAL: {detail}")
            }
            Self::PlanApprovalNotAuthorization { detail } => {
                format!("GRANT_PLAN_APPROVAL_NOT_PERMISSION: {detail}")
            }
            Self::CandidatePolicyWideningRejected { detail } => {
                format!("GRANT_CANDIDATE_POLICY_WIDENING: {detail}")
            }
            Self::PathNotAllowed { path } => {
                format!("GRANT_PATH_NOT_ALLOWED: path '{path}' is outside effective scope")
            }
            Self::PathExplicitlyDenied { path, matched_rule } => {
                format!("GRANT_PATH_DENIED: path '{path}' denied by rule '{matched_rule}'")
            }
            Self::OperationDenied { operation } => {
                format!("GRANT_OPERATION_DENIED: operation '{operation}' is explicitly denied")
            }
            Self::OperationNotPermitted { operation } => {
                format!(
                    "GRANT_OPERATION_NOT_PERMITTED: operation '{operation}' is outside effective operations"
                )
            }
            Self::ResourceMismatch { resource } => {
                format!(
                    "GRANT_RESOURCE_MISMATCH: resource '{resource}' is outside effective resource scope"
                )
            }
            Self::DelegationRecipientMismatch { expected, actual } => format!(
                "GRANT_RECIPIENT_MISMATCH: delegation bound to '{expected}', got '{actual}'"
            ),
            Self::DelegationDepthExceeded { remaining } => {
                format!(
                    "GRANT_DELEGATION_DEPTH_EXCEEDED: remaining delegation depth is {remaining}"
                )
            }
            Self::DelegationWideningRejected { layer_id, detail } => format!(
                "GRANT_DELEGATION_WIDENING: layer '{layer_id}' attempted to widen parent authority ({detail})"
            ),
            Self::BudgetExceeded { requested, limit } => {
                format!("GRANT_BUDGET_EXCEEDED: requested budget {requested} exceeds limit {limit}")
            }
            Self::CumulativeBudgetExceeded {
                parent_task_id,
                parent_budget,
                already_reserved,
                requested,
            } => format!(
                "CUMULATIVE_DELEGATION_BUDGET_EXCEEDED: parent '{parent_task_id}' capacity {parent_budget} already has {already_reserved} reserved/consumed; cannot reserve additional {requested}"
            ),
            Self::CustodyNotHeld {
                task_id,
                holder,
                actor,
            } => format!(
                "GRANT_CUSTODY_NOT_HELD: task '{task_id}' is held by '{holder}', not '{actor}'"
            ),
            Self::LeaseExpired {
                task_id,
                expired_at,
            } => format!("GRANT_LEASE_EXPIRED: lease for task '{task_id}' expired at {expired_at}"),
            Self::LeaseMissingOrUnspecified { task_id } => {
                format!("GRANT_LEASE_UNSPECIFIED: task '{task_id}' has no active exclusive lease")
            }
            Self::PrestateDrift { expected, actual } => format!(
                "GRANT_PRESTATE_DRIFT: expected prestate '{expected}', current state is '{actual}'"
            ),
            Self::PolicyRevisionMismatch { expected, actual } => format!(
                "GRANT_POLICY_REVISION_STALE: expected '{expected}', accepted policy is '{actual}'"
            ),
            Self::HarnessOrSessionMismatch { expected, actual } => {
                format!("GRANT_HARNESS_SESSION_MISMATCH: expected '{expected}', active '{actual}'")
            }
            Self::RepoIdentityMismatch { expected, actual } => {
                format!("GRANT_REPO_MISMATCH: expected repo '{expected}', current '{actual}'")
            }
            Self::ApprovalRequired {
                operation,
                target_fingerprint,
                scope,
            } => format!(
                "APPROVAL REQUIRED: operation '{operation}' requires live scoped approval matching '{target_fingerprint}' ({scope})"
            ),
            Self::ProtectedPathApprovalRequired { path } => format!(
                "GRANT_PROTECTED_PATH_APPROVAL_REQUIRED: protected path '{path}' requires scoped human approval"
            ),
            Self::TrustLevelInsufficient {
                actor,
                actor_trust,
                required_trust,
            } => format!(
                "GRANT_TRUST_INSUFFICIENT: actor '{actor}' trust '{actor_trust}' < required '{required_trust}'"
            ),
            Self::CloudScopeDenied { detail } => {
                format!("GRANT_CLOUD_SCOPE_DENIED: {detail}")
            }
            Self::AcceptedPolicyUnavailable { detail } => {
                format!("GRANT_ACCEPTED_POLICY_UNAVAILABLE: {detail}")
            }
        }
    }
}

/// Canonical normalized path or glob pattern supporting exact set intersection and subsumption.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum PathPattern {
    /// Matches every repository-relative path (`**`).
    RepoWide,
    /// Matches any path strictly beneath the normalized directory prefix (`src/**`, `src/api/**`).
    DirPrefix(Vec<String>),
    /// Matches immediate children of the directory (`*`, `src/*`).
    DirImmediate(Vec<String>),
    /// Matches one exact repository-relative file or directory path (`src/api/routes.rs`).
    Exact(Vec<String>),
}

impl PathPattern {
    /// Parse a single path or glob pattern strictly without silent fallback.
    pub fn parse(raw: &str) -> Result<Self, GrantRejectionReason> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(GrantRejectionReason::InvalidPathPattern {
                raw: raw.to_string(),
                detail: "pattern cannot be empty".into(),
            });
        }
        if trimmed.contains('\0')
            || trimmed.contains('\\')
            || trimmed.contains('?')
            || trimmed.contains('[')
            || trimmed.contains(']')
            || trimmed.contains('{')
            || trimmed.contains('}')
            || trimmed.contains(':')
        {
            return Err(GrantRejectionReason::InvalidPathPattern {
                raw: raw.to_string(),
                detail: "contains forbidden or ambiguous path/glob character".into(),
            });
        }
        if trimmed.starts_with('/') {
            return Err(GrantRejectionReason::InvalidPathPattern {
                raw: raw.to_string(),
                detail: "pattern must be repository-relative, not absolute".into(),
            });
        }
        if trimmed == "**" {
            return Ok(Self::RepoWide);
        }
        if trimmed == "*" {
            return Ok(Self::DirImmediate(Vec::new()));
        }

        let trailing_slash = trimmed.ends_with('/');
        let without_trailing = trimmed.trim_end_matches('/');
        if without_trailing.is_empty() {
            return Err(GrantRejectionReason::InvalidPathPattern {
                raw: raw.to_string(),
                detail: "root slash alone is not a valid repository path".into(),
            });
        }

        let raw_segments: Vec<&str> = without_trailing.split('/').collect();
        let mut clean_segments = Vec::with_capacity(raw_segments.len());

        for (idx, seg) in raw_segments.iter().enumerate() {
            let is_last = idx + 1 == raw_segments.len();
            if seg.is_empty() || *seg == "." || *seg == ".." {
                return Err(GrantRejectionReason::InvalidPathPattern {
                    raw: raw.to_string(),
                    detail: "empty, '.', or '..' segment is not allowed".into(),
                });
            }
            if seg.contains('*') {
                if !is_last || trailing_slash {
                    return Err(GrantRejectionReason::InvalidPathPattern {
                        raw: raw.to_string(),
                        detail: "wildcards are only permitted as the final path segment".into(),
                    });
                }
                return match *seg {
                    "**" => Ok(Self::DirPrefix(clean_segments)),
                    "*" => Ok(Self::DirImmediate(clean_segments)),
                    _ => Err(GrantRejectionReason::InvalidPathPattern {
                        raw: raw.to_string(),
                        detail: "partial-segment or multi-star globs are ambiguous and rejected"
                            .into(),
                    }),
                };
            }
            clean_segments.push((*seg).to_string());
        }

        if trailing_slash {
            Ok(Self::DirPrefix(clean_segments))
        } else {
            Ok(Self::Exact(clean_segments))
        }
    }

    /// Normalize a concrete non-glob repository-relative path into path segments.
    pub fn normalize_concrete_path(raw: &str) -> Result<Vec<String>, GrantRejectionReason> {
        let parsed = Self::parse(raw)?;
        match parsed {
            Self::Exact(segments) => Ok(segments),
            _ => Err(GrantRejectionReason::InvalidPathPattern {
                raw: raw.to_string(),
                detail: "expected a concrete repository path without wildcards or trailing slash"
                    .into(),
            }),
        }
    }

    pub fn canonical_string(&self) -> String {
        match self {
            Self::RepoWide => "**".to_string(),
            Self::DirPrefix(segs) => {
                if segs.is_empty() {
                    "**".to_string()
                } else {
                    format!("{}/**", segs.join("/"))
                }
            }
            Self::DirImmediate(segs) => {
                if segs.is_empty() {
                    "*".to_string()
                } else {
                    format!("{}/*", segs.join("/"))
                }
            }
            Self::Exact(segs) => segs.join("/"),
        }
    }

    /// Return true when this pattern matches the concrete normalized path segments.
    pub fn matches_segments(&self, path: &[String]) -> bool {
        if path.is_empty() {
            return matches!(self, Self::RepoWide);
        }
        match self {
            Self::RepoWide => true,
            Self::DirPrefix(prefix) => path.len() > prefix.len() && path.starts_with(prefix),
            Self::DirImmediate(dir) => path.len() == dir.len() + 1 && path.starts_with(dir),
            Self::Exact(exact) => path == exact.as_slice(),
        }
    }

    /// Return true when every concrete path matched by `other` is also matched by `self`.
    pub fn subsumes(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::RepoWide, _) => true,
            (_, Self::RepoWide) => false,
            (Self::DirPrefix(a), Self::DirPrefix(b)) => b.starts_with(a),
            (Self::DirPrefix(a), Self::DirImmediate(b)) => b.starts_with(a),
            (Self::DirPrefix(a), Self::Exact(b)) => b.len() > a.len() && b.starts_with(a),
            (Self::DirImmediate(_), Self::DirPrefix(_)) => false,
            (Self::DirImmediate(a), Self::DirImmediate(b)) => a == b,
            (Self::DirImmediate(a), Self::Exact(b)) => b.len() == a.len() + 1 && b.starts_with(a),
            (Self::Exact(a), Self::Exact(b)) => a == b,
            (Self::Exact(_), _) => false,
        }
    }

    /// Commutative, associative canonical intersection of two path patterns.
    pub fn intersect(&self, other: &Self) -> Option<Self> {
        match (self, other) {
            (Self::RepoWide, p) | (p, Self::RepoWide) => Some(p.clone()),
            (Self::DirPrefix(a), Self::DirPrefix(b)) => {
                if a.starts_with(b) {
                    Some(Self::DirPrefix(a.clone()))
                } else if b.starts_with(a) {
                    Some(Self::DirPrefix(b.clone()))
                } else {
                    None
                }
            }
            (Self::DirPrefix(p), Self::DirImmediate(d))
            | (Self::DirImmediate(d), Self::DirPrefix(p)) => {
                if d.starts_with(p) {
                    Some(Self::DirImmediate(d.clone()))
                } else {
                    None
                }
            }
            (Self::DirPrefix(p), Self::Exact(e)) | (Self::Exact(e), Self::DirPrefix(p)) => {
                if e.len() > p.len() && e.starts_with(p) {
                    Some(Self::Exact(e.clone()))
                } else {
                    None
                }
            }
            (Self::DirImmediate(a), Self::DirImmediate(b)) => {
                if a == b {
                    Some(Self::DirImmediate(a.clone()))
                } else {
                    None
                }
            }
            (Self::DirImmediate(d), Self::Exact(e)) | (Self::Exact(e), Self::DirImmediate(d)) => {
                if e.len() == d.len() + 1 && e.starts_with(d) {
                    Some(Self::Exact(e.clone()))
                } else {
                    None
                }
            }
            (Self::Exact(a), Self::Exact(b)) => {
                if a == b {
                    Some(Self::Exact(a.clone()))
                } else {
                    None
                }
            }
        }
    }
}

/// Canonical allowance set formed by monotonic intersection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScopeAllowanceSet {
    /// No path restriction imposed at this layer (`**`).
    Unconstrained,
    /// Only concrete paths matched by at least one pattern in the set are allowed.
    /// An empty `BTreeSet` allows zero paths.
    Restricted(BTreeSet<PathPattern>),
}

impl ScopeAllowanceSet {
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Restricted(set) if set.is_empty())
    }

    pub fn matches_segments(&self, segments: &[String]) -> bool {
        match self {
            Self::Unconstrained => true,
            Self::Restricted(patterns) => patterns.iter().any(|pat| pat.matches_segments(segments)),
        }
    }

    pub fn subsumes_pattern(&self, pat: &PathPattern) -> bool {
        match self {
            Self::Unconstrained => true,
            Self::Restricted(patterns) => patterns.iter().any(|outer| outer.subsumes(pat)),
        }
    }

    /// Commutative, associative, idempotent intersection of two allowance sets.
    pub fn intersect(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::Unconstrained, r) | (r, Self::Unconstrained) => r.clone(),
            (Self::Restricted(a), Self::Restricted(b)) => {
                let mut raw = BTreeSet::new();
                for pa in a {
                    for pb in b {
                        if let Some(inter) = pa.intersect(pb) {
                            raw.insert(inter);
                        }
                    }
                }
                Self::Restricted(reduce_subsumed_patterns(raw))
            }
        }
    }

    pub fn from_patterns(patterns: impl IntoIterator<Item = PathPattern>) -> Self {
        let mut set = BTreeSet::new();
        for pat in patterns {
            if matches!(pat, PathPattern::RepoWide) {
                return Self::Unconstrained;
            }
            set.insert(pat);
        }
        Self::Restricted(reduce_subsumed_patterns(set))
    }

    pub fn canonical_strings(&self) -> Vec<String> {
        match self {
            Self::Unconstrained => vec!["**".to_string()],
            Self::Restricted(set) => set.iter().map(PathPattern::canonical_string).collect(),
        }
    }
}

/// Remove patterns that are already subsumed by another pattern in the same union set.
pub fn reduce_subsumed_patterns(patterns: BTreeSet<PathPattern>) -> BTreeSet<PathPattern> {
    let list: Vec<PathPattern> = patterns.into_iter().collect();
    let mut kept = BTreeSet::new();
    for (i, candidate) in list.iter().enumerate() {
        let covered_by_other = list
            .iter()
            .enumerate()
            .any(|(j, other)| i != j && other.subsumes(candidate));
        if !covered_by_other {
            kept.insert(candidate.clone());
        }
    }
    kept
}

/// Parsed structured or legacy task/delegation scope declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedScopeSpec {
    pub allowances: ScopeAllowanceSet,
    pub denials: BTreeSet<PathPattern>,
    pub allowed_operations: Option<BTreeSet<String>>,
    pub allowed_resources: Option<BTreeSet<String>>,
    pub max_budget_units: Option<u64>,
    pub max_delegation_depth: Option<u32>,
    pub expected_recipient: Option<String>,
    #[serde(default)]
    pub is_advisory_paths: bool,
}

impl Default for ParsedScopeSpec {
    fn default() -> Self {
        Self {
            allowances: ScopeAllowanceSet::Unconstrained,
            denials: BTreeSet::new(),
            allowed_operations: None,
            allowed_resources: None,
            max_budget_units: None,
            max_delegation_depth: None,
            expected_recipient: None,
            is_advisory_paths: false,
        }
    }
}

fn is_advisory_scope_label(trimmed: &str) -> bool {
    !trimmed.is_empty()
        && !trimmed.starts_with('.')
        && !trimmed.starts_with('-')
        && trimmed
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Strictly parse task `--scope` / delegation scope descriptors without guessing on ambiguity.
pub fn parse_scope_spec(raw: &str) -> Result<ParsedScopeSpec, GrantRejectionReason> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("root")
        || trimmed.eq_ignore_ascii_case("workspace")
        || COMPONENT_NAMES.contains(&trimmed)
        || is_advisory_scope_label(trimmed)
    {
        return Ok(ParsedScopeSpec::default());
    }

    // Check unbalanced quotes or brackets globally before splitting clauses.
    validate_balanced_delimiters(trimmed, raw)?;

    if !trimmed.contains(':') {
        // Legacy comma/whitespace-free single pattern vs ambiguous mixed bare string.
        if trimmed.contains(['[', ']', '"', ';', ' ', '\t', '\n', '\r']) {
            return Err(GrantRejectionReason::AmbiguousScopeSpec {
                raw: raw.to_string(),
                detail: "unkeyed scope with brackets/quotes/separators is ambiguous".into(),
            });
        }
        if trimmed.contains(',') {
            let mut pats = Vec::new();
            for item in trimmed.split(',') {
                let item = item.trim();
                if item.is_empty() {
                    return Err(GrantRejectionReason::AmbiguousScopeSpec {
                        raw: raw.to_string(),
                        detail: "empty item in comma-separated scope list".into(),
                    });
                }
                pats.push(PathPattern::parse(item)?);
            }
            return Ok(ParsedScopeSpec {
                allowances: ScopeAllowanceSet::from_patterns(pats),
                ..ParsedScopeSpec::default()
            });
        }
        let pat = PathPattern::parse(trimmed)?;
        return Ok(ParsedScopeSpec {
            allowances: ScopeAllowanceSet::from_patterns([pat]),
            ..ParsedScopeSpec::default()
        });
    }

    let clauses = split_top_level_clauses(trimmed, raw)?;
    if clauses.is_empty() {
        return Err(GrantRejectionReason::AmbiguousScopeSpec {
            raw: raw.to_string(),
            detail: "structured scope has no clauses".into(),
        });
    }

    let mut parsed = ParsedScopeSpec::default();
    let mut seen_keys = BTreeSet::new();

    for clause in clauses {
        let Some((key_raw, val_raw)) = clause.split_once(':') else {
            return Err(GrantRejectionReason::AmbiguousScopeSpec {
                raw: raw.to_string(),
                detail: format!("clause '{clause}' lacks explicit key:value form"),
            });
        };
        let key = key_raw.trim().to_ascii_lowercase();
        let val = val_raw.trim();
        if val.is_empty() {
            return Err(GrantRejectionReason::AmbiguousScopeSpec {
                raw: raw.to_string(),
                detail: format!("clause '{key}' has empty value"),
            });
        }
        if !seen_keys.insert(key.clone()) {
            return Err(GrantRejectionReason::AmbiguousScopeSpec {
                raw: raw.to_string(),
                detail: format!("duplicate scope key '{key}' is ambiguous"),
            });
        }

        match key.as_str() {
            "paths" | "path" | "allow" => {
                let items = parse_bracket_or_scalar_list(val, raw)?;
                let mut pats = Vec::with_capacity(items.len());
                for item in items {
                    pats.push(PathPattern::parse(&item)?);
                }
                parsed.allowances = ScopeAllowanceSet::from_patterns(pats);
            }
            "deny" | "denied" => {
                let items = parse_bracket_or_scalar_list(val, raw)?;
                if items.is_empty() {
                    return Err(GrantRejectionReason::AmbiguousScopeSpec {
                        raw: raw.to_string(),
                        detail: "deny:[] cannot be empty".into(),
                    });
                }
                for item in items {
                    parsed.denials.insert(PathPattern::parse(&item)?);
                }
                parsed.denials = reduce_subsumed_patterns(parsed.denials);
            }
            "ops" | "operations" => {
                let items = parse_bracket_or_scalar_list(val, raw)?;
                parsed.allowed_operations = Some(items.into_iter().collect());
            }
            "resource" | "resources" => {
                let items = parse_bracket_or_scalar_list(val, raw)?;
                parsed.allowed_resources = Some(items.into_iter().collect());
            }
            "budget" => {
                let units =
                    val.parse::<u64>()
                        .map_err(|_| GrantRejectionReason::AmbiguousScopeSpec {
                            raw: raw.to_string(),
                            detail: format!("invalid budget integer '{val}'"),
                        })?;
                parsed.max_budget_units = Some(units);
            }
            "depth" => {
                let depth =
                    val.parse::<u32>()
                        .map_err(|_| GrantRejectionReason::AmbiguousScopeSpec {
                            raw: raw.to_string(),
                            detail: format!("invalid depth integer '{val}'"),
                        })?;
                parsed.max_delegation_depth = Some(depth);
            }
            "recipient" => {
                if val.contains([' ', ',', ';', '[', ']']) || is_missing_or_unknown_actor(val) {
                    return Err(GrantRejectionReason::AmbiguousScopeSpec {
                        raw: raw.to_string(),
                        detail: format!("invalid recipient '{val}'"),
                    });
                }
                parsed.expected_recipient = Some(val.to_string());
            }
            _ => {
                return Err(GrantRejectionReason::AmbiguousScopeSpec {
                    raw: raw.to_string(),
                    detail: format!("unsupported scope key '{key}'"),
                });
            }
        }
    }

    parsed.is_advisory_paths = seen_keys.len() == 1 && seen_keys.contains("paths");
    Ok(parsed)
}

fn validate_balanced_delimiters(s: &str, raw: &str) -> Result<(), GrantRejectionReason> {
    let mut bracket_depth: usize = 0;
    let mut in_quotes = false;
    for ch in s.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            '[' if !in_quotes => bracket_depth += 1,
            ']' if !in_quotes => {
                if bracket_depth == 0 {
                    return Err(GrantRejectionReason::AmbiguousScopeSpec {
                        raw: raw.to_string(),
                        detail: "unmatched closing ']'".into(),
                    });
                }
                bracket_depth -= 1;
            }
            _ => {}
        }
    }
    if in_quotes || bracket_depth != 0 {
        return Err(GrantRejectionReason::AmbiguousScopeSpec {
            raw: raw.to_string(),
            detail: "unterminated quote or bracket in scope spec".into(),
        });
    }
    Ok(())
}

fn split_top_level_clauses(s: &str, raw: &str) -> Result<Vec<String>, GrantRejectionReason> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    let mut in_quotes = false;

    for ch in s.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                current.push(ch);
            }
            '[' if !in_quotes => {
                depth += 1;
                current.push(ch);
            }
            ']' if !in_quotes => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ';' if !in_quotes && depth == 0 => {
                let piece = current.trim();
                if piece.is_empty() {
                    return Err(GrantRejectionReason::AmbiguousScopeSpec {
                        raw: raw.to_string(),
                        detail: "empty ';' clause".into(),
                    });
                }
                out.push(piece.to_string());
                current.clear();
            }
            ',' if !in_quotes && depth == 0 => {
                let piece = current.trim();
                if piece.is_empty() {
                    return Err(GrantRejectionReason::AmbiguousScopeSpec {
                        raw: raw.to_string(),
                        detail: "empty ',' clause".into(),
                    });
                }
                out.push(piece.to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    let tail = current.trim();
    if !tail.is_empty() {
        out.push(tail.to_string());
    }
    Ok(out)
}

fn parse_bracket_or_scalar_list(val: &str, raw: &str) -> Result<Vec<String>, GrantRejectionReason> {
    let trimmed = val.trim();
    if let Some(inner) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let inner = inner.trim();
        if inner.is_empty() {
            return Ok(Vec::new());
        }
        let mut items = Vec::new();
        for part in inner.split(',') {
            let clean = part.trim().trim_matches('"').trim();
            if clean.is_empty() {
                return Err(GrantRejectionReason::AmbiguousScopeSpec {
                    raw: raw.to_string(),
                    detail: "empty entry inside bracketed scope list".into(),
                });
            }
            items.push(clean.to_string());
        }
        return Ok(items);
    }
    if trimmed.contains(['[', ']']) {
        return Err(GrantRejectionReason::AmbiguousScopeSpec {
            raw: raw.to_string(),
            detail: format!("malformed list syntax '{trimmed}'"),
        });
    }
    let clean = trimmed.trim_matches('"').trim();
    if clean.is_empty() {
        return Err(GrantRejectionReason::AmbiguousScopeSpec {
            raw: raw.to_string(),
            detail: "empty quoted value".into(),
        });
    }
    Ok(vec![clean.to_string()])
}

/// Verified server-issued Cloud authorization tier and optional path/resource bounds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedCloudScope {
    pub repo_id: String,
    pub access_tier: RepoAccessTier,
    pub max_delegated_tier: RepoAccessTier,
    pub seat_authorized: bool,
    pub policy_bound: bool,
    pub allowed_paths: Option<Vec<String>>,
    pub denied_paths: Vec<String>,
    pub allowed_operations: Option<Vec<String>>,
    pub server_epoch_s: Option<u64>,
    pub expires_at_epoch_s: Option<u64>,
}

impl VerifiedCloudScope {
    pub fn from_access_status(status: &CloudRepositoryAccessStatus) -> Self {
        Self {
            repo_id: status.repository.clone(),
            access_tier: status.effective_tier,
            max_delegated_tier: status.max_delegated_tier,
            seat_authorized: status.seat_authorized,
            policy_bound: status.policy_bound,
            allowed_paths: None,
            denied_paths: Vec::new(),
            allowed_operations: None,
            server_epoch_s: None,
            expires_at_epoch_s: None,
        }
    }

    pub fn into_layer(&self) -> Result<DelegationLayer, GrantRejectionReason> {
        let mut layer = DelegationLayer::new(format!("cloud_scope:{}", self.repo_id));
        if !self.seat_authorized || !self.policy_bound {
            return Err(GrantRejectionReason::CloudScopeDenied {
                detail: format!(
                    "verified cloud access for '{}' lacks active seat authorization or bound policy",
                    self.repo_id
                ),
            });
        }
        let effective = match (self.access_tier, self.max_delegated_tier) {
            (RepoAccessTier::Read, _) | (_, RepoAccessTier::Read) => RepoAccessTier::Read,
            (RepoAccessTier::Write, _) | (_, RepoAccessTier::Write) => RepoAccessTier::Write,
            (RepoAccessTier::Owner, RepoAccessTier::Owner) => RepoAccessTier::Owner,
        };
        if matches!(effective, RepoAccessTier::Read) {
            // Read-only tier cannot permit any mutating operations.
            layer.allowed_operations = Some(BTreeSet::from(["policy.grant.explain".to_string()]));
        }
        if let Some(ops) = &self.allowed_operations {
            let parsed: BTreeSet<String> = ops.iter().cloned().collect();
            layer.allowed_operations = Some(match layer.allowed_operations {
                Some(existing) => existing.intersection(&parsed).cloned().collect(),
                None => parsed,
            });
        }
        if let Some(paths) = &self.allowed_paths {
            let mut pats = Vec::with_capacity(paths.len());
            for p in paths {
                pats.push(PathPattern::parse(p)?);
            }
            layer.allowances = ScopeAllowanceSet::from_patterns(pats);
        }
        for p in &self.denied_paths {
            layer.denials.insert(PathPattern::parse(p)?);
        }
        layer.denials = reduce_subsumed_patterns(layer.denials);
        layer.expires_at_epoch_s = self.expires_at_epoch_s;
        layer.allowed_resources = Some(BTreeSet::from([format!("repo:{}", self.repo_id)]));
        Ok(layer)
    }
}

/// One deterministic narrowing layer in the authority stack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegationLayer {
    pub layer_id: String,
    pub allowances: ScopeAllowanceSet,
    pub denials: BTreeSet<PathPattern>,
    pub approval_required_paths: BTreeSet<PathPattern>,
    pub allowed_operations: Option<BTreeSet<String>>,
    pub denied_operations: BTreeSet<String>,
    pub allowed_resources: Option<BTreeSet<String>>,
    pub denied_resources: BTreeSet<String>,
    pub max_budget_units: Option<u64>,
    pub max_delegation_depth: Option<u32>,
    pub expected_recipient: Option<String>,
    pub expires_at_epoch_s: Option<u64>,
    pub bound_repo_fingerprint: Option<String>,
    pub bound_policy_revision: Option<String>,
    pub bound_prestate: Option<String>,
}

impl DelegationLayer {
    pub fn new(layer_id: impl Into<String>) -> Self {
        Self {
            layer_id: layer_id.into(),
            allowances: ScopeAllowanceSet::Unconstrained,
            denials: BTreeSet::new(),
            approval_required_paths: BTreeSet::new(),
            allowed_operations: None,
            denied_operations: BTreeSet::new(),
            allowed_resources: None,
            denied_resources: BTreeSet::new(),
            max_budget_units: None,
            max_delegation_depth: None,
            expected_recipient: None,
            expires_at_epoch_s: None,
            bound_repo_fingerprint: None,
            bound_policy_revision: None,
            bound_prestate: None,
        }
    }

    pub fn from_parsed_scope(layer_id: impl Into<String>, scope: ParsedScopeSpec) -> Self {
        Self {
            layer_id: layer_id.into(),
            allowances: scope.allowances,
            denials: scope.denials,
            approval_required_paths: BTreeSet::new(),
            allowed_operations: scope.allowed_operations,
            denied_operations: BTreeSet::new(),
            allowed_resources: scope.allowed_resources,
            denied_resources: BTreeSet::new(),
            max_budget_units: scope.max_budget_units,
            max_delegation_depth: scope.max_delegation_depth,
            expected_recipient: scope.expected_recipient,
            expires_at_epoch_s: None,
            bound_repo_fingerprint: None,
            bound_policy_revision: None,
            bound_prestate: None,
        }
    }
}

/// Summary of one contributing layer exposed in bounded read-only explanations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrantLayerSummary {
    pub layer_id: String,
    pub allowances: Vec<String>,
    pub denials: Vec<String>,
    pub max_budget_units: Option<u64>,
    pub max_delegation_depth: Option<u32>,
}

/// Persisted parent-child delegation policy stored in `events::TODO`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegationPolicyRecord {
    pub parent_task_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_task_id: Option<String>,
    pub grantor_actor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipient_actor: Option<String>,
    #[serde(default)]
    pub allowed_paths: Vec<String>,
    #[serde(default)]
    pub denied_paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_operations: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_total_budget_units: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_child_budget_units: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_delegation_depth: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_scope: Option<String>,
    pub policy_revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_epoch_s: Option<u64>,
}

impl DelegationPolicyRecord {
    pub fn to_layer(&self, layer_id: String) -> Result<DelegationLayer, GrantRejectionReason> {
        let mut layer = DelegationLayer::new(layer_id);
        if !self.allowed_paths.is_empty() {
            let mut pats = Vec::with_capacity(self.allowed_paths.len());
            for raw in &self.allowed_paths {
                pats.push(PathPattern::parse(raw)?);
            }
            layer.allowances = ScopeAllowanceSet::from_patterns(pats);
        }
        for raw in &self.denied_paths {
            layer.denials.insert(PathPattern::parse(raw)?);
        }
        layer.denials = reduce_subsumed_patterns(layer.denials);
        layer.allowed_operations = self
            .allowed_operations
            .as_ref()
            .map(|ops| ops.iter().cloned().collect());
        if let Some(res) = &self.resource_scope {
            layer.allowed_resources = Some(BTreeSet::from([res.clone()]));
        }
        layer.max_budget_units = match (self.max_total_budget_units, self.max_child_budget_units) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        layer.max_delegation_depth = self.max_delegation_depth;
        layer.expected_recipient = self.recipient_actor.clone();
        layer.expires_at_epoch_s = self.expires_at_epoch_s;
        layer.bound_policy_revision = if self.policy_revision.is_empty() {
            None
        } else {
            Some(self.policy_revision.clone())
        };
        Ok(layer)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReservationStatus {
    Reserved,
    Consumed,
    Released,
}

/// Persisted child reservation against a shared parent budget in `events::TODO`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegationReservationRecord {
    pub reservation_id: String,
    pub parent_task_id: String,
    pub child_task_id: String,
    pub actor_id: String,
    pub units: u64,
    pub generation: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
    pub status: ReservationStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CumulativeBudgetSnapshot {
    pub parent_task_id: String,
    pub parent_total_budget: u64,
    pub reserved_or_consumed_units: u64,
    pub remaining_units: u64,
}

/// Cryptographically bound repository/task/policy state at grant formation time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrantPrestateBinding {
    pub repo_fingerprint: String,
    pub accepted_base_ref: String,
    pub accepted_base_oid: String,
    pub head_oid: String,
    pub policy_revision: String,
    pub working_tree_digest: String,
    pub task_id: Option<String>,
    pub task_revision: Option<i64>,
    pub lease_generation: Option<i64>,
    pub lease_expires_at: Option<String>,
    pub session_id: Option<String>,
    pub harness_id: Option<String>,
    pub prestate_digest: String,
}

impl GrantPrestateBinding {
    pub fn compute_digest(&mut self) {
        let canonical = serde_json::json!({
            "repo_fingerprint": self.repo_fingerprint,
            "accepted_base_ref": self.accepted_base_ref,
            "accepted_base_oid": self.accepted_base_oid,
            "head_oid": self.head_oid,
            "policy_revision": self.policy_revision,
            "working_tree_digest": self.working_tree_digest,
            "task_id": self.task_id,
            "task_revision": self.task_revision,
            "lease_generation": self.lease_generation,
            "lease_expires_at": self.lease_expires_at,
            "session_id": self.session_id,
            "harness_id": self.harness_id,
        });
        self.prestate_digest = sha256_hex(canonical.to_string().as_bytes());
    }
}

/// Complete, deterministically formed effective operation grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveOperationGrant {
    pub schema_version: String,
    pub grant_digest: String,
    pub decision: GrantDecision,
    pub reasons: Vec<GrantRejectionReason>,
    pub summary: String,
    pub operation: String,
    pub actor_id: String,
    pub task_id: Option<String>,
    pub target_path: Option<String>,
    pub target_resource: Option<String>,
    pub recipient: Option<String>,
    pub requested_budget_units: Option<u64>,
    pub effective_allowances: Vec<String>,
    pub effective_denials: Vec<String>,
    pub approval_required_paths: Vec<String>,
    pub effective_allowed_operations: Option<Vec<String>>,
    pub effective_denied_operations: Vec<String>,
    pub effective_allowed_resources: Option<Vec<String>>,
    pub effective_max_budget_units: Option<u64>,
    pub remaining_delegation_depth: Option<u32>,
    pub cumulative_budget: Option<CumulativeBudgetSnapshot>,
    pub approval_required: bool,
    pub approval_ids_matched: Vec<String>,
    pub target_fingerprint: String,
    pub prestate: GrantPrestateBinding,
    pub layers: Vec<GrantLayerSummary>,
    pub execution_outcome: ExecutionOutcomeKind,
}

impl EffectiveOperationGrant {
    /// Bounded machine/human rendering without leaking secrets.
    pub fn render_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "decision={} operation={} actor={} task={}\n",
            self.decision.as_str(),
            self.operation,
            self.actor_id,
            self.task_id.as_deref().unwrap_or("<none>")
        ));
        out.push_str(&format!(
            "policy_revision={} base_oid={} head_oid={} prestate={}\n",
            self.prestate.policy_revision,
            self.prestate.accepted_base_oid,
            self.prestate.head_oid,
            &self.prestate.prestate_digest[..16.min(self.prestate.prestate_digest.len())]
        ));
        out.push_str(&format!(
            "allowances=[{}] denials=[{}] max_budget={:?} remaining_depth={:?}\n",
            self.effective_allowances.join(", "),
            self.effective_denials.join(", "),
            self.effective_max_budget_units,
            self.remaining_delegation_depth
        ));
        if let Some(cum) = &self.cumulative_budget {
            out.push_str(&format!(
                "cumulative_budget parent={} total={} used={} remaining={}\n",
                cum.parent_task_id,
                cum.parent_total_budget,
                cum.reserved_or_consumed_units,
                cum.remaining_units
            ));
        }
        out.push_str(&format!("summary={}\n", self.summary));
        for reason in &self.reasons {
            out.push_str(&format!("reason={}\n", reason.summary()));
        }
        out
    }

    pub fn first_error(&self) -> DecapodError {
        let msg = if self.reasons.is_empty() {
            self.summary.clone()
        } else {
            self.reasons
                .iter()
                .map(GrantRejectionReason::summary)
                .collect::<Vec<_>>()
                .join("; ")
        };
        DecapodError::ValidationError(msg)
    }
}

/// Request context for evaluating an [`EffectiveOperationGrant`].
#[derive(Debug, Clone)]
pub struct GrantEvaluationRequest {
    pub operation: String,
    pub actor_id: String,
    pub task_id: Option<String>,
    pub target_path: Option<String>,
    pub additional_paths: Vec<String>,
    pub target_resource: Option<String>,
    pub recipient: Option<String>,
    pub requested_budget_units: Option<u64>,
    pub require_exclusive_custody: bool,
    pub expected_prestate_digest: Option<String>,
    pub expected_policy_revision: Option<String>,
    pub expected_harness_id: Option<String>,
    pub expected_session_id: Option<String>,
    pub expected_repo_fingerprint: Option<String>,
    pub presented_parent_grant_task_id: Option<String>,
    pub presented_approval_token: Option<String>,
    pub treat_plan_approval_as_permission: bool,
    pub verified_cloud_scope: Option<VerifiedCloudScope>,
    pub extra_layers: Vec<DelegationLayer>,
    pub now_epoch_s: Option<u64>,
    pub is_delegation_request: bool,
}

impl GrantEvaluationRequest {
    pub fn for_operation(operation: impl Into<String>, actor_id: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            actor_id: actor_id.into(),
            task_id: None,
            target_path: None,
            additional_paths: Vec::new(),
            target_resource: None,
            recipient: None,
            requested_budget_units: None,
            require_exclusive_custody: false,
            expected_prestate_digest: None,
            expected_policy_revision: None,
            expected_harness_id: None,
            expected_session_id: None,
            expected_repo_fingerprint: None,
            presented_parent_grant_task_id: None,
            presented_approval_token: None,
            treat_plan_approval_as_permission: false,
            verified_cloud_scope: None,
            extra_layers: Vec::new(),
            now_epoch_s: None,
            is_delegation_request: false,
        }
    }
}

#[derive(Debug, Clone)]
struct TaskCustodyRow {
    id: String,
    status: String,
    scope: String,
    parent_task_id: Option<String>,
    assigned_to: String,
    lease_expires_at: Option<String>,
    lease_generation: i64,
    lease_lifecycle: String,
    revision: i64,
}

/// Composed result of folding [`DelegationLayer`]s in any order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposedScope {
    pub allowances: ScopeAllowanceSet,
    pub denials: BTreeSet<PathPattern>,
    pub approval_required_paths: BTreeSet<PathPattern>,
    pub allowed_operations: Option<BTreeSet<String>>,
    pub denied_operations: BTreeSet<String>,
    pub allowed_resources: Option<BTreeSet<String>>,
    pub denied_resources: BTreeSet<String>,
    pub max_budget_units: Option<u64>,
    pub min_expires_at_epoch_s: Option<u64>,
    pub expected_recipient: Option<String>,
    pub recipient_conflict: Option<(String, String)>,
}

/// Compose independent [`DelegationLayer`]s monotonically and deterministically
/// regardless of input slice order.
pub fn compose_layers_order_independent(layers: &[DelegationLayer]) -> ComposedScope {
    let mut allowances = ScopeAllowanceSet::Unconstrained;
    let mut denials = BTreeSet::new();
    let mut approval_required_paths = BTreeSet::new();
    let mut allowed_operations: Option<BTreeSet<String>> = None;
    let mut denied_operations = BTreeSet::new();
    let mut allowed_resources: Option<BTreeSet<String>> = None;
    let mut denied_resources = BTreeSet::new();
    let mut max_budget_units: Option<u64> = None;
    let mut min_expires_at_epoch_s: Option<u64> = None;
    let mut expected_recipient: Option<String> = None;
    let mut recipient_conflict: Option<(String, String)> = None;

    for layer in layers {
        allowances = allowances.intersect(&layer.allowances);
        for pat in &layer.denials {
            denials.insert(pat.clone());
        }
        for pat in &layer.approval_required_paths {
            approval_required_paths.insert(pat.clone());
        }
        if let Some(ops) = &layer.allowed_operations {
            allowed_operations = Some(match allowed_operations {
                Some(existing) => existing.intersection(ops).cloned().collect(),
                None => ops.clone(),
            });
        }
        for op in &layer.denied_operations {
            denied_operations.insert(op.clone());
        }
        if let Some(res) = &layer.allowed_resources {
            allowed_resources = Some(match allowed_resources {
                Some(existing) => existing.intersection(res).cloned().collect(),
                None => res.clone(),
            });
        }
        for res in &layer.denied_resources {
            denied_resources.insert(res.clone());
        }
        if let Some(budget) = layer.max_budget_units {
            max_budget_units = Some(match max_budget_units {
                Some(cur) => cur.min(budget),
                None => budget,
            });
        }
        if let Some(exp) = layer.expires_at_epoch_s {
            min_expires_at_epoch_s = Some(match min_expires_at_epoch_s {
                Some(cur) => cur.min(exp),
                None => exp,
            });
        }
        if let Some(rec) = &layer.expected_recipient {
            match &expected_recipient {
                None => expected_recipient = Some(rec.clone()),
                Some(prev) if prev != rec && recipient_conflict.is_none() => {
                    recipient_conflict = Some((prev.clone(), rec.clone()));
                }
                _ => {}
            }
        }
    }

    ComposedScope {
        allowances,
        denials: reduce_subsumed_patterns(denials),
        approval_required_paths: reduce_subsumed_patterns(approval_required_paths),
        allowed_operations,
        denied_operations,
        allowed_resources,
        denied_resources,
        max_budget_units,
        min_expires_at_epoch_s,
        expected_recipient,
        recipient_conflict,
    }
}

/// Evaluate the effective operation grant deterministically from repository policy,
/// custody, approvals, delegation lineage, and current prestate without mutating state.
pub fn evaluate_effective_grant(
    project_root: &Path,
    store_root: &Path,
    req: &GrantEvaluationRequest,
) -> EffectiveOperationGrant {
    let now_s = req
        .now_epoch_s
        .unwrap_or_else(|| parse_epoch_z(&now_epoch_z()).unwrap_or(0));
    let mut reasons: Vec<GrantRejectionReason> = Vec::new();
    let mut layers: Vec<DelegationLayer> = Vec::new();
    let mut approval_ids_matched = Vec::new();
    let mut approval_required = false;
    let mut cumulative_budget: Option<CumulativeBudgetSnapshot> = None;
    let mut remaining_depth: Option<u32> = None;

    if req.operation.trim().is_empty() {
        reasons.push(GrantRejectionReason::MissingMandatoryField {
            field: "operation".into(),
        });
    }
    if req.actor_id.trim().is_empty() {
        reasons.push(GrantRejectionReason::MissingMandatoryField {
            field: "actor_id".into(),
        });
    } else if is_missing_or_unknown_actor(&req.actor_id) {
        reasons.push(GrantRejectionReason::ActorSpoofingRejected {
            actor: req.actor_id.clone(),
            detail: "anonymous, blank, or 'unknown' actor identity is forbidden".into(),
        });
    }

    let main_root = get_main_repo_root(project_root).unwrap_or_else(|_| project_root.to_path_buf());
    let repo_fingerprint = compute_repo_fingerprint(&main_root);
    let base_branch = resolve_base_branch(&main_root, None);
    let base_oid = git_stdout(&main_root, &["rev-parse", "--verify", &base_branch])
        .or_else(|| git_stdout(&main_root, &["rev-parse", "--verify", "HEAD"]))
        .unwrap_or_else(|| "unborn".to_string());
    let head_oid = git_stdout(project_root, &["rev-parse", "--verify", "HEAD"])
        .unwrap_or_else(|| "unborn".into());
    let working_tree_digest = compute_working_tree_digest(project_root);

    let (accepted_cfg_opt, policy_revision) = match resolve_repository_policy(&main_root) {
        Ok((raw_bytes, cfg, rev, violation)) => {
            if let Some(msg) = violation {
                reasons.push(GrantRejectionReason::CandidatePolicyWideningRejected { detail: msg });
            }
            if let Some(reason) = detect_candidate_policy_widening(project_root, &main_root, &cfg) {
                reasons.push(reason);
            }
            let _ = raw_bytes;
            (Some(cfg), rev)
        }
        Err((code, msg)) => {
            reasons.push(GrantRejectionReason::AcceptedPolicyUnavailable {
                detail: format!("{code}: {msg}"),
            });
            (None, "unavailable".to_string())
        }
    };

    if let Err((code, msg)) = resolve_repository_override(&main_root) {
        reasons.push(GrantRejectionReason::AcceptedPolicyUnavailable {
            detail: format!("{code}: {msg}"),
        });
    }

    // Layer 0: Accepted repository policy layer.
    if let Some(cfg) = accepted_cfg_opt.as_ref() {
        let mut policy_layer = DelegationLayer::new(format!("repo_policy:{policy_revision}"));
        let gatekeeper = GatekeeperConfig::from_repo_config(project_root);
        for protected in cfg
            .governance
            .protected_paths
            .iter()
            .chain(gatekeeper.protected_paths.iter())
        {
            match PathPattern::parse(protected) {
                Ok(pat) => {
                    policy_layer.approval_required_paths.insert(pat);
                }
                Err(err) => reasons.push(err),
            }
        }
        policy_layer.approval_required_paths =
            reduce_subsumed_patterns(policy_layer.approval_required_paths);
        layers.push(policy_layer);
    }

    // Layer 1: Verified Cloud scope (never derived from local/candidate config).
    if let Some(cloud_scope) = &req.verified_cloud_scope {
        match cloud_scope.into_layer() {
            Ok(layer) => layers.push(layer),
            Err(err) => reasons.push(err),
        }
    }

    // Layer 2: Explicit PlanState::Approved anti-self-authorization guard.
    if req.treat_plan_approval_as_permission {
        reasons.push(GrantRejectionReason::PlanApprovalNotAuthorization {
            detail: "PlanState::Approved only marks workflow intent and never substitutes for human approval".into(),
        });
    }
    if let Ok(Some(plan)) = load_plan(project_root) {
        let constraints = &plan.constraints;
        if !constraints.forbidden_paths.is_empty() || constraints.file_touch_budget.is_some() {
            let mut plan_layer = DelegationLayer::new(format!("plan_scope:{:?}", plan.state));
            for raw in &constraints.forbidden_paths {
                match PathPattern::parse(raw) {
                    Ok(pat) => {
                        plan_layer.denials.insert(pat);
                    }
                    Err(err) => reasons.push(err),
                }
            }
            plan_layer.denials = reduce_subsumed_patterns(plan_layer.denials);
            if let Some(file_budget) = constraints.file_touch_budget {
                plan_layer.max_budget_units = u64::try_from(file_budget).ok();
            }
            layers.push(plan_layer);
        }
        // Explicitly reject attempts where a plan marked Approved has no live human approval for protected ops.
        if matches!(plan.state, PlanState::Approved)
            && req.presented_approval_token.as_deref() == Some("plan:approved")
        {
            reasons.push(GrantRejectionReason::PlanApprovalNotAuthorization {
                detail: "cannot treat plan 'approved' state token as operational approval".into(),
            });
        }
    }

    // Guard against copied parent grant tokens or forged approval tokens.
    if let Some(presented_task_id) = &req.presented_parent_grant_task_id
        && req.task_id.as_deref() != Some(presented_task_id.as_str())
    {
        reasons.push(GrantRejectionReason::CopiedParentGrantRejected {
            expected_task_id: req.task_id.clone(),
            presented_task_id: Some(presented_task_id.clone()),
            detail: "parent grants cannot be copied or replayed across child tasks".into(),
        });
    }
    if let Some(tok) = &req.presented_approval_token
        && !tok.trim().is_empty()
    {
        // Only canonical approval IDs verified in policy.db are accepted; arbitrary tokens are forged.
        if !tok.starts_with("appr_") {
            reasons.push(GrantRejectionReason::ForgedOrUnknownApproval {
                detail: format!("unrecognized approval token '{tok}'"),
            });
        }
    }

    // Layer 3: Task custody + ancestor delegation lineage + cumulative parent budget.
    let broker = DbBroker::new(store_root);
    let mut current_task_row: Option<TaskCustodyRow> = None;
    let active_harness_id = std::env::var("DECAPOD_HARNESS_ID").ok();
    let active_session_id = std::env::var("DECAPOD_SESSION_ID")
        .or_else(|_| std::env::var("DECAPOD_AGENT_ID"))
        .ok();

    if let Some(task_id) = &req.task_id {
        if task_id.trim().is_empty() {
            reasons.push(GrantRejectionReason::MissingMandatoryField {
                field: "task_id".into(),
            });
        } else {
            let todo_db = todo_db_path(store_root);
            if todo_db.exists() {
                let eval_res = broker.with_conn(
                    &todo_db,
                    "decapod",
                    None,
                    "effective_grant.todo_inspect",
                    |conn| {
                        let task = load_task_custody_row(conn, task_id)?;
                        let all_events = query_all_on_conn(conn, events::TODO)?;
                        let policies = extract_delegation_policies(&all_events);
                        let reservations = extract_delegation_reservations(&all_events);
                        let ancestors = if let Some(t) = &task {
                            collect_ancestor_chain(conn, t)?
                        } else {
                            Vec::new()
                        };
                        Ok((task, ancestors, policies, reservations))
                    },
                );
                match eval_res {
                    Ok((Some(task), ancestors, policies, reservations)) => {
                        current_task_row = Some(task.clone());
                        validate_task_custody(&task, req, now_s, &mut reasons);

                        // Apply ancestor tasks top-down (root -> ... -> parent -> self).
                        let mut chain: Vec<TaskCustodyRow> = ancestors;
                        chain.reverse();
                        chain.push(task.clone());

                        let mut running_depth: Option<u32> = None;
                        for (idx, node) in chain.iter().enumerate() {
                            match parse_scope_spec(&node.scope) {
                                Ok(parsed) => {
                                    layers.push(DelegationLayer::from_parsed_scope(
                                        format!("task_scope:{}", node.id),
                                        parsed,
                                    ));
                                }
                                Err(err) => reasons.push(err),
                            }
                            if idx + 1 < chain.len() {
                                let child_node = &chain[idx + 1];
                                if let Some(policy) = select_applicable_delegation_policy(
                                    &policies,
                                    &node.id,
                                    &child_node.id,
                                ) {
                                    match policy.to_layer(format!(
                                        "delegation:{}->{}",
                                        node.id, child_node.id
                                    )) {
                                        Ok(layer) => {
                                            if let Some(d) = policy.max_delegation_depth {
                                                running_depth = Some(match running_depth {
                                                    Some(prev) => prev.min(d),
                                                    None => d,
                                                });
                                            }
                                            layers.push(layer);
                                        }
                                        Err(err) => reasons.push(err),
                                    }
                                    if let Some(parent_total) = policy.max_total_budget_units {
                                        let total_used = sum_active_reservations_for_parent(
                                            &reservations,
                                            &node.id,
                                            None,
                                        );
                                        let other_used = sum_active_reservations_for_parent(
                                            &reservations,
                                            &node.id,
                                            Some(&child_node.id),
                                        );
                                        if let Some(requested) = req.requested_budget_units {
                                            if other_used.saturating_add(requested) > parent_total {
                                                reasons.push(
                                                    GrantRejectionReason::CumulativeBudgetExceeded {
                                                        parent_task_id: node.id.clone(),
                                                        parent_budget: parent_total,
                                                        already_reserved: other_used,
                                                        requested,
                                                    },
                                                );
                                            }
                                        } else if total_used > parent_total {
                                            reasons.push(
                                                GrantRejectionReason::CumulativeBudgetExceeded {
                                                    parent_task_id: node.id.clone(),
                                                    parent_budget: parent_total,
                                                    already_reserved: total_used,
                                                    requested: 0,
                                                },
                                            );
                                        }
                                        cumulative_budget = Some(CumulativeBudgetSnapshot {
                                            parent_task_id: node.id.clone(),
                                            parent_total_budget: parent_total,
                                            reserved_or_consumed_units: total_used,
                                            remaining_units: parent_total
                                                .saturating_sub(total_used),
                                        });
                                    }
                                }
                                // Decrement available delegation depth across each parent->child edge.
                                if let Some(d) = running_depth {
                                    if d == 0 {
                                        reasons.push(
                                            GrantRejectionReason::DelegationDepthExceeded {
                                                remaining: 0,
                                            },
                                        );
                                    }
                                    running_depth = Some(d.saturating_sub(1));
                                }
                            }
                        }

                        // If current task is acting as a parent creating/delegating another child:
                        if req.is_delegation_request {
                            for pol in policies.iter().filter(|p| p.parent_task_id == task.id) {
                                if let Some(d) = pol.max_delegation_depth {
                                    running_depth = Some(match running_depth {
                                        Some(prev) => prev.min(d),
                                        None => d,
                                    });
                                }
                            }
                            if running_depth == Some(0) {
                                reasons.push(GrantRejectionReason::DelegationDepthExceeded {
                                    remaining: 0,
                                });
                            }
                        }
                        remaining_depth = running_depth;
                    }
                    Ok((None, _, _, _)) => {
                        reasons.push(GrantRejectionReason::MissingMandatoryField {
                            field: format!("task:{task_id}"),
                        });
                    }
                    Err(err) => {
                        reasons.push(GrantRejectionReason::AcceptedPolicyUnavailable {
                            detail: format!("failed inspecting task lineage: {err}"),
                        });
                    }
                }
            }
        }
    }

    // Layer 4: Caller/request extra narrowing layers.
    for extra in &req.extra_layers {
        layers.push(extra.clone());
    }

    // Compose all layers monotonically (order-independent algebra).
    let composed = compose_layers_order_independent(&layers);

    if let Some((expected, actual)) = &composed.recipient_conflict {
        reasons.push(GrantRejectionReason::DelegationRecipientMismatch {
            expected: expected.clone(),
            actual: actual.clone(),
        });
    }
    if let Some(expected_rec) = &composed.expected_recipient {
        let actual_rec = req.recipient.as_deref().unwrap_or(&req.actor_id);
        if actual_rec != expected_rec {
            reasons.push(GrantRejectionReason::DelegationRecipientMismatch {
                expected: expected_rec.clone(),
                actual: actual_rec.to_string(),
            });
        }
    }

    // Check operation allow/deny lists.
    if composed.denied_operations.contains(&req.operation) {
        reasons.push(GrantRejectionReason::OperationDenied {
            operation: req.operation.clone(),
        });
    }
    if let Some(allowed_ops) = &composed.allowed_operations
        && !allowed_ops.contains(&req.operation)
    {
        reasons.push(GrantRejectionReason::OperationNotPermitted {
            operation: req.operation.clone(),
        });
    }

    // Check resource scope.
    if let Some(res) = &req.target_resource {
        if composed.denied_resources.contains(res) {
            reasons.push(GrantRejectionReason::ResourceMismatch {
                resource: res.clone(),
            });
        }
        if let Some(allowed_res) = &composed.allowed_resources
            && !allowed_res.contains(res)
        {
            reasons.push(GrantRejectionReason::ResourceMismatch {
                resource: res.clone(),
            });
        }
    }

    // Check budget constraint.
    if let (Some(requested), Some(limit)) = (req.requested_budget_units, composed.max_budget_units)
        && requested > limit
    {
        reasons.push(GrantRejectionReason::BudgetExceeded { requested, limit });
    }

    // Check expiration across all layers.
    if let Some(exp) = composed.min_expires_at_epoch_s
        && now_s >= exp
    {
        reasons.push(GrantRejectionReason::LeaseExpired {
            task_id: req.task_id.clone().unwrap_or_else(|| "grant".into()),
            expired_at: format!("{exp}Z"),
        });
    }

    // Check all target paths against composed allowances & denials.
    // For non-filesystem task lifecycle commands (e.g. `task.archive --path <task_id> --task-id <task_id>`),
    // treat `--path <task_id>` as the policy target identifier rather than a repository file path.
    let is_task_target = is_task_lifecycle_operation(&req.operation)
        && req.task_id.is_some()
        && req.target_path.as_deref() == req.task_id.as_deref();
    let policy_target = if is_task_target {
        req.task_id.as_deref()
    } else {
        req.target_path.as_deref()
    };

    let mut paths_to_check: Vec<String> = Vec::new();
    if !is_task_target && let Some(p) = &req.target_path {
        paths_to_check.push(p.clone());
    }
    paths_to_check.extend(req.additional_paths.iter().cloned());

    let mut target_hits_protected_path = false;
    for raw_path in &paths_to_check {
        match PathPattern::normalize_concrete_path(raw_path) {
            Ok(segments) => {
                if let Some(denied) = composed
                    .denials
                    .iter()
                    .find(|pat| pat.matches_segments(&segments))
                {
                    reasons.push(GrantRejectionReason::PathExplicitlyDenied {
                        path: raw_path.clone(),
                        matched_rule: denied.canonical_string(),
                    });
                } else if !composed.allowances.matches_segments(&segments) {
                    reasons.push(GrantRejectionReason::PathNotAllowed {
                        path: raw_path.clone(),
                    });
                }
                if composed
                    .approval_required_paths
                    .iter()
                    .any(|pat| pat.matches_segments(&segments))
                {
                    target_hits_protected_path = true;
                }
            }
            Err(err) => reasons.push(err),
        }
    }

    // Evaluate existing policy risk + live scoped approvals in policy.db (never auto-approving).
    let target_fp = derive_fingerprint(&req.operation, policy_target, "global");
    if !is_read_only_operation(&req.operation) && !req.operation.trim().is_empty() {
        let riskmap = load_risk_map(store_root).unwrap_or(RiskMap { zones: vec![] });
        let (risk_level, _risk_reqs) = eval_risk(&req.operation, policy_target, &riskmap);
        let policy_db = policy_db_path(store_root);
        let cats = match accepted_cfg_opt.as_ref() {
            Some(cfg) => cfg.governance.approval_categories.clone(),
            None => configured_approval_categories_at_project(project_root).unwrap_or_default(),
        };
        let category_requires_approval = configured_approval_required(&cats, &req.operation);
        let risk_requires_approval = matches!(risk_level, RiskLevel::CRITICAL);

        if policy_db.exists() {
            let required_trust = required_trust_for_operation(&req.operation);
            let approval_check = broker.with_conn(
                &policy_db,
                "decapod",
                None,
                "effective_grant.policy_check",
                |conn| {
                    let trust = actor_trust_level_on_conn(conn, &req.actor_id)?;
                    let trust_insufficient =
                        trust_level_to_int(&trust) < trust_level_to_int(required_trust);
                    let hil = is_high_risk(risk_level) && trust != "core";
                    let needs = risk_requires_approval
                        || category_requires_approval
                        || target_hits_protected_path
                        || hil;
                    let scope = req
                        .task_id
                        .as_deref()
                        .filter(|s| !s.is_empty())
                        .unwrap_or("global");
                    let scoped_fp = derive_fingerprint(&req.operation, policy_target, scope);
                    let has_live =
                        check_approval_on_conn(conn, &req.operation, policy_target, "global")?
                            || has_live_scoped_approval_on_conn(conn, &scoped_fp, scope)?;
                    let matched_ids = if let Some(target) = policy_target {
                        exact_target_approval_ids_on_conn(conn, &req.operation, target)?
                    } else {
                        Vec::new()
                    };
                    Ok((trust, trust_insufficient, needs, has_live, matched_ids))
                },
            );
            match approval_check {
                Ok((actor_trust, trust_insufficient, needs, has_live, matched_ids)) => {
                    approval_required = needs;
                    approval_ids_matched = matched_ids;
                    if trust_insufficient {
                        reasons.push(GrantRejectionReason::TrustLevelInsufficient {
                            actor: req.actor_id.clone(),
                            actor_trust,
                            required_trust: required_trust.to_string(),
                        });
                    }
                    if let Some(tok) = &req.presented_approval_token
                        && !approval_ids_matched.iter().any(|id| id == tok)
                    {
                        reasons.push(GrantRejectionReason::ForgedOrUnknownApproval {
                            detail: format!("approval token '{tok}' is neither live nor bound to target '{target_fp}'"),
                        });
                    }
                    if needs && !has_live {
                        if target_hits_protected_path {
                            reasons.push(GrantRejectionReason::ProtectedPathApprovalRequired {
                                path: req
                                    .target_path
                                    .clone()
                                    .unwrap_or_else(|| "<protected>".into()),
                            });
                        } else {
                            reasons.push(GrantRejectionReason::ApprovalRequired {
                                operation: req.operation.clone(),
                                target_fingerprint: target_fp.clone(),
                                scope: "global".into(),
                            });
                        }
                    }
                }
                Err(err) => {
                    reasons.push(GrantRejectionReason::AcceptedPolicyUnavailable {
                        detail: format!("failed inspecting policy approvals: {err}"),
                    });
                }
            }
        } else if category_requires_approval || target_hits_protected_path || risk_requires_approval
        {
            approval_required = true;
            if target_hits_protected_path {
                reasons.push(GrantRejectionReason::ProtectedPathApprovalRequired {
                    path: req
                        .target_path
                        .clone()
                        .unwrap_or_else(|| "<protected>".into()),
                });
            } else {
                reasons.push(GrantRejectionReason::ApprovalRequired {
                    operation: req.operation.clone(),
                    target_fingerprint: target_fp.clone(),
                    scope: "global".into(),
                });
            }
        }
    }

    // Build prestate binding & check TOCTOU / staleness invariants.
    let mut prestate = GrantPrestateBinding {
        repo_fingerprint: repo_fingerprint.clone(),
        accepted_base_ref: base_branch,
        accepted_base_oid: base_oid,
        head_oid,
        policy_revision: policy_revision.clone(),
        working_tree_digest,
        task_id: current_task_row
            .as_ref()
            .map(|t| t.id.clone())
            .or_else(|| req.task_id.clone()),
        task_revision: current_task_row.as_ref().map(|t| t.revision),
        lease_generation: current_task_row.as_ref().map(|t| t.lease_generation),
        lease_expires_at: current_task_row
            .as_ref()
            .and_then(|t| t.lease_expires_at.clone()),
        session_id: active_session_id.clone(),
        harness_id: active_harness_id.clone(),
        prestate_digest: String::new(),
    };
    prestate.compute_digest();

    if let Some(exp_repo) = &req.expected_repo_fingerprint
        && exp_repo != &repo_fingerprint
    {
        reasons.push(GrantRejectionReason::RepoIdentityMismatch {
            expected: exp_repo.clone(),
            actual: repo_fingerprint,
        });
    }
    if let Some(exp_rev) = &req.expected_policy_revision
        && exp_rev != &policy_revision
    {
        reasons.push(GrantRejectionReason::PolicyRevisionMismatch {
            expected: exp_rev.clone(),
            actual: policy_revision,
        });
    }
    if let Some(exp_prestate) = &req.expected_prestate_digest
        && exp_prestate != &prestate.prestate_digest
    {
        reasons.push(GrantRejectionReason::PrestateDrift {
            expected: exp_prestate.clone(),
            actual: prestate.prestate_digest.clone(),
        });
    }
    if let Some(exp_harness) = &req.expected_harness_id {
        let actual = active_harness_id.as_deref().unwrap_or("<none>");
        if actual != exp_harness {
            reasons.push(GrantRejectionReason::HarnessOrSessionMismatch {
                expected: exp_harness.clone(),
                actual: actual.to_string(),
            });
        }
    }
    if let Some(exp_sess) = &req.expected_session_id {
        let actual = active_session_id.as_deref().unwrap_or("<none>");
        if actual != exp_sess {
            reasons.push(GrantRejectionReason::HarnessOrSessionMismatch {
                expected: exp_sess.clone(),
                actual: actual.to_string(),
            });
        }
    }

    // Precedence of verdict: Denied > Stale > Unproven > Allowed.
    let decision = if reasons.is_empty() {
        GrantDecision::Allowed
    } else if reasons
        .iter()
        .any(|r| r.decision_class() == GrantDecision::Denied)
    {
        GrantDecision::Denied
    } else if reasons
        .iter()
        .any(|r| r.decision_class() == GrantDecision::Stale)
    {
        GrantDecision::Stale
    } else {
        GrantDecision::Unproven
    };

    let summary = if reasons.is_empty() {
        format!(
            "effective grant allowed '{}' for '{}' under policy {}",
            req.operation, req.actor_id, prestate.policy_revision
        )
    } else {
        reasons
            .iter()
            .map(GrantRejectionReason::summary)
            .collect::<Vec<_>>()
            .join("; ")
    };

    let layer_summaries: Vec<GrantLayerSummary> = layers
        .iter()
        .map(|l| GrantLayerSummary {
            layer_id: l.layer_id.clone(),
            allowances: l.allowances.canonical_strings(),
            denials: l
                .denials
                .iter()
                .map(PathPattern::canonical_string)
                .collect(),
            max_budget_units: l.max_budget_units,
            max_delegation_depth: l.max_delegation_depth,
        })
        .collect();

    let mut grant = EffectiveOperationGrant {
        schema_version: EFFECTIVE_GRANT_SCHEMA_VERSION.to_string(),
        grant_digest: String::new(),
        decision,
        reasons,
        summary,
        operation: req.operation.clone(),
        actor_id: req.actor_id.clone(),
        task_id: req.task_id.clone(),
        target_path: req.target_path.clone(),
        target_resource: req.target_resource.clone(),
        recipient: req.recipient.clone(),
        requested_budget_units: req.requested_budget_units,
        effective_allowances: composed.allowances.canonical_strings(),
        effective_denials: composed
            .denials
            .iter()
            .map(PathPattern::canonical_string)
            .collect(),
        approval_required_paths: composed
            .approval_required_paths
            .iter()
            .map(PathPattern::canonical_string)
            .collect(),
        effective_allowed_operations: composed.allowed_operations.map(|s| s.into_iter().collect()),
        effective_denied_operations: composed.denied_operations.into_iter().collect(),
        effective_allowed_resources: composed.allowed_resources.map(|s| s.into_iter().collect()),
        effective_max_budget_units: composed.max_budget_units,
        remaining_delegation_depth: remaining_depth,
        cumulative_budget,
        approval_required,
        approval_ids_matched,
        target_fingerprint: target_fp,
        prestate,
        layers: layer_summaries,
        execution_outcome: ExecutionOutcomeKind::NotAttempted,
    };
    grant.grant_digest = compute_grant_digest(&grant);
    grant
}

/// Enforce [`EffectiveOperationGrant`] when a task is delegated (`parent_task_id.is_some()`),
/// has an explicit delegation policy, or declares restrictive scope rules.
/// Unscoped legacy tasks without delegation boundaries continue through existing policy gates.
pub fn enforce_task_operation_if_delegated(
    project_root: &Path,
    store_root: &Path,
    operation: &str,
    actor_id: &str,
    task_id: &str,
    recipient: Option<&str>,
    paths: &[String],
) -> Result<Option<EffectiveOperationGrant>, DecapodError> {
    let broker = DbBroker::new(store_root);
    if broker.is_cloud() {
        return Ok(None);
    }
    let todo_db = todo_db_path(store_root);
    if !todo_db.exists() {
        return Ok(None);
    }
    let should_enforce = broker.with_conn(
        &todo_db,
        "decapod",
        None,
        "effective_grant.delegation_check",
        |conn| {
            let Some(task) = load_task_custody_row(conn, task_id)? else {
                return Ok(false);
            };
            if task.parent_task_id.is_some() {
                return Ok(true);
            }
            let parsed = parse_scope_spec(&task.scope)
                .map_err(|r| DecapodError::ValidationError(r.summary()))?;
            let has_scope_restriction =
                !matches!(parsed.allowances, ScopeAllowanceSet::Unconstrained)
                    || !parsed.denials.is_empty()
                    || parsed.allowed_operations.is_some()
                    || parsed.allowed_resources.is_some()
                    || parsed.max_budget_units.is_some()
                    || parsed.max_delegation_depth.is_some()
                    || parsed.expected_recipient.is_some();
            if !parsed.is_advisory_paths && has_scope_restriction {
                return Ok(true);
            }
            let all_events = query_all_on_conn(conn, events::TODO)?;
            let policies = extract_delegation_policies(&all_events);
            Ok(policies.iter().any(|p| {
                p.parent_task_id == task_id || p.child_task_id.as_deref() == Some(task_id)
            }))
        },
    )?;
    if !should_enforce {
        return Ok(None);
    }
    let mut req = GrantEvaluationRequest::for_operation(operation, actor_id);
    req.task_id = Some(task_id.to_string());
    req.recipient = recipient.map(str::to_string);
    req.target_path = paths.first().cloned();
    if paths.len() > 1 {
        req.additional_paths = paths[1..].to_vec();
    }
    req.require_exclusive_custody = true;
    let grant = enforce_mediated_operation(project_root, store_root, &req)?;
    Ok(Some(grant))
}

/// Resolve an unambiguous active task bound to the current worktree branch, if any.
pub fn find_active_task_for_checkout(
    project_root: &Path,
    store_root: &Path,
) -> Result<Option<String>, DecapodError> {
    let observed = crate::core::workspace::observe_checkout(project_root);
    if observed.branch.is_empty() {
        return Ok(None);
    }
    let broker = DbBroker::new(store_root);
    if broker.is_cloud() {
        return Ok(None);
    }
    let todo_db = todo_db_path(store_root);
    if !todo_db.exists() {
        return Ok(None);
    }
    broker.with_conn(
        &todo_db,
        "decapod",
        None,
        "effective_grant.branch_task_lookup",
        |conn| {
            if !conn.has_table("tasks")? {
                return Ok(None);
            }
            let mut stmt = conn.prepare("SELECT id, hash FROM tasks ORDER BY id LIMIT 10001")?;
            let rows =
                stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            let mut matched = Vec::new();
            for row in rows {
                let (id, hash) = row?;
                if crate::core::workspace::branch_names_task(&observed.branch, &id, &hash) {
                    matched.push(id);
                }
            }
            if matched.len() == 1 {
                Ok(matched.into_iter().next())
            } else {
                Ok(None)
            }
        },
    )
}

/// Filter out Decapod's own per-PR governance/spec bookkeeping artifacts so branch path grants
/// govern user/candidate paths and sensitive config files (`.decapod/config.toml`, `.decapod/OVERRIDE.md`).
fn is_governance_bookkeeping_path(path: &str) -> bool {
    path == ".decapod/governance.json" || path.starts_with(".decapod/managed/")
}

/// Enforce effective operation grants before publication (`workspace.publish`).
pub fn enforce_workspace_publish_grant(
    project_root: &Path,
    store_root: &Path,
    actor_id: &str,
    changed_files: &BTreeSet<String>,
) -> Result<(), DecapodError> {
    let main_root = get_main_repo_root(project_root).unwrap_or_else(|_| project_root.to_path_buf());
    if let Ok((_, accepted_cfg, _, violation)) = resolve_repository_policy(&main_root) {
        if let Some(msg) = violation {
            return Err(DecapodError::ValidationError(
                GrantRejectionReason::CandidatePolicyWideningRejected { detail: msg }.summary(),
            ));
        }
        if let Some(widening) =
            detect_candidate_policy_widening(project_root, &main_root, &accepted_cfg)
        {
            return Err(DecapodError::ValidationError(widening.summary()));
        }
    }

    let check_paths: Vec<String> = changed_files
        .iter()
        .filter(|p| !is_governance_bookkeeping_path(p))
        .cloned()
        .collect();

    if let Some(task_id) = find_active_task_for_checkout(project_root, store_root)? {
        enforce_task_operation_if_delegated(
            project_root,
            store_root,
            "workspace.publish",
            actor_id,
            &task_id,
            None,
            &check_paths,
        )?;
    }

    // Even without a delegated task, check any changed protected_paths explicitly against live approval.
    if let Ok((_, accepted_cfg, _, _)) = resolve_repository_policy(&main_root)
        && !accepted_cfg.governance.protected_paths.is_empty()
    {
        for raw_path in &check_paths {
            let Ok(segments) = PathPattern::normalize_concrete_path(raw_path) else {
                continue;
            };
            let hits_protected = accepted_cfg.governance.protected_paths.iter().any(|rule| {
                PathPattern::parse(rule)
                    .map(|pat| pat.matches_segments(&segments))
                    .unwrap_or(false)
            });
            if hits_protected {
                let mut req = GrantEvaluationRequest::for_operation("workspace.publish", actor_id);
                req.target_path = Some(raw_path.clone());
                enforce_mediated_operation(project_root, store_root, &req)?;
            }
        }
    }
    Ok(())
}

/// Evaluate an operation grant and return `Err(DecapodError)` unless `GrantDecision::Allowed`.
pub fn enforce_mediated_operation(
    project_root: &Path,
    store_root: &Path,
    req: &GrantEvaluationRequest,
) -> Result<EffectiveOperationGrant, DecapodError> {
    let grant = evaluate_effective_grant(project_root, store_root, req);
    if !grant.decision.is_allowed() {
        return Err(grant.first_error());
    }
    Ok(grant)
}

/// Verify that candidate branch `.decapod/config.toml` does not attempt to weaken or widen
/// protections established by the accepted repository policy.
fn detect_candidate_policy_widening(
    project_root: &Path,
    _main_root: &Path,
    accepted: &DecapodProjectConfig,
) -> Option<GrantRejectionReason> {
    let candidate_path = project_root.join(".decapod").join("config.toml");
    if !candidate_path.exists() {
        return None;
    }
    let Ok(bytes) = std::fs::read(&candidate_path) else {
        return None;
    };
    let Ok(candidate_str) = std::str::from_utf8(&bytes) else {
        return Some(GrantRejectionReason::CandidatePolicyWideningRejected {
            detail: "candidate .decapod/config.toml is not valid UTF-8".into(),
        });
    };
    let Ok(candidate) = toml::from_str::<DecapodProjectConfig>(candidate_str) else {
        return None;
    };
    let acc_gov: &GovernanceConfig = &accepted.governance;
    let cand_gov: &GovernanceConfig = &candidate.governance;

    let acc_protected: BTreeSet<&str> =
        acc_gov.protected_paths.iter().map(String::as_str).collect();
    let cand_protected: BTreeSet<&str> = cand_gov
        .protected_paths
        .iter()
        .map(String::as_str)
        .collect();
    if !acc_protected.is_subset(&cand_protected) {
        return Some(GrantRejectionReason::CandidatePolicyWideningRejected {
            detail:
                "candidate .decapod/config.toml removed accepted governance.protected_paths entries"
                    .into(),
        });
    }

    let acc_cats: BTreeSet<&str> = acc_gov
        .approval_categories
        .iter()
        .map(String::as_str)
        .collect();
    let cand_cats: BTreeSet<&str> = cand_gov
        .approval_categories
        .iter()
        .map(String::as_str)
        .collect();
    if !acc_cats.is_subset(&cand_cats) {
        return Some(GrantRejectionReason::CandidatePolicyWideningRejected {
            detail: "candidate .decapod/config.toml removed accepted governance.approval_categories entries".into(),
        });
    }

    if !acc_gov.allow_basic_shared_claims && cand_gov.allow_basic_shared_claims {
        return Some(GrantRejectionReason::CandidatePolicyWideningRejected {
            detail: "candidate .decapod/config.toml cannot enable allow_basic_shared_claims beyond accepted policy".into(),
        });
    }

    if cand_gov.rbac.actor_roles != acc_gov.rbac.actor_roles
        && (!cand_gov.rbac.actor_roles.is_empty() || !acc_gov.rbac.actor_roles.is_empty())
    {
        for (actor, roles) in &cand_gov.rbac.actor_roles {
            let acc_roles = acc_gov.rbac.actor_roles.get(actor);
            if roles
                .iter()
                .any(|r| !acc_roles.is_some_and(|ar| ar.contains(r)))
            {
                return Some(GrantRejectionReason::CandidatePolicyWideningRejected {
                    detail: format!(
                        "candidate .decapod/config.toml cannot grant RBAC role to '{actor}' beyond accepted policy"
                    ),
                });
            }
        }
    }

    None
}

fn validate_task_custody(
    task: &TaskCustodyRow,
    req: &GrantEvaluationRequest,
    now_s: u64,
    reasons: &mut Vec<GrantRejectionReason>,
) {
    let now_ts = format!("{now_s}Z");
    let lifecycle = LeaseLifecycle::parse(Some(&task.lease_lifecycle));
    let lease_st = lease_state(task.lease_expires_at.as_deref(), &now_ts);

    let must_hold_lease = req.require_exclusive_custody
        || task.parent_task_id.is_some()
        || !task.assigned_to.is_empty();

    if !must_hold_lease {
        return;
    }

    if !task.assigned_to.is_empty() && task.assigned_to != req.actor_id {
        reasons.push(GrantRejectionReason::CustodyNotHeld {
            task_id: task.id.clone(),
            holder: task.assigned_to.clone(),
            actor: req.actor_id.clone(),
        });
        return;
    }

    if task.assigned_to.is_empty() {
        reasons.push(GrantRejectionReason::CustodyNotHeld {
            task_id: task.id.clone(),
            holder: "<unclaimed>".into(),
            actor: req.actor_id.clone(),
        });
        return;
    }

    if !lifecycle.holds_exclusive_custody() {
        reasons.push(GrantRejectionReason::LeaseMissingOrUnspecified {
            task_id: task.id.clone(),
        });
    } else if matches!(lease_st, LeaseState::Expired) {
        reasons.push(GrantRejectionReason::LeaseExpired {
            task_id: task.id.clone(),
            expired_at: task.lease_expires_at.clone().unwrap_or_default(),
        });
    } else if matches!(lease_st, LeaseState::Unspecified) && req.require_exclusive_custody {
        reasons.push(GrantRejectionReason::LeaseMissingOrUnspecified {
            task_id: task.id.clone(),
        });
    }

    if task.status == "archived" {
        reasons.push(GrantRejectionReason::CustodyNotHeld {
            task_id: task.id.clone(),
            holder: format!("status:{}", task.status),
            actor: req.actor_id.clone(),
        });
    }
}

fn load_task_custody_row(
    conn: &Connection,
    task_id: &str,
) -> Result<Option<TaskCustodyRow>, DecapodError> {
    if !conn.has_table("tasks")? {
        return Ok(None);
    }
    conn.query_row(
        "SELECT id, status, COALESCE(scope, ''), parent_task_id, COALESCE(assigned_to, ''),
                lease_expires_at, COALESCE(lease_generation, 0), COALESCE(lease_lifecycle, ''),
                COALESCE(revision, 0)
         FROM tasks WHERE id = ?1",
        [task_id],
        |row| {
            Ok(TaskCustodyRow {
                id: row.get(0)?,
                status: row.get(1)?,
                scope: row.get(2)?,
                parent_task_id: row.get(3)?,
                assigned_to: row.get(4)?,
                lease_expires_at: row.get(5)?,
                lease_generation: row.get(6)?,
                lease_lifecycle: row.get(7)?,
                revision: row.get(8)?,
            })
        },
    )
    .optional()
    .map_err(DecapodError::StorageError)
}

fn collect_ancestor_chain(
    conn: &Connection,
    start: &TaskCustodyRow,
) -> Result<Vec<TaskCustodyRow>, DecapodError> {
    let mut ancestors = Vec::new();
    let mut seen = BTreeSet::from([start.id.clone()]);
    let mut cursor = start.parent_task_id.clone();

    while let Some(parent_id) = cursor {
        if ancestors.len() >= MAX_DELEGATION_CHAIN_DEPTH || !seen.insert(parent_id.clone()) {
            break;
        }
        let Some(parent) = load_task_custody_row(conn, &parent_id)? else {
            break;
        };
        cursor = parent.parent_task_id.clone();
        ancestors.push(parent);
    }
    Ok(ancestors)
}

pub fn extract_delegation_policies(
    events_list: &[events::StoredEvent],
) -> Vec<DelegationPolicyRecord> {
    let mut out = Vec::new();
    for ev in events_list {
        if ev.event_type != DELEGATION_POLICY_EVENT_TYPE {
            continue;
        }
        let payload = ev.payload.get("payload").unwrap_or(&ev.payload);
        if let Ok(rec) = serde_json::from_value::<DelegationPolicyRecord>(payload.clone()) {
            out.push(rec);
        }
    }
    out
}

pub fn extract_delegation_reservations(
    events_list: &[events::StoredEvent],
) -> Vec<DelegationReservationRecord> {
    let mut latest_by_key: BTreeMap<(String, String), DelegationReservationRecord> =
        BTreeMap::new();
    for ev in events_list {
        if ev.event_type != DELEGATION_RESERVE_EVENT_TYPE {
            continue;
        }
        let payload = ev.payload.get("payload").unwrap_or(&ev.payload);
        if let Ok(rec) = serde_json::from_value::<DelegationReservationRecord>(payload.clone()) {
            latest_by_key.insert((rec.parent_task_id.clone(), rec.child_task_id.clone()), rec);
        }
    }
    latest_by_key.into_values().collect()
}

fn select_applicable_delegation_policy<'a>(
    policies: &'a [DelegationPolicyRecord],
    parent_id: &str,
    child_id: &str,
) -> Option<&'a DelegationPolicyRecord> {
    // Prefer exact child policy, falling back to latest wildcard policy for that parent.
    policies
        .iter()
        .rev()
        .find(|p| p.parent_task_id == parent_id && p.child_task_id.as_deref() == Some(child_id))
        .or_else(|| {
            policies
                .iter()
                .rev()
                .find(|p| p.parent_task_id == parent_id && p.child_task_id.is_none())
        })
}

fn sum_active_reservations_for_parent(
    reservations: &[DelegationReservationRecord],
    parent_id: &str,
    exclude_child_id: Option<&str>,
) -> u64 {
    reservations
        .iter()
        .filter(|r| {
            r.parent_task_id == parent_id
                && !matches!(r.status, ReservationStatus::Released)
                && exclude_child_id.is_none_or(|c| r.child_task_id != c)
        })
        .fold(0u64, |acc, r| acc.saturating_add(r.units))
}

/// Atomically persist a parent->child delegation policy on the `TODO` stream inside the caller's
/// transaction, verifying monotonic narrowing relative to any ancestor policies.
pub fn record_delegation_policy_on_conn(
    conn: &Connection,
    ts: &str,
    actor: &str,
    record: &DelegationPolicyRecord,
) -> Result<String, DecapodError> {
    if is_missing_or_unknown_actor(actor) {
        return Err(DecapodError::ValidationError(
            GrantRejectionReason::ActorSpoofingRejected {
                actor: actor.to_string(),
                detail: "anonymous or 'unknown' actor cannot delegate authority".into(),
            }
            .summary(),
        ));
    }

    let parent_task = load_task_custody_row(conn, &record.parent_task_id)?.ok_or_else(|| {
        DecapodError::ValidationError(format!(
            "Parent task '{}' not found for delegation",
            record.parent_task_id
        ))
    })?;
    if !parent_task.assigned_to.is_empty() && parent_task.assigned_to != actor {
        return Err(DecapodError::ValidationError(
            GrantRejectionReason::CustodyNotHeld {
                task_id: parent_task.id,
                holder: parent_task.assigned_to,
                actor: actor.to_string(),
            }
            .summary(),
        ));
    }

    let child_layer = record
        .to_layer(format!("delegation:{}", record.parent_task_id))
        .map_err(|r| DecapodError::ValidationError(r.summary()))?;

    let all_events = query_all_on_conn(conn, events::TODO)?;
    let policies = extract_delegation_policies(&all_events);

    // Build effective ancestor scope to prevent widening by an intermediate parent.
    let mut ancestors = collect_ancestor_chain(conn, &parent_task)?;
    ancestors.reverse();
    ancestors.push(parent_task.clone());

    let mut parent_layers = Vec::new();
    let mut parent_remaining_depth: Option<u32> = None;
    for (idx, node) in ancestors.iter().enumerate() {
        let parsed = parse_scope_spec(&node.scope)
            .map_err(|r| DecapodError::ValidationError(r.summary()))?;
        parent_layers.push(DelegationLayer::from_parsed_scope(
            format!("task_scope:{}", node.id),
            parsed,
        ));
        if idx + 1 < ancestors.len()
            && let Some(pol) =
                select_applicable_delegation_policy(&policies, &node.id, &ancestors[idx + 1].id)
        {
            let layer = pol
                .to_layer(format!("delegation:{}->{}", node.id, ancestors[idx + 1].id))
                .map_err(|r| DecapodError::ValidationError(r.summary()))?;
            if let Some(d) = pol.max_delegation_depth {
                parent_remaining_depth = Some(match parent_remaining_depth {
                    Some(prev) => prev.min(d),
                    None => d,
                });
            }
            parent_layers.push(layer);
            if let Some(d) = parent_remaining_depth {
                parent_remaining_depth = Some(d.saturating_sub(1));
            }
        }
    }

    // Also include existing wildcard/self policy already set on parent_task if any.
    for pol in policies
        .iter()
        .filter(|p| p.parent_task_id == record.parent_task_id && p.child_task_id.is_none())
    {
        if let Some(d) = pol.max_delegation_depth {
            parent_remaining_depth = Some(match parent_remaining_depth {
                Some(prev) => prev.min(d),
                None => d,
            });
        }
        let layer = pol
            .to_layer(format!("existing_parent_policy:{}", pol.parent_task_id))
            .map_err(|r| DecapodError::ValidationError(r.summary()))?;
        parent_layers.push(layer);
    }

    if parent_remaining_depth == Some(0) {
        return Err(DecapodError::ValidationError(
            GrantRejectionReason::DelegationDepthExceeded { remaining: 0 }.summary(),
        ));
    }

    let parent_composed = compose_layers_order_independent(&parent_layers);
    // Reject explicit child widening relative to parent's composed allowances/denials/budget.
    match (&parent_composed.allowances, &child_layer.allowances) {
        (ScopeAllowanceSet::Restricted(_), ScopeAllowanceSet::Unconstrained)
            if record.allowed_paths.is_empty() && !parent_composed.allowances.is_empty() =>
        {
            // An empty allowed_paths simply inherits parent's restricted allowances on evaluation,
            // EXCEPT when child explicitly attempted `["**"]`.
        }
        (ScopeAllowanceSet::Restricted(_), ScopeAllowanceSet::Restricted(child_pats)) => {
            for pat in child_pats {
                if !parent_composed.allowances.subsumes_pattern(pat) {
                    return Err(DecapodError::ValidationError(
                        GrantRejectionReason::DelegationWideningRejected {
                            layer_id: record.parent_task_id.clone(),
                            detail: format!(
                                "child path allowance '{}' widens outside parent allowance [{}]",
                                pat.canonical_string(),
                                parent_composed.allowances.canonical_strings().join(", ")
                            ),
                        }
                        .summary(),
                    ));
                }
                if parent_composed
                    .denials
                    .iter()
                    .any(|denied| denied.subsumes(pat))
                {
                    return Err(DecapodError::ValidationError(
                        GrantRejectionReason::DelegationWideningRejected {
                            layer_id: record.parent_task_id.clone(),
                            detail: format!(
                                "child path allowance '{}' targets parent-denied path",
                                pat.canonical_string()
                            ),
                        }
                        .summary(),
                    ));
                }
            }
        }
        (ScopeAllowanceSet::Restricted(_), ScopeAllowanceSet::Unconstrained)
            if record.allowed_paths.iter().any(|p| p.trim() == "**") =>
        {
            return Err(DecapodError::ValidationError(
                GrantRejectionReason::DelegationWideningRejected {
                    layer_id: record.parent_task_id.clone(),
                    detail: "child cannot specify '**' when parent is path-restricted".into(),
                }
                .summary(),
            ));
        }
        _ => {}
    }

    if let Some(parent_budget) = parent_composed.max_budget_units {
        if let Some(child_total) = record.max_total_budget_units
            && child_total > parent_budget
        {
            return Err(DecapodError::ValidationError(
                GrantRejectionReason::DelegationWideningRejected {
                    layer_id: record.parent_task_id.clone(),
                    detail: format!(
                        "child total budget {child_total} exceeds parent effective budget {parent_budget}"
                    ),
                }
                .summary(),
            ));
        }
        if let Some(child_budget) = child_layer.max_budget_units
            && child_budget > parent_budget
        {
            return Err(DecapodError::ValidationError(
                GrantRejectionReason::DelegationWideningRejected {
                    layer_id: record.parent_task_id.clone(),
                    detail: format!(
                        "child budget {child_budget} exceeds parent effective budget {parent_budget}"
                    ),
                }
                .summary(),
            ));
        }
    }

    if let (Some(parent_d), Some(child_d)) = (parent_remaining_depth, record.max_delegation_depth)
        && child_d > parent_d
    {
        return Err(DecapodError::ValidationError(
            GrantRejectionReason::DelegationWideningRejected {
                layer_id: record.parent_task_id.clone(),
                detail: format!(
                    "child delegation depth {child_d} exceeds parent remaining depth {parent_d}"
                ),
            }
            .summary(),
        ));
    }

    let event_id = format!(
        "dlg_{}",
        &sha256_hex(
            serde_json::json!({
                "ts": ts,
                "actor": actor,
                "record": record,
                "seq_hint": all_events.len(),
            })
            .to_string()
            .as_bytes()
        )[..20]
    );

    let event = serde_json::json!({
        "ts": ts,
        "event_id": event_id,
        "event_type": DELEGATION_POLICY_EVENT_TYPE,
        "status": "success",
        "task_id": record.parent_task_id,
        "actor": actor,
        "payload": record,
    });
    append_on_conn(conn, events::TODO, &event)?;
    Ok(event_id)
}

/// Atomically check cumulative shared-parent budget across all sibling children and record
/// a reservation for `child_task_id` inside the caller's SQLite write transaction.
pub fn reserve_delegation_budget_on_conn(
    conn: &Connection,
    ts: &str,
    actor: &str,
    child_task_id: &str,
    generation: i64,
    requested_units: Option<u64>,
    resource: Option<&str>,
) -> Result<Vec<DelegationReservationRecord>, DecapodError> {
    let Some(child) = load_task_custody_row(conn, child_task_id)? else {
        return Ok(Vec::new());
    };
    if child.parent_task_id.is_none() {
        return Ok(Vec::new());
    }

    let mut ancestors = collect_ancestor_chain(conn, &child)?;
    if ancestors.is_empty() {
        return Ok(Vec::new());
    }
    ancestors.reverse();

    let all_events = query_all_on_conn(conn, events::TODO)?;
    let policies = extract_delegation_policies(&all_events);
    let reservations = extract_delegation_reservations(&all_events);

    let mut recorded = Vec::new();
    let mut chain = ancestors.clone();
    chain.push(child.clone());

    // Check every ancestor edge along the chain for both per-child caps and cumulative total budget.
    for idx in 0..ancestors.len() {
        let parent = &chain[idx];
        let child_node = &chain[idx + 1];
        let Some(policy) =
            select_applicable_delegation_policy(&policies, &parent.id, &child_node.id)
        else {
            continue;
        };

        let units = requested_units
            .or(policy.max_child_budget_units)
            .unwrap_or(0);

        if let Some(per_child_cap) = policy.max_child_budget_units
            && units > per_child_cap
        {
            return Err(DecapodError::ValidationError(
                GrantRejectionReason::BudgetExceeded {
                    requested: units,
                    limit: per_child_cap,
                }
                .summary(),
            ));
        }

        if let Some(parent_total_cap) = policy.max_total_budget_units {
            let already_reserved =
                sum_active_reservations_for_parent(&reservations, &parent.id, Some(&child_node.id));
            if already_reserved.saturating_add(units) > parent_total_cap {
                return Err(DecapodError::ValidationError(
                    GrantRejectionReason::CumulativeBudgetExceeded {
                        parent_task_id: parent.id.clone(),
                        parent_budget: parent_total_cap,
                        already_reserved,
                        requested: units,
                    }
                    .summary(),
                ));
            }
            if units > 0 && idx + 1 == ancestors.len() {
                let res_id = format!(
                    "rsv_{}",
                    &sha256_hex(
                        format!("{}:{}:{}:{}:{}", parent.id, child.id, generation, units, ts)
                            .as_bytes()
                    )[..20]
                );
                let rec = DelegationReservationRecord {
                    reservation_id: res_id.clone(),
                    parent_task_id: parent.id.clone(),
                    child_task_id: child.id.clone(),
                    actor_id: actor.to_string(),
                    units,
                    generation,
                    resource: resource.map(str::to_string),
                    status: ReservationStatus::Reserved,
                };
                let event = serde_json::json!({
                    "ts": ts,
                    "event_id": res_id,
                    "event_type": DELEGATION_RESERVE_EVENT_TYPE,
                    "status": "success",
                    "task_id": parent.id,
                    "actor": actor,
                    "payload": rec,
                });
                append_on_conn(conn, events::TODO, &event)?;
                recorded.push(rec);
            }
        }
    }

    Ok(recorded)
}

/// Mark active delegation reservations held by `child_task_id` as `Consumed` (on done)
/// or `Released` (on explicit release/yield). Consumed reservations continue counting
/// against the parent's cumulative budget; Released reservations return capacity.
pub fn transition_child_reservations_on_conn(
    conn: &Connection,
    ts: &str,
    actor: &str,
    child_task_id: &str,
    new_status: ReservationStatus,
) -> Result<(), DecapodError> {
    if !conn.has_table("events")? {
        return Ok(());
    }
    let all_events = query_all_on_conn(conn, events::TODO)?;
    let reservations = extract_delegation_reservations(&all_events);
    for rec in reservations {
        if rec.child_task_id != child_task_id || rec.status == new_status {
            continue;
        }
        // Once consumed, do not release back to free cumulative budget.
        if rec.status == ReservationStatus::Consumed && new_status == ReservationStatus::Released {
            continue;
        }
        let mut updated = rec.clone();
        updated.status = new_status;
        let suffix = match new_status {
            ReservationStatus::Reserved => "rsv",
            ReservationStatus::Consumed => "cns",
            ReservationStatus::Released => "rel",
        };
        let event_id = format!(
            "rsv_{}_{}",
            suffix,
            &sha256_hex(
                format!(
                    "{}:{}:{}:{:?}",
                    rec.parent_task_id,
                    rec.child_task_id,
                    ts,
                    all_events.len()
                )
                .as_bytes()
            )[..16]
        );
        let event = serde_json::json!({
            "ts": ts,
            "event_id": event_id,
            "event_type": DELEGATION_RESERVE_EVENT_TYPE,
            "status": "success",
            "task_id": rec.parent_task_id,
            "actor": actor,
            "payload": updated,
        });
        append_on_conn(conn, events::TODO, &event)?;
    }
    Ok(())
}

fn compute_repo_fingerprint(main_root: &Path) -> String {
    let origin_url = git_stdout(main_root, &["config", "--get", "remote.origin.url"])
        .unwrap_or_else(|| "local-only".to_string());
    let root_commit = git_stdout(main_root, &["rev-list", "--max-parents=0", "HEAD"])
        .unwrap_or_else(|| "unborn".to_string());
    sha256_hex(format!("{origin_url}|{root_commit}").as_bytes())
}

fn compute_working_tree_digest(project_root: &Path) -> String {
    let status = git_stdout(project_root, &["status", "--porcelain"]).unwrap_or_default();
    sha256_hex(status.as_bytes())
}

fn git_stdout(repo_root: &Path, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new("git");
    for (k, _) in std::env::vars_os() {
        if k.to_string_lossy().starts_with("GIT_") {
            cmd.env_remove(k);
        }
    }
    let out = cmd.arg("-C").arg(repo_root).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn compute_grant_digest(grant: &EffectiveOperationGrant) -> String {
    let payload = serde_json::json!({
        "schema_version": grant.schema_version,
        "decision": grant.decision,
        "operation": grant.operation,
        "actor_id": grant.actor_id,
        "task_id": grant.task_id,
        "target_path": grant.target_path,
        "target_resource": grant.target_resource,
        "recipient": grant.recipient,
        "effective_allowances": grant.effective_allowances,
        "effective_denials": grant.effective_denials,
        "effective_max_budget_units": grant.effective_max_budget_units,
        "remaining_delegation_depth": grant.remaining_delegation_depth,
        "prestate_digest": grant.prestate.prestate_digest,
        "reasons": grant.reasons,
    });
    sha256_hex(payload.to_string().as_bytes())
}

fn load_risk_map(store_root: &Path) -> Option<RiskMap> {
    let risk_map_path = store_root.join("RISKMAP.json");
    if !risk_map_path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(risk_map_path).ok()?;
    serde_json::from_str::<RiskMap>(&content).ok()
}

fn is_task_lifecycle_operation(op: &str) -> bool {
    op.starts_with("task.") || op.starts_with("todo.")
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
#[path = "../../../tests/unit/core/effective_grant_tests.rs"]
mod tests;
