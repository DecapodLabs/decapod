//! Lease-aware multi-agent fleet coordination helpers (Houseboat 2).
//!
//! Decapod already owns todos, claims, agent presence, handoffs, and workspaces.
//! This module is the pure lease-graph stratum: generations, intent anchors,
//! lifecycle transitions, overlap detection, and fleet health projections.
//! It does not introduce a standalone fleet service.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Default exclusive-claim lease duration (30 minutes).
/// Aligns with agent presence eviction so lease expiry and stale presence agree.
pub const DEFAULT_CLAIM_LEASE_SECS: u64 = 30 * 60;

/// Maximum lease duration an agent may request or renew (24 hours).
pub const MAX_CLAIM_LEASE_SECS: u64 = 24 * 60 * 60;

/// Soft capacity for concurrent exclusive leases before fleet pressure rises.
pub const DEFAULT_FLEET_SLOT_CAPACITY: u32 = 8;

/// Leases expiring within this many seconds are reported as expiry risk.
pub const EXPIRY_RISK_WINDOW_SECS: u64 = 5 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeaseState {
    /// Claim has no lease timestamp (legacy assignment).
    Unspecified,
    /// Lease is active and not yet expired.
    Active,
    /// Lease timestamp is in the past; reclaim is allowed.
    Expired,
}

/// Explicit lease lifecycle for the Houseboat lease graph.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeaseLifecycle {
    /// No lifecycle recorded (legacy).
    #[default]
    Unspecified,
    Claimed,
    Extended,
    Yielded,
    Expired,
    Released,
    Reclaimed,
}

