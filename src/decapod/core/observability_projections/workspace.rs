//! Governed workunit observability, live workspace evolution, bounded file
//! inventory, immutable source anchors, and shared human/machine renderer
//! (#860, #1714, #1723, #1727, #1728).

use crate::core::error::DecapodError;
use crate::core::fleet_coord::{
    ActiveClaimView, CapacityPressure, LeaseLifecycle, LeaseState, detect_fleet_overlaps,
};
use crate::core::governance_document::{self, GOVERNANCE_PATH};
use crate::core::observability_projections::snapshot::{
    DEFAULT_MAX_ANCHOR_BYTES, DEFAULT_MAX_INVENTORY_FILES, LiveWorkspaceInventory,
    OBSERVE_SCHEMA_VERSION, ObservationSnapshot, SourceAnchor, TaskRecordView,
    capture_observation_with, enumerate_live_workspace_inventory, read_source_anchor,
    read_tasks_readonly,
};
use crate::core::observability_projections::specs::{
    IntentLineageProjection, ReceiptIntegrityProjection, extract_intent_lineage,
    inspect_validation_receipt,
};
use crate::core::projection_budget::{
    ProjectionBudget, WorktreeProjectionEntry, WorktreeProjectionQuery, project_worktrees_read_only,
};
use crate::core::time;
use crate::core::trace;
use crate::core::trajectory::{self, TrajectoryMotionState};
use crate::plan_governance::GovernedPlan;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceProjectionQuery {
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_workspace_limit")]
    pub limit: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_path: Option<String>,
    #[serde(default)]
    pub start_line: usize,
    #[serde(default)]
    pub end_line: usize,
    #[serde(default = "default_max_anchor_bytes")]
    pub max_anchor_bytes: usize,
}

impl Default for WorkspaceProjectionQuery {
    fn default() -> Self {
        Self {
            offset: 0,
            limit: default_workspace_limit(),
            anchor_path: None,
            start_line: 1,
            end_line: 0,
            max_anchor_bytes: default_max_anchor_bytes(),
        }
    }
}

fn default_workspace_limit() -> usize {
    DEFAULT_MAX_INVENTORY_FILES
}

fn default_max_anchor_bytes() -> usize {
    DEFAULT_MAX_ANCHOR_BYTES
}

/// Canonical workunit activity categories (#860).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum WorkunitActivityCategory {
    Active,
    Waiting,
    Blocked,
    Drifting,
    Completed,
    Abandoned,
}

impl WorkunitActivityCategory {
    /// Typed transition rules for governed workunit activity states (#860).
    pub fn allowed_transitions(self) -> &'static [WorkunitActivityCategory] {
        use WorkunitActivityCategory::*;
        match self {
            Waiting => &[Active, Blocked, Abandoned],
            Active => &[Waiting, Blocked, Drifting, Completed, Abandoned],
            Blocked => &[Waiting, Active, Drifting, Abandoned],
            Drifting => &[Active, Blocked, Abandoned],
            Completed => &[],
            Abandoned => &[Waiting],
        }
    }

    pub fn can_transition_to(self, next: WorkunitActivityCategory) -> bool {
        self == next || self.allowed_transitions().contains(&next)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Waiting => "waiting",
            Self::Blocked => "blocked",
            Self::Drifting => "drifting",
            Self::Completed => "completed",
            Self::Abandoned => "abandoned",
        }
    }
}

/// Authoritative vs stale/inferred status distinction (#860).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StatusAuthorityClass {
    Authoritative,
    StaleInput,
    Inferred,
}

/// Deterministic input to classify one workunit's activity category (#860).
#[derive(Debug, Clone)]
pub struct WorkunitClassificationInput {
    pub task_status: String,
    pub owner: String,
    pub lease_lifecycle: String,
    pub lease_expired: bool,
    pub has_dependency_blockers: bool,
    pub has_overlap_contention: bool,
    pub validation_failed: bool,
    pub receipt_stale: bool,
    pub trajectory_motion: Option<TrajectoryMotionState>,
    pub unresolved_claims_count: usize,
    pub proof_verified: bool,
}

