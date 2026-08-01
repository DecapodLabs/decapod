//! Lease-aware multi-agent fleet coordination helpers (Houseboat 2).
//!
//! Decapod already owns todos, claims, agent presence, handoffs, and workspaces.
//! This module adds pure lease/overlap projections so concurrent agents can
//! detect expired ownership, reclaim safely, and surface domain/path conflicts
//! without introducing a standalone fleet service.

use serde::{Deserialize, Serialize};

/// Default exclusive-claim lease duration (30 minutes).
/// Aligns with agent presence eviction so lease expiry and stale presence agree.
pub const DEFAULT_CLAIM_LEASE_SECS: u64 = 30 * 60;

/// Maximum lease duration an agent may request or renew (24 hours).
pub const MAX_CLAIM_LEASE_SECS: u64 = 24 * 60 * 60;

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
pub struct FleetHealthProjection {
    pub active_claims: Vec<ActiveClaimView>,
    pub expired_leases: Vec<ActiveClaimView>,
    pub overlaps: Vec<ClaimOverlap>,
    pub agent_count: usize,
    pub claim_count: usize,
    pub expired_count: usize,
    pub overlap_count: usize,
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

/// Normalize a repository-relative or absolute path for prefix comparison.
pub fn normalize_path(path: &str) -> String {
    let trimmed = path.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return String::new();
    }
    // Collapse duplicate slashes without requiring a real filesystem.
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
///
/// Empty paths never overlap. Identical paths always overlap.
pub fn paths_overlap(a: &str, b: &str) -> bool {
    let a = normalize_path(a);
    let b = normalize_path(b);
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    // Require boundary-aware prefix so `src/foo` does not match `src/foobar`.
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
///
/// Skips the candidate's own task id and claims held by the same agent. Skips
/// expired leases so reclaimers are not blocked by abandoned ownership.
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

/// Pairwise overlaps among active (non-expired) claims held by different agents.
pub fn detect_fleet_overlaps(active: &[ActiveClaimView], now_ts: &str) -> Vec<ClaimOverlap> {
    let live: Vec<&ActiveClaimView> = active
        .iter()
        .filter(|c| {
            !matches!(
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

/// Build a deterministic fleet health projection from active exclusive claims.
pub fn project_fleet_health(claims: Vec<ActiveClaimView>, now_ts: &str) -> FleetHealthProjection {
    let mut active_claims = claims;
    for claim in &mut active_claims {
        claim.lease_state = lease_state(claim.lease_expires_at.as_deref(), now_ts);
    }
    active_claims.sort_by(|a, b| a.task_id.cmp(&b.task_id));

    let expired_leases: Vec<ActiveClaimView> = active_claims
        .iter()
        .filter(|c| c.lease_state == LeaseState::Expired)
        .cloned()
        .collect();
    let overlaps = detect_fleet_overlaps(&active_claims, now_ts);
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
        agent_count,
        active_claims,
        expired_leases,
        overlaps,
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/core/fleet_coord_tests.rs"]
mod tests;