impl LeaseLifecycle {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unspecified => "unspecified",
            Self::Claimed => "claimed",
            Self::Extended => "extended",
            Self::Yielded => "yielded",
            Self::Expired => "expired",
            Self::Released => "released",
            Self::Reclaimed => "reclaimed",
        }
    }

    pub fn parse(raw: Option<&str>) -> Self {
        match raw.map(str::trim).unwrap_or("") {
            "" | "unspecified" => Self::Unspecified,
            "claimed" => Self::Claimed,
            "extended" => Self::Extended,
            "yielded" => Self::Yielded,
            "expired" => Self::Expired,
            "released" => Self::Released,
            "reclaimed" => Self::Reclaimed,
            _ => Self::Unspecified,
        }
    }

    /// Active exclusive custody that must not publish/complete without proof.
    pub fn holds_exclusive_custody(self) -> bool {
        matches!(self, Self::Claimed | Self::Extended | Self::Reclaimed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveClaimView {
    pub task_id: String,
    pub agent_id: String,
    pub scope: String,
    pub category: String,
    pub dir_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_expires_at: Option<String>,
    pub lease_state: LeaseState,
    pub assigned_at: Option<String>,
    /// Monotonic lease generation; reclaim/issue increments.
    #[serde(default)]
    pub lease_generation: u32,
    #[serde(default)]
    pub lease_lifecycle: LeaseLifecycle,
    /// Citable intent anchor (trajectory intent, managed-spec ref, or todo intent).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub intent_anchor: String,
    /// Reserved capacity units (1 = one exclusive workunit slot).
    #[serde(default = "default_capacity_units")]
    pub capacity_units: u32,
    /// Current prerequisite/proof readiness for this workunit.
    #[serde(default)]
    pub dependency_readiness: DependencyReadiness,
}

fn default_capacity_units() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimOverlap {
    pub task_id: String,
    pub agent_id: String,
    pub reason: String,
    pub surface: String,
    pub expected: String,
    pub observed: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapacityPressure {
    None,
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetCapacityView {
    pub claimed_slots: u32,
    pub max_slots: u32,
    pub pressure: CapacityPressure,
    pub reserved_units: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetHealthProjection {
    pub active_claims: Vec<ActiveClaimView>,
    pub expired_leases: Vec<ActiveClaimView>,
    pub yielded_claims: Vec<ActiveClaimView>,
    pub expiry_risk: Vec<ActiveClaimView>,
    pub dependency_blocked_claims: Vec<ActiveClaimView>,
    pub overlaps: Vec<ClaimOverlap>,
    pub intent_anchors: Vec<String>,
    pub capacity: FleetCapacityView,
    pub agent_count: usize,
    pub claim_count: usize,
    pub expired_count: usize,
    pub overlap_count: usize,
    pub yielded_count: usize,
    pub expiry_risk_count: usize,
    pub dependency_blocked_count: usize,
    pub proof_blocked_count: usize,
}

/// Deterministic dependency gate state for a todo/workunit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyReadinessState {
    #[default]
    Ready,
    Waiting,
    Missing,
    ProofBlocked,
    Cycle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyNodeView {
    pub task_id: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_status: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proof_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyBlocker {
    pub task_id: String,
    pub reason: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_status: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyReadiness {
    #[serde(default)]
    pub state: DependencyReadinessState,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<DependencyNodeView>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blockers: Vec<DependencyBlocker>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cycle: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proof_refs: Vec<String>,
}

impl DependencyReadiness {
    pub fn ready() -> Self {
        Self::default()
    }

    pub fn is_ready(&self) -> bool {
        self.state == DependencyReadinessState::Ready
    }
}

fn verification_passed(status: Option<&str>) -> bool {
    status
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "pass" | "passed" | "verified"
            )
        })
        .unwrap_or(false)
}

/// Evaluate direct prerequisite state and any cycle reachable from `task_id`.
///
/// `edges` contains the full reachable graph as `(task, prerequisite)` pairs;
/// `dependencies` contains the resolved direct prerequisite records. Both are
/// canonicalized here so database row order cannot affect the projection.
pub fn evaluate_dependency_readiness(
    task_id: &str,
    mut dependencies: Vec<DependencyNodeView>,
    edges: &[(String, String)],
) -> DependencyReadiness {
    dependencies.sort_by(|left, right| left.task_id.cmp(&right.task_id));
    dependencies.dedup_by(|left, right| left.task_id == right.task_id);

    let cycle = find_reachable_dependency_cycle(task_id, edges);
    let mut blockers = Vec::new();
    let mut proof_refs = Vec::new();
    for dependency in &mut dependencies {
        dependency.proof_refs.sort();
        dependency.proof_refs.dedup();
        proof_refs.extend(dependency.proof_refs.iter().cloned());
        let (reason, blocked) = if dependency.status == "missing" {
            ("dependency_missing", true)
        } else if dependency.status != "done" {
            ("dependency_not_complete", true)
        } else if !verification_passed(dependency.validation_status.as_deref()) {
            ("dependency_proof_not_passed", true)
        } else {
            ("", false)
        };
        if blocked {
            blockers.push(DependencyBlocker {
                task_id: dependency.task_id.clone(),
                reason: reason.to_string(),
                status: dependency.status.clone(),
                validation_status: dependency.validation_status.clone(),
            });
        }
    }
    blockers.sort_by(|left, right| {
        left.task_id
            .cmp(&right.task_id)
            .then(left.reason.cmp(&right.reason))
    });
    proof_refs.sort();
    proof_refs.dedup();

    let state = if !cycle.is_empty() {
        DependencyReadinessState::Cycle
    } else if blockers
        .iter()
        .any(|blocker| blocker.reason == "dependency_missing")
    {
        DependencyReadinessState::Missing
    } else if blockers
        .iter()
        .any(|blocker| blocker.reason == "dependency_not_complete")
    {
        DependencyReadinessState::Waiting
    } else if blockers
        .iter()
        .any(|blocker| blocker.reason == "dependency_proof_not_passed")
    {
        DependencyReadinessState::ProofBlocked
    } else {
        DependencyReadinessState::Ready
    };

    DependencyReadiness {
        state,
        dependencies,
        blockers,
        cycle,
        proof_refs,
    }
}

fn find_reachable_dependency_cycle(task_id: &str, edges: &[(String, String)]) -> Vec<String> {
    let mut graph: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (task, dependency) in edges {
        graph
            .entry(task.as_str())
            .or_default()
            .push(dependency.as_str());
    }
    for dependencies in graph.values_mut() {
        dependencies.sort_unstable();
        dependencies.dedup();
    }

    fn visit<'a>(
        node: &'a str,
        graph: &BTreeMap<&'a str, Vec<&'a str>>,
        visited: &mut BTreeSet<&'a str>,
        stack: &mut Vec<&'a str>,
        active: &mut BTreeSet<&'a str>,
    ) -> Option<Vec<String>> {
        if active.contains(node) {
            let start = stack.iter().position(|entry| *entry == node).unwrap_or(0);
            let mut cycle: Vec<String> = stack[start..]
                .iter()
                .map(|entry| (*entry).to_string())
                .collect();
            cycle.push(node.to_string());
            return Some(cycle);
        }
        if !visited.insert(node) {
            return None;
        }
        active.insert(node);
        stack.push(node);
        if let Some(dependencies) = graph.get(node) {
            for dependency in dependencies {
                if let Some(cycle) = visit(dependency, graph, visited, stack, active) {
                    return Some(cycle);
                }
            }
        }
        stack.pop();
        active.remove(node);
        None
    }

    visit(
        task_id,
        &graph,
        &mut BTreeSet::new(),
        &mut Vec::new(),
        &mut BTreeSet::new(),
    )
    .unwrap_or_default()
}

/// Parse Decapod epoch-Z timestamps (`{unix_secs}Z`).
pub fn parse_epoch_z(ts: &str) -> Option<u64> {
    ts.trim_end_matches('Z').parse::<u64>().ok()
}

/// Format unix seconds as epoch-Z.
pub fn format_epoch_z(secs: u64) -> String {
    format!("{secs}Z")
}

/// Compute a lease expiry timestamp from now + lease_seconds (clamped).
pub fn lease_expires_at(now_secs: u64, lease_seconds: u64) -> String {
    let secs = lease_seconds.clamp(1, MAX_CLAIM_LEASE_SECS);
    format_epoch_z(now_secs.saturating_add(secs))
}

/// Classify lease state relative to `now_ts` (epoch-Z).
pub fn lease_state(lease_expires_at: Option<&str>, now_ts: &str) -> LeaseState {
    let Some(exp) = lease_expires_at.filter(|s| !s.trim().is_empty()) else {
        return LeaseState::Unspecified;
    };
    let Some(exp_secs) = parse_epoch_z(exp) else {
        return LeaseState::Unspecified;
    };
    let Some(now_secs) = parse_epoch_z(now_ts) else {
        return LeaseState::Unspecified;
    };
    if now_secs >= exp_secs {
        LeaseState::Expired
    } else {
        LeaseState::Active
    }
}

/// True when an exclusive claim may be reclaimed by another agent.
///
/// Reclaim is allowed when the lease is explicitly expired. Unspecified leases
/// fall back to agent-presence staleness (handled by the caller).
pub fn lease_allows_reclaim(lease_expires_at: Option<&str>, now_ts: &str) -> bool {
    matches!(lease_state(lease_expires_at, now_ts), LeaseState::Expired)
}

/// Issue a new exclusive lease generation (claim or post-reclaim).
pub fn next_lease_generation(previous: u32, reclaimed: bool) -> u32 {
    if previous == 0 {
        1
    } else if reclaimed {
        previous.saturating_add(1)
    } else {
        // Same-agent re-claim keeps generation until an explicit extend.
        previous.max(1)
    }
}

/// Extend increments generation and marks lifecycle extended.
pub fn extended_generation(current: u32) -> u32 {
    current.max(1).saturating_add(1)
}

/// Handoff transfers exclusive custody to a different agent.
///
/// Generation always advances so concurrent observations cannot confuse the
/// prior holder's lease with the receiver's lease. A zero (legacy) generation
/// becomes 1 on the first exclusive handoff.
pub fn handoff_lease_generation(previous: u32) -> u32 {
    next_lease_generation(previous, true)
}

/// Pure lease-graph outcome for a successful exclusive handoff transfer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffLeaseTransfer {
    pub previous_agent: String,
    pub next_agent: String,
    pub prior_generation: u32,
    pub next_generation: u32,
    pub prior_lifecycle: LeaseLifecycle,
    pub next_lifecycle: LeaseLifecycle,
    pub intent_anchor: String,
    pub lease_expires_at: String,
    pub lease_seconds: u64,
}

/// Compute the lease fields that must be written on a successful handoff.
pub fn plan_handoff_lease_transfer(
    previous_agent: &str,
    next_agent: &str,
    prior_generation: u32,
    prior_lifecycle: LeaseLifecycle,
    intent_anchor: &str,
    now_secs: u64,
    lease_seconds: u64,
) -> HandoffLeaseTransfer {
    let lease_seconds = lease_seconds.clamp(1, MAX_CLAIM_LEASE_SECS);
    let intent = if intent_anchor.trim().is_empty() {
        String::new()
    } else {
        intent_anchor.trim().to_string()
    };
    HandoffLeaseTransfer {
        previous_agent: previous_agent.to_string(),
        next_agent: next_agent.to_string(),
        prior_generation,
        next_generation: handoff_lease_generation(prior_generation),
        prior_lifecycle,
        next_lifecycle: LeaseLifecycle::Claimed,
        intent_anchor: intent,
        lease_expires_at: lease_expires_at(now_secs, lease_seconds),
        lease_seconds,
    }
}

/// Default intent anchor when none is supplied by the caller.
pub fn default_intent_anchor(task_id: &str) -> String {
    format!("intent:todo:{task_id}")
}

/// Normalize a repository-relative or absolute path for prefix comparison.
pub fn normalize_path(path: &str) -> String {
    let trimmed = path.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(trimmed.len());
    let mut prev_slash = false;
    for ch in trimmed.chars() {
        if ch == '/' {
            if !prev_slash {
                out.push(ch);
            }
            prev_slash = true;
        } else {
            out.push(ch);
            prev_slash = false;
        }
    }
    out
}

/// True when two paths share a hierarchical prefix (file/module claim conflict).
pub fn paths_overlap(a: &str, b: &str) -> bool {
    let a = normalize_path(a);
    let b = normalize_path(b);
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    let a_prefix = format!("{a}/");
    let b_prefix = format!("{b}/");
    b.starts_with(&a_prefix) || a.starts_with(&b_prefix)
}

/// True when two non-root scopes collide (domain claim conflict).
pub fn scopes_overlap(a: &str, b: &str) -> bool {
    let a = a.trim();
    let b = b.trim();
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == "root" || b == "root" || a == "workspace" || b == "workspace" {
        return false;
    }
    a.eq_ignore_ascii_case(b)
}

/// True when two non-empty categories collide (domain claim conflict).
pub fn categories_overlap(a: &str, b: &str) -> bool {
    let a = a.trim();
    let b = b.trim();
    if a.is_empty() || b.is_empty() {
        return false;
    }
    a.eq_ignore_ascii_case(b)
}

/// Detect overlaps between a candidate claim and currently active exclusive claims.
pub fn detect_overlaps(
    candidate_task_id: &str,
    candidate_agent: &str,
    candidate_scope: &str,
    candidate_category: &str,
    candidate_dir: &str,
    active: &[ActiveClaimView],
    now_ts: &str,
) -> Vec<ClaimOverlap> {
    let mut out = Vec::new();
    for claim in active {
        if claim.task_id == candidate_task_id {
            continue;
        }
        if claim.agent_id == candidate_agent {
            continue;
        }
        // Yielded and expired leases do not block new exclusive claims.
        if matches!(
            claim.lease_lifecycle,
            LeaseLifecycle::Yielded | LeaseLifecycle::Released | LeaseLifecycle::Expired
        ) {
            continue;
        }
        if matches!(
            lease_state(claim.lease_expires_at.as_deref(), now_ts),
            LeaseState::Expired
        ) {
            continue;
        }

        if categories_overlap(candidate_category, &claim.category) {
            out.push(ClaimOverlap {
                task_id: claim.task_id.clone(),
                agent_id: claim.agent_id.clone(),
                reason: "category_overlap".to_string(),
                surface: "category".to_string(),
                expected: candidate_category.to_string(),
                observed: claim.category.clone(),
            });
        }
        if scopes_overlap(candidate_scope, &claim.scope) {
            out.push(ClaimOverlap {
                task_id: claim.task_id.clone(),
                agent_id: claim.agent_id.clone(),
                reason: "scope_overlap".to_string(),
                surface: "scope".to_string(),
                expected: candidate_scope.to_string(),
                observed: claim.scope.clone(),
            });
        }
        if paths_overlap(candidate_dir, &claim.dir_path) {
            out.push(ClaimOverlap {
                task_id: claim.task_id.clone(),
                agent_id: claim.agent_id.clone(),
                reason: "path_overlap".to_string(),
                surface: "dir_path".to_string(),
                expected: candidate_dir.to_string(),
                observed: claim.dir_path.clone(),
            });
        }
    }
    out.sort_by(|a, b| {
        a.task_id
            .cmp(&b.task_id)
            .then(a.reason.cmp(&b.reason))
            .then(a.surface.cmp(&b.surface))
    });
    out.dedup_by(|a, b| {
        a.task_id == b.task_id
            && a.agent_id == b.agent_id
            && a.reason == b.reason
            && a.surface == b.surface
    });
    out
}

/// Pairwise overlaps among active (non-expired, non-yielded) claims.
pub fn detect_fleet_overlaps(active: &[ActiveClaimView], now_ts: &str) -> Vec<ClaimOverlap> {
    let live: Vec<&ActiveClaimView> = active
        .iter()
        .filter(|c| {
            !matches!(
                c.lease_lifecycle,
                LeaseLifecycle::Yielded | LeaseLifecycle::Released | LeaseLifecycle::Expired
            ) && !matches!(
                lease_state(c.lease_expires_at.as_deref(), now_ts),
                LeaseState::Expired
            )
        })
        .collect();

    let mut out = Vec::new();
    for (i, left) in live.iter().enumerate() {
        for right in live.iter().skip(i + 1) {
            if left.agent_id == right.agent_id {
                continue;
            }
            if categories_overlap(&left.category, &right.category) {
                out.push(ClaimOverlap {
                    task_id: right.task_id.clone(),
                    agent_id: right.agent_id.clone(),
                    reason: "category_overlap".to_string(),
                    surface: "category".to_string(),
                    expected: left.category.clone(),
                    observed: right.category.clone(),
                });
            }
            if scopes_overlap(&left.scope, &right.scope) {
                out.push(ClaimOverlap {
                    task_id: right.task_id.clone(),
                    agent_id: right.agent_id.clone(),
                    reason: "scope_overlap".to_string(),
                    surface: "scope".to_string(),
                    expected: left.scope.clone(),
                    observed: right.scope.clone(),
                });
            }
            if paths_overlap(&left.dir_path, &right.dir_path) {
                out.push(ClaimOverlap {
                    task_id: right.task_id.clone(),
                    agent_id: right.agent_id.clone(),
                    reason: "path_overlap".to_string(),
                    surface: "dir_path".to_string(),
                    expected: left.dir_path.clone(),
                    observed: right.dir_path.clone(),
                });
            }
        }
    }
    out.sort_by(|a, b| {
        a.task_id
            .cmp(&b.task_id)
            .then(a.reason.cmp(&b.reason))
            .then(a.agent_id.cmp(&b.agent_id))
    });
    out
}

fn capacity_pressure(claimed: u32, max_slots: u32) -> CapacityPressure {
    if max_slots == 0 {
        return CapacityPressure::Critical;
    }
    let pct = (claimed as u64 * 100) / max_slots as u64;
    match pct {
        0 => CapacityPressure::None,
        1..=49 => CapacityPressure::Low,
        50..=74 => CapacityPressure::Medium,
        75..=99 => CapacityPressure::High,
        _ => CapacityPressure::Critical,
    }
}

fn within_expiry_risk(lease_expires_at: Option<&str>, now_ts: &str) -> bool {
    let Some(exp) = lease_expires_at.and_then(parse_epoch_z) else {
        return false;
    };
    let Some(now) = parse_epoch_z(now_ts) else {
        return false;
    };
    if exp <= now {
        return false;
    }
    exp - now <= EXPIRY_RISK_WINDOW_SECS
}

/// Build a deterministic fleet health projection from exclusive claim records.
pub fn project_fleet_health(claims: Vec<ActiveClaimView>, now_ts: &str) -> FleetHealthProjection {
    project_fleet_health_with_capacity(claims, now_ts, DEFAULT_FLEET_SLOT_CAPACITY)
}

pub fn project_fleet_health_with_capacity(
    claims: Vec<ActiveClaimView>,
    now_ts: &str,
    max_slots: u32,
) -> FleetHealthProjection {
    let mut active_claims = claims;
    for claim in &mut active_claims {
        claim.lease_state = lease_state(claim.lease_expires_at.as_deref(), now_ts);
        if claim.capacity_units == 0 {
            claim.capacity_units = 1;
        }
    }
    active_claims.sort_by(|a, b| a.task_id.cmp(&b.task_id));

    let expired_leases: Vec<ActiveClaimView> = active_claims
        .iter()
        .filter(|c| {
            c.lease_state == LeaseState::Expired || c.lease_lifecycle == LeaseLifecycle::Expired
        })
        .cloned()
        .collect();
    let yielded_claims: Vec<ActiveClaimView> = active_claims
        .iter()
        .filter(|c| c.lease_lifecycle == LeaseLifecycle::Yielded)
        .cloned()
        .collect();
    let expiry_risk: Vec<ActiveClaimView> = active_claims
        .iter()
        .filter(|c| {
            c.lease_lifecycle.holds_exclusive_custody()
                && within_expiry_risk(c.lease_expires_at.as_deref(), now_ts)
        })
        .cloned()
        .collect();
    let dependency_blocked_claims: Vec<ActiveClaimView> = active_claims
        .iter()
        .filter(|claim| !claim.dependency_readiness.is_ready())
        .cloned()
        .collect();
    let proof_blocked_count = dependency_blocked_claims
        .iter()
        .filter(|claim| claim.dependency_readiness.state == DependencyReadinessState::ProofBlocked)
        .count();
    let overlaps = detect_fleet_overlaps(&active_claims, now_ts);

    let reserved_units: u32 = active_claims
        .iter()
        .filter(|c| {
            c.lease_lifecycle.holds_exclusive_custody() && c.lease_state != LeaseState::Expired
        })
        .map(|c| c.capacity_units.max(1))
        .sum();
    let claimed_slots = reserved_units;
    let capacity = FleetCapacityView {
        claimed_slots,
        max_slots,
        pressure: capacity_pressure(claimed_slots, max_slots),
        reserved_units,
    };

    let mut intent_anchors: Vec<String> = active_claims
        .iter()
        .map(|c| c.intent_anchor.clone())
        .filter(|a| !a.is_empty())
        .collect();
    intent_anchors.sort();
    intent_anchors.dedup();

    let agent_count = {
        let mut agents: Vec<&str> = active_claims
            .iter()
            .map(|c| c.agent_id.as_str())
            .filter(|a| !a.is_empty())
            .collect();
        agents.sort_unstable();
        agents.dedup();
        agents.len()
    };

    FleetHealthProjection {
        claim_count: active_claims.len(),
        expired_count: expired_leases.len(),
        overlap_count: overlaps.len(),
        yielded_count: yielded_claims.len(),
        expiry_risk_count: expiry_risk.len(),
        dependency_blocked_count: dependency_blocked_claims.len(),
        proof_blocked_count,
        agent_count,
        active_claims,
        expired_leases,
        yielded_claims,
        expiry_risk,
        dependency_blocked_claims,
        overlaps,
        intent_anchors,
        capacity,
    }
}

/// Proof is required before exclusive-lease completion.
pub fn exclusive_lease_requires_proof(lifecycle: LeaseLifecycle, assigned: bool) -> bool {
    assigned && lifecycle.holds_exclusive_custody()
}

#[cfg(test)]
#[path = "../../../tests/unit/core/fleet_coord_tests.rs"]
mod tests;