/// Pure, deterministic classifier for #860 workunit categories.
/// Never infers healthy progress from heartbeats alone!
pub fn classify_workunit_activity(
    input: &WorkunitClassificationInput,
) -> (WorkunitActivityCategory, StatusAuthorityClass, Vec<String>) {
    let status_lower = input.task_status.to_ascii_lowercase();
    let mut reasons = Vec::new();

    if matches!(status_lower.as_str(), "done" | "completed" | "closed") {
        if input.proof_verified && !input.receipt_stale {
            return (
                WorkunitActivityCategory::Completed,
                StatusAuthorityClass::Authoritative,
                vec!["task completed with fresh verified receipt".to_string()],
            );
        }
        reasons.push("task marked done but validation receipt is missing or stale".to_string());
        return (
            WorkunitActivityCategory::Drifting,
            StatusAuthorityClass::StaleInput,
            reasons,
        );
    }

    if matches!(status_lower.as_str(), "abandoned" | "cancelled")
        || matches!(
            input.trajectory_motion,
            Some(TrajectoryMotionState::Abandoned)
        )
    {
        reasons.push("task or trajectory explicitly abandoned".to_string());
        return (
            WorkunitActivityCategory::Abandoned,
            StatusAuthorityClass::Authoritative,
            reasons,
        );
    }

    if input.has_dependency_blockers
        || input.has_overlap_contention
        || input.validation_failed
        || matches!(
            input.trajectory_motion,
            Some(TrajectoryMotionState::Blocked)
        )
    {
        if input.has_dependency_blockers {
            reasons.push("unresolved prerequisite dependencies".to_string());
        }
        if input.has_overlap_contention {
            reasons.push("scope overlap contention with concurrent agent lease".to_string());
        }
        if input.validation_failed {
            reasons.push("validation receipt reports failures".to_string());
        }
        return (
            WorkunitActivityCategory::Blocked,
            StatusAuthorityClass::Authoritative,
            reasons,
        );
    }

    if input.lease_expired
        || input.receipt_stale
        || matches!(
            input.trajectory_motion,
            Some(TrajectoryMotionState::Stalled)
        )
        || (input.unresolved_claims_count > 0 && input.owner.trim().is_empty())
    {
        if input.lease_expired {
            reasons.push("claim lease expired".to_string());
        }
        if input.receipt_stale {
            reasons.push("validation evidence stale against current HEAD".to_string());
        }
        if matches!(
            input.trajectory_motion,
            Some(TrajectoryMotionState::Stalled)
        ) {
            reasons.push("trajectory reports repeated failed loops (stalled)".to_string());
        }
        return (
            WorkunitActivityCategory::Drifting,
            StatusAuthorityClass::StaleInput,
            reasons,
        );
    }

    if input.owner.trim().is_empty()
        || input.lease_lifecycle.eq_ignore_ascii_case("yielded")
        || matches!(
            input.trajectory_motion,
            Some(TrajectoryMotionState::Waiting)
        )
    {
        reasons.push("unclaimed or yielded awaiting owner".to_string());
        return (
            WorkunitActivityCategory::Waiting,
            StatusAuthorityClass::Authoritative,
            reasons,
        );
    }

    reasons.push("actively owned with valid lease".to_string());
    (
        WorkunitActivityCategory::Active,
        StatusAuthorityClass::Authoritative,
        reasons,
    )
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkunitStatusProjection {
    pub task_id: String,
    pub title: String,
    pub category: WorkunitActivityCategory,
    pub authority_class: StatusAuthorityClass,
    pub allowed_next_categories: Vec<WorkunitActivityCategory>,
    pub owner: Option<String>,
    pub lease_generation: i64,
    pub lease_lifecycle: String,
    pub contention_overlaps: Vec<String>,
    pub dependency_blockers: Vec<String>,
    pub phase_readiness: String,
    pub validation_failure_count: u32,
    pub unresolved_claims_count: usize,
    pub evidence_coverage_pct: u8,
    pub stale_inputs: Vec<String>,
    pub source_refs: Vec<String>,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FleetObservabilitySummary {
    pub total_workunits: usize,
    pub active_count: usize,
    pub waiting_count: usize,
    pub blocked_count: usize,
    pub drifting_count: usize,
    pub completed_count: usize,
    pub abandoned_count: usize,
    pub overlap_count: usize,
    pub budget_pressure: CapacityPressure,
    pub stale_source_detected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClaimedEffectRecord {
    pub claim_id: String,
    pub statement: String,
    pub falsifier: String,
    pub status: String,
    pub proof_refs: Vec<String>,
    /// Explicitly false: agent-authored claims are never kernel proofs (#1723).
    pub is_kernel_proof: bool,
    /// Human-intent semantic alignment remains `not_proven` without independent evidence.
    pub semantic_alignment: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceEvolutionRecord {
    pub intent_lineage: IntentLineageProjection,
    pub claimed_effects: Vec<ClaimedEffectRecord>,
    pub validation_receipt: ReceiptIntegrityProjection,
    pub trajectory_motion_state: Option<String>,
    pub working_tree_dirty: bool,
    pub unresolved_claims_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceObservationProjection {
    pub schema_version: String,
    pub read_only: bool,
    pub authoritative: bool,
    pub snapshot: ObservationSnapshot,
    pub fleet_summary: FleetObservabilitySummary,
    pub workunits: Vec<WorkunitStatusProjection>,
    pub evolution: WorkspaceEvolutionRecord,
    pub inventory: LiveWorkspaceInventory,
    pub worktrees: Vec<WorktreeProjectionEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_anchor: Option<SourceAnchor>,
}

/// Project live workspace inventory, workunit activity (#860), evolution (#1723),
/// and snapshot consistency (#1727, #1728).
pub fn project_workspace_observation(
    repo_root: &Path,
    query: &WorkspaceProjectionQuery,
) -> Result<WorkspaceObservationProjection, DecapodError> {
    let now_ts = time::now_epoch_z();
    let (snapshot, inner) = capture_observation_with(repo_root, |root, data_root| {
        let doc = governance_document::load(root).ok().flatten();
        let traj = trajectory::load_trajectory_cookie(root).ok().flatten();
        let tasks = read_tasks_readonly(data_root, 256);
        let worktrees = project_worktrees_read_only(
            root,
            &WorktreeProjectionQuery {
                after_worktree_path: None,
                include_dirty_summary: false,
                budget: ProjectionBudget::default(),
            },
        )
        .map(|p| p.items)
        .unwrap_or_default();
        Ok((doc, traj, tasks, worktrees))
    })?;

    let (doc, traj, tasks, worktrees) = inner;
    let inventory = enumerate_live_workspace_inventory(
        repo_root,
        &snapshot.snapshot_id,
        query.offset,
        query.limit,
    );

    let plan_opt: Option<GovernedPlan> = doc
        .as_ref()
        .and_then(|d| d.sections.get("plan"))
        .and_then(|v| serde_json::from_value(v.clone()).ok());
    let intent_lineage = extract_intent_lineage(doc.as_ref(), plan_opt.as_ref());
    let receipt =
        inspect_validation_receipt(doc.as_ref(), snapshot.worktree.head_commit.as_deref());

    let claimed_effects: Vec<ClaimedEffectRecord> = doc
        .as_ref()
        .map(|d| {
            d.claims
                .iter()
                .map(|(id, c)| ClaimedEffectRecord {
                    claim_id: id.clone(),
                    statement: trace::redact_string(&c.statement),
                    falsifier: trace::redact_string(&c.falsifier),
                    status: c.status.clone(),
                    proof_refs: c.proof_refs.clone(),
                    is_kernel_proof: false,
                    semantic_alignment: "not_proven".to_string(),
                })
                .collect()
        })
        .unwrap_or_default();
    let total_claims = claimed_effects.len();
    let proven_claims = claimed_effects
        .iter()
        .filter(|c| !c.proof_refs.is_empty() && c.status.eq_ignore_ascii_case("verified"))
        .count();
    let unresolved_claims_count = total_claims.saturating_sub(proven_claims);
    let evidence_coverage_pct = match (proven_claims * 100).checked_div(total_claims) {
        Some(pct) => pct as u8,
        None if receipt.integrity_valid && !receipt.stale_vs_head => 100,
        None => 0,
    };

    // Build active claim views for overlap contention detection via existing `fleet_coord`.
    let active_claims: Vec<ActiveClaimView> = tasks
        .iter()
        .filter(|t| !t.assigned_to.trim().is_empty() || !t.owner.trim().is_empty())
        .filter(|t| {
            !matches!(
                t.status.to_ascii_lowercase().as_str(),
                "done" | "completed" | "closed"
            )
        })
        .map(|t| ActiveClaimView {
            task_id: t.id.clone(),
            agent_id: if !t.assigned_to.trim().is_empty() {
                t.assigned_to.clone()
            } else {
                t.owner.clone()
            },
            scope: t.scope.clone(),
            category: String::new(),
            dir_path: t.dir_path.clone(),
            lease_expires_at: t.lease_expires_at.clone(),
            lease_state: LeaseState::Active,
            assigned_at: Some(t.updated_at.clone()),
            lease_generation: t.lease_generation.max(0) as u32,
            lease_lifecycle: LeaseLifecycle::parse(Some(&t.lease_lifecycle)),
            intent_anchor: t.intent_anchor.clone(),
            capacity_units: 1,
            dependency_readiness: Default::default(),
        })
        .collect();
    let overlaps = detect_fleet_overlaps(&active_claims, &now_ts);

    let completed_ids: BTreeSet<&str> = tasks
        .iter()
        .filter(|t| {
            matches!(
                t.status.to_ascii_lowercase().as_str(),
                "done" | "completed" | "closed"
            )
        })
        .map(|t| t.id.as_str())
        .collect();

    let trajectory_motion = traj.as_ref().map(trajectory::motion_state);
    let working_tree_dirty = inventory.total_observed > 0;
    let receipt_stale = !receipt.present
        || receipt.stale_vs_head
        || receipt.forged_or_tampered
        || working_tree_dirty;

    let mut workunits = Vec::new();
    for task in &tasks {
        let owner = if !task.assigned_to.trim().is_empty() {
            task.assigned_to.clone()
        } else {
            task.owner.clone()
        };
        let task_overlaps: Vec<String> = overlaps
            .iter()
            .filter(|o| o.task_id == task.id)
            .map(|o| format!("{}@{} ({})", o.task_id, o.agent_id, o.reason))
            .collect();
        let dep_blockers: Vec<String> = task
            .depends_on
            .iter()
            .filter(|dep| !completed_ids.contains(dep.as_str()))
            .cloned()
            .collect();
        let lease_expired = task
            .lease_expires_at
            .as_deref()
            .map(|exp| !exp.is_empty() && exp < now_ts.as_str())
            .unwrap_or(false);

        let is_traj_task = traj
            .as_ref()
            .and_then(|t| t.task_id.as_deref())
            .map(|tid| tid == task.id)
            .unwrap_or(false);
        let input = WorkunitClassificationInput {
            task_status: task.status.clone(),
            owner: owner.clone(),
            lease_lifecycle: task.lease_lifecycle.clone(),
            lease_expired,
            has_dependency_blockers: !dep_blockers.is_empty(),
            has_overlap_contention: !task_overlaps.is_empty(),
            validation_failed: receipt.present
                && (receipt.fail_count > 0 || receipt.forged_or_tampered),
            receipt_stale: is_traj_task && receipt_stale,
            trajectory_motion: if is_traj_task {
                trajectory_motion.clone()
            } else {
                None
            },
            unresolved_claims_count: if is_traj_task {
                unresolved_claims_count
            } else {
                0
            },
            proof_verified: receipt.integrity_valid
                && !receipt.stale_vs_head
                && !working_tree_dirty,
        };
        let (category, authority_class, reasons) = classify_workunit_activity(&input);

        let mut stale_inputs = Vec::new();
        if lease_expired {
            stale_inputs.push("lease_expired".to_string());
        }
        if is_traj_task && receipt.stale_vs_head {
            stale_inputs.push("validation_receipt_stale_vs_head".to_string());
        }
        if is_traj_task && working_tree_dirty {
            stale_inputs.push("uncommitted_working_tree_changes".to_string());
        }
        if !snapshot.consistency.consistent {
            stale_inputs.push("snapshot_inconsistent_during_read".to_string());
        }

        let phase_readiness = if category == WorkunitActivityCategory::Completed {
            "ready".to_string()
        } else if !dep_blockers.is_empty() || !task_overlaps.is_empty() {
            "blocked".to_string()
        } else if receipt.integrity_valid && !receipt.stale_vs_head && !working_tree_dirty {
            "validated".to_string()
        } else {
            "not_proven".to_string()
        };

        let mut source_refs = vec![format!(".decapod/data/decapod.db#tasks/{}", task.id)];
        if is_traj_task {
            source_refs.push(format!("{GOVERNANCE_PATH}#/sections/trajectory"));
        }
        if receipt.present {
            source_refs.push(format!("{GOVERNANCE_PATH}#/sections/validation"));
        }

        workunits.push(WorkunitStatusProjection {
            task_id: task.id.clone(),
            title: trace::redact_string(&task.title),
            category,
            authority_class,
            allowed_next_categories: category.allowed_transitions().to_vec(),
            owner: if owner.trim().is_empty() {
                None
            } else {
                Some(owner)
            },
            lease_generation: task.lease_generation,
            lease_lifecycle: task.lease_lifecycle.clone(),
            contention_overlaps: task_overlaps,
            dependency_blockers: dep_blockers,
            phase_readiness,
            validation_failure_count: receipt.fail_count,
            unresolved_claims_count: if is_traj_task {
                unresolved_claims_count
            } else {
                0
            },
            evidence_coverage_pct,
            stale_inputs,
            source_refs,
            reasons,
        });
    }

    let active_count = workunits
        .iter()
        .filter(|w| w.category == WorkunitActivityCategory::Active)
        .count();
    let waiting_count = workunits
        .iter()
        .filter(|w| w.category == WorkunitActivityCategory::Waiting)
        .count();
    let blocked_count = workunits
        .iter()
        .filter(|w| w.category == WorkunitActivityCategory::Blocked)
        .count();
    let drifting_count = workunits
        .iter()
        .filter(|w| w.category == WorkunitActivityCategory::Drifting)
        .count();
    let completed_count = workunits
        .iter()
        .filter(|w| w.category == WorkunitActivityCategory::Completed)
        .count();
    let abandoned_count = workunits
        .iter()
        .filter(|w| w.category == WorkunitActivityCategory::Abandoned)
        .count();

    let budget_pressure = if blocked_count > 0 && active_count >= 8 {
        CapacityPressure::Critical
    } else if active_count >= 6 || inventory.truncated {
        CapacityPressure::High
    } else if active_count >= 3 {
        CapacityPressure::Medium
    } else if active_count >= 1 {
        CapacityPressure::Low
    } else {
        CapacityPressure::None
    };

    let fleet_summary = FleetObservabilitySummary {
        total_workunits: workunits.len(),
        active_count,
        waiting_count,
        blocked_count,
        drifting_count,
        completed_count,
        abandoned_count,
        overlap_count: overlaps.len(),
        budget_pressure,
        stale_source_detected: !snapshot.consistency.consistent
            || receipt_stale
            || workunits.iter().any(|w| !w.stale_inputs.is_empty()),
    };

    let source_anchor = if let Some(ref anchor_path) = query.anchor_path {
        Some(read_source_anchor(
            repo_root,
            &snapshot,
            anchor_path,
            query.start_line,
            query.end_line,
            query.max_anchor_bytes,
        )?)
    } else {
        None
    };

    let evolution = WorkspaceEvolutionRecord {
        intent_lineage,
        claimed_effects,
        validation_receipt: receipt,
        trajectory_motion_state: trajectory_motion.map(|m| format!("{m:?}").to_ascii_lowercase()),
        working_tree_dirty,
        unresolved_claims_count,
    };

    Ok(WorkspaceObservationProjection {
        schema_version: OBSERVE_SCHEMA_VERSION.to_string(),
        read_only: true,
        authoritative: false,
        snapshot,
        fleet_summary,
        workunits,
        evolution,
        inventory,
        worktrees,
        source_anchor,
    })
}

/// Shared concise human renderer driven directly by the canonical machine
/// projection struct (#860).
pub fn render_workspace_human(proj: &WorkspaceObservationProjection) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "Decapod Workspace Observation ({})\n",
        proj.schema_version
    ));
    out.push_str(&format!(
        "  snapshot_id: {} (consistent={}, atomicity={})\n",
        proj.snapshot.snapshot_id,
        proj.snapshot.consistency.consistent,
        proj.snapshot.consistency.atomicity
    ));
    out.push_str(&format!(
        "  repo: {} | branch: {} | head: {}\n",
        proj.snapshot.repository.repo_id,
        proj.snapshot
            .worktree
            .branch
            .as_deref()
            .unwrap_or("detached"),
        proj.snapshot
            .worktree
            .head_commit
            .as_deref()
            .unwrap_or("unborn")
    ));
    out.push_str(&format!(
        "  baseline: {} (local={:?}, remote={:?}, merge_base={:?})\n",
        proj.snapshot.baseline.base_branch,
        proj.snapshot.baseline.local_main_sha,
        proj.snapshot.baseline.remote_main_sha,
        proj.snapshot.baseline.merge_base_sha
    ));
    out.push_str(&format!(
        "  fleet: total={} active={} waiting={} blocked={} drifting={} completed={} abandoned={} overlaps={} pressure={:?} stale_sources={}\n",
        proj.fleet_summary.total_workunits,
        proj.fleet_summary.active_count,
        proj.fleet_summary.waiting_count,
        proj.fleet_summary.blocked_count,
        proj.fleet_summary.drifting_count,
        proj.fleet_summary.completed_count,
        proj.fleet_summary.abandoned_count,
        proj.fleet_summary.overlap_count,
        proj.fleet_summary.budget_pressure,
        proj.fleet_summary.stale_source_detected
    ));
    out.push_str(&format!(
        "  inventory: total={} staged={} unstaged={} untracked={} renamed={} deleted={} secret_blocked={} symlink_blocked={}\n",
        proj.inventory.total_observed,
        proj.inventory.staged_count,
        proj.inventory.unstaged_count,
        proj.inventory.untracked_count,
        proj.inventory.renamed_count,
        proj.inventory.deleted_count,
        proj.inventory.secret_blocked_count,
        proj.inventory.symlink_blocked_count
    ));
    for w in &proj.workunits {
        out.push_str(&format!(
            "  - [{}|{:?}] {} ({}) owner={} readiness={} coverage={}%\n",
            w.category.as_str(),
            w.authority_class,
            w.task_id,
            w.title,
            w.owner.as_deref().unwrap_or("unassigned"),
            w.phase_readiness,
            w.evidence_coverage_pct
        ));
    }
    out
}

/// Ensure TaskRecordView is kept accessible for callers of workspace helpers.
pub fn summarize_task_ids(tasks: &[TaskRecordView]) -> Vec<String> {
    tasks.iter().map(|t| t.id.clone()).collect()
}
