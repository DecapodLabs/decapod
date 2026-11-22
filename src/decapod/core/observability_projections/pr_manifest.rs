//! Prospective PR manifest and verified publication correspondence projection
//! (#1731, #1723).

use crate::core::error::DecapodError;
use crate::core::governance_document::{self, GOVERNANCE_PATH};
use crate::core::observability_projections::snapshot::{
    OBSERVE_SCHEMA_VERSION, ObservationSnapshot, capture_observation_with,
    enumerate_live_workspace_inventory, run_git_stdout_trimmed,
};
use crate::core::observability_projections::specs::inspect_validation_receipt;
use crate::core::observability_projections::validation_plan::{
    OverallReadinessVerdict, ValidationPlanQuery, project_validation_plan,
};
use crate::plan_governance::GovernedPlan;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PrManifestQuery {
    /// Optional explicit observed provider PR record for offline verification
    /// when not yet persisted inside `.decapod/governance.json`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_published_pr: Option<PublishedPrInput>,
    /// Optional planned scope file list to compare against live workspace diff.
    #[serde(default)]
    pub planned_scope_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublishedPrInput {
    pub pr_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr_number: Option<u64>,
    pub published_head_sha: String,
    pub published_base_sha: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publication_receipt_hash: Option<String>,
    #[serde(default)]
    pub admitted_proof_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validated_check_head_sha: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProspectivePrManifest {
    /// Always `"agent_intent"` — a prospective PR description is never provider
    /// state or admitted proof (#1731).
    pub authority_class: String,
    /// Never labels an auto-approved or draft plan as human-approved (#1731).
    pub plan_approval_label: String,
    pub proposed_title: String,
    pub proposed_summary: String,
    pub target_base_branch: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_base_sha: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_revision: Option<String>,
    pub included_tasks: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub included_branch: Option<String>,
    pub commits_since_base: Vec<String>,
    pub changed_files: Vec<String>,
    pub uncommitted_files: Vec<String>,
    pub validation_requirements: Vec<String>,
    pub readiness_verdict: OverallReadinessVerdict,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublishedPrRecord {
    /// Always `"provider_state"` — distinct from agent intent and admitted proof.
    pub authority_class: String,
    pub pr_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr_number: Option<u64>,
    pub published_head_sha: String,
    pub published_base_sha: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publication_receipt_hash: Option<String>,
    /// Separate verified outcome: `"admitted"`, `"not_admitted"`, `"quarantined"`, `"unknown"`.
    pub admitted_proof_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PrCorrespondenceState {
    PlanAbsent,
    UnpublishedPlanOnly,
    Corresponds,
    Drifted,
    Superseded,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum PrDriftFlag {
    PlanAbsent,
    WorkspaceDiffersFromPlannedScope,
    TargetMainMoved,
    BranchRebased,
    FinalShaChanged,
    UncommittedEditsOmitted,
    CheckTiedToWrongHead,
    PrSuperseded,
    ValidationStaleOrMissing,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrCorrespondenceReport {
    pub state: PrCorrespondenceState,
    pub plan_absent: bool,
    /// Always true when `published_pr` is absent so a plan can never masquerade
    /// as a published PR (#1731).
    pub plan_is_not_published: bool,
    pub drift_flags: Vec<PrDriftFlag>,
    pub details: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrManifestProjection {
    pub schema_version: String,
    pub read_only: bool,
    pub authoritative: bool,
    pub snapshot: ObservationSnapshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prospective_pr: Option<ProspectivePrManifest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_pr: Option<PublishedPrRecord>,
    pub correspondence: PrCorrespondenceReport,
    pub source_ref: String,
}

fn load_published_pr_from_governance(
    doc: &governance_document::GovernanceDocument,
) -> Option<PublishedPrInput> {
    let val = doc
        .sections
        .get("publication")
        .or_else(|| doc.sections.get("pr_publication"))?;
    let url = val
        .get("pr_url")
        .or_else(|| val.get("url"))
        .and_then(Value::as_str)?;
    let head = val
        .get("published_head_sha")
        .or_else(|| val.get("head_sha"))
        .and_then(Value::as_str)?;
    let base = val
        .get("published_base_sha")
        .or_else(|| val.get("base_sha"))
        .and_then(Value::as_str)
        .unwrap_or("");
    Some(PublishedPrInput {
        pr_url: url.to_string(),
        pr_number: val.get("pr_number").and_then(Value::as_u64),
        published_head_sha: head.to_string(),
        published_base_sha: base.to_string(),
        publication_receipt_hash: val
            .get("publication_receipt_hash")
            .or_else(|| val.get("receipt_hash"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        admitted_proof_status: val
            .get("admitted_proof_status")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string(),
        superseded_by: val
            .get("superseded_by")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        validated_check_head_sha: val
            .get("validated_check_head_sha")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
    })
}

/// Project prospective PR manifest and verify correspondence against published
/// provider state and live validation receipts (#1731).
pub fn project_pr_manifest(
    repo_root: &Path,
    query: &PrManifestQuery,
) -> Result<PrManifestProjection, DecapodError> {
    let (snapshot, doc) = capture_observation_with(repo_root, |root, _| {
        Ok(governance_document::load(root).ok().flatten())
    })?;

    let inventory = enumerate_live_workspace_inventory(repo_root, &snapshot.snapshot_id, 0, 512);
    let validation_plan = project_validation_plan(repo_root, &ValidationPlanQuery::default())?;
    let receipt =
        inspect_validation_receipt(doc.as_ref(), snapshot.worktree.head_commit.as_deref());

    let plan_opt: Option<GovernedPlan> = doc
        .as_ref()
        .and_then(|d| d.sections.get("plan"))
        .and_then(|v| serde_json::from_value(v.clone()).ok());

    let commits_since_base: Vec<String> = snapshot
        .baseline
        .merge_base_sha
        .as_deref()
        .and_then(|mb| {
            run_git_stdout_trimmed(
                repo_root,
                &["rev-list", "--max-count=64", &format!("{mb}..HEAD")],
            )
        })
        .map(|s| s.lines().map(ToOwned::to_owned).collect())
        .unwrap_or_default();

    let changed_files: Vec<String> = snapshot
        .baseline
        .merge_base_sha
        .as_deref()
        .and_then(|mb| {
            run_git_stdout_trimmed(repo_root, &["diff", "--name-only", &format!("{mb}..HEAD")])
        })
        .map(|s| {
            s.lines()
                .filter(|l| !l.trim().is_empty())
                .take(256)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default();

    let uncommitted_files: Vec<String> = inventory.entries.iter().map(|e| e.path.clone()).collect();

    let prospective_pr = plan_opt.as_ref().map(|plan| {
        let state_str = format!("{:?}", plan.state).to_ascii_uppercase();
        let plan_approval_label = match state_str.as_str() {
            "APPROVED" | "EXECUTING" | "DONE" => "agent_plan_approved_not_human_authenticated",
            _ => "draft_unapproved",
        }
        .to_string();
        ProspectivePrManifest {
            authority_class: "agent_intent".to_string(),
            plan_approval_label,
            proposed_title: plan.title.clone(),
            proposed_summary: plan.intent.clone(),
            target_base_branch: doc
                .as_ref()
                .and_then(|d| d.change.as_ref().map(|c| c.target_base.clone()))
                .unwrap_or_else(|| snapshot.baseline.base_branch.clone()),
            target_base_sha: snapshot.baseline.pinned_commit.clone(),
            change_id: doc
                .as_ref()
                .and_then(|d| d.change.as_ref().map(|c| c.id.clone())),
            plan_revision: validation_plan.candidate_identity.plan_revision.clone(),
            included_tasks: plan.todo_ids.clone(),
            included_branch: snapshot.worktree.branch.clone(),
            commits_since_base: commits_since_base.clone(),
            changed_files: changed_files.clone(),
            uncommitted_files: uncommitted_files.clone(),
            validation_requirements: plan.proof_hooks.clone(),
            readiness_verdict: validation_plan.overall_readiness,
        }
    });

    let pub_input = query
        .observed_published_pr
        .clone()
        .or_else(|| doc.as_ref().and_then(load_published_pr_from_governance));

    let published_pr = pub_input.as_ref().map(|p| PublishedPrRecord {
        authority_class: "provider_state".to_string(),
        pr_url: p.pr_url.clone(),
        pr_number: p.pr_number,
        published_head_sha: p.published_head_sha.clone(),
        published_base_sha: p.published_base_sha.clone(),
        publication_receipt_hash: p.publication_receipt_hash.clone(),
        admitted_proof_status: if p.admitted_proof_status.trim().is_empty() {
            "unknown".to_string()
        } else {
            p.admitted_proof_status.clone()
        },
        superseded_by: p.superseded_by.clone(),
    });

    let mut drift_flags: BTreeSet<PrDriftFlag> = BTreeSet::new();
    let mut details = Vec::new();
    let plan_absent = prospective_pr.is_none();

    if plan_absent {
        drift_flags.insert(PrDriftFlag::PlanAbsent);
        details.push("governed plan is absent from .decapod/governance.json".to_string());
    }

    // Detect uncommitted edits omitted from committed candidate / PR
    if !uncommitted_files.is_empty() {
        drift_flags.insert(PrDriftFlag::UncommittedEditsOmitted);
        details.push(format!(
            "{} uncommitted workspace edit(s) are not included in committed HEAD",
            uncommitted_files.len()
        ));
    }

    // Detect workspace diff diverging from planned scope
    if !query.planned_scope_files.is_empty() {
        let planned_set: BTreeSet<&str> = query
            .planned_scope_files
            .iter()
            .map(String::as_str)
            .collect();
        let observed_set: BTreeSet<&str> = changed_files
            .iter()
            .chain(uncommitted_files.iter())
            .map(String::as_str)
            .collect();
        let unexpected: Vec<&str> = observed_set.difference(&planned_set).copied().collect();
        if !unexpected.is_empty() {
            drift_flags.insert(PrDriftFlag::WorkspaceDiffersFromPlannedScope);
            details.push(format!(
                "workspace touches paths outside planned scope: {}",
                unexpected.join(", ")
            ));
        }
    }

    // Detect target main/master movement
    if snapshot.baseline.baseline_moved_vs_change_base {
        drift_flags.insert(PrDriftFlag::TargetMainMoved);
        details.push(
            "accepted base branch commit advanced beyond recorded change.base_commit".to_string(),
        );
    }

    // Detect validation staleness or receipt bound to wrong HEAD
    if !receipt.present || !receipt.integrity_valid || receipt.fail_count > 0 {
        drift_flags.insert(PrDriftFlag::ValidationStaleOrMissing);
        details.push("valid current validation receipt is missing or failed".to_string());
    } else if receipt.stale_vs_head {
        drift_flags.insert(PrDriftFlag::CheckTiedToWrongHead);
        drift_flags.insert(PrDriftFlag::ValidationStaleOrMissing);
        details.push(format!(
            "validation receipt git_revision {:?} does not match current HEAD {:?}",
            receipt.git_revision, snapshot.worktree.head_commit
        ));
    }

    // Correspondence against published PR (if present)
    if let Some(ref pub_in) = pub_input {
        if pub_in.superseded_by.is_some() {
            drift_flags.insert(PrDriftFlag::PrSuperseded);
            details.push(format!(
                "published PR was superseded by {}",
                pub_in.superseded_by.as_deref().unwrap_or("another PR")
            ));
        }
        if let Some(ref cur_head) = snapshot.worktree.head_commit
            && cur_head != &pub_in.published_head_sha
        {
            drift_flags.insert(PrDriftFlag::FinalShaChanged);
            // Distinguish simple new commit vs rebase (published head not ancestor of current HEAD)
            let is_ancestor = run_git_stdout_trimmed(
                repo_root,
                &[
                    "merge-base",
                    "--is-ancestor",
                    &pub_in.published_head_sha,
                    cur_head,
                ],
            )
            .is_some();
            if !is_ancestor {
                drift_flags.insert(PrDriftFlag::BranchRebased);
                details.push(format!(
                    "branch rebased or diverged: published head {} is not an ancestor of HEAD {}",
                    pub_in.published_head_sha, cur_head
                ));
            } else {
                details.push(format!(
                    "local HEAD {} advanced beyond published PR head {}",
                    cur_head, pub_in.published_head_sha
                ));
            }
        }
        if let Some(ref cur_base) = snapshot.baseline.pinned_commit
            && !pub_in.published_base_sha.is_empty()
            && cur_base != &pub_in.published_base_sha
        {
            drift_flags.insert(PrDriftFlag::TargetMainMoved);
            details.push(format!(
                "target base SHA moved from published {} to {}",
                pub_in.published_base_sha, cur_base
            ));
        }
        if let Some(ref check_head) = pub_in.validated_check_head_sha
            && check_head != &pub_in.published_head_sha
        {
            drift_flags.insert(PrDriftFlag::CheckTiedToWrongHead);
            details.push(format!(
                "published PR check head {} does not match published PR head {}",
                check_head, pub_in.published_head_sha
            ));
        }
    }

    let plan_is_not_published = published_pr.is_none();
    let state = if drift_flags.contains(&PrDriftFlag::PrSuperseded) {
        PrCorrespondenceState::Superseded
    } else if plan_absent && published_pr.is_none() {
        PrCorrespondenceState::PlanAbsent
    } else if published_pr.is_none() {
        PrCorrespondenceState::UnpublishedPlanOnly
    } else if drift_flags.is_empty() {
        PrCorrespondenceState::Corresponds
    } else {
        PrCorrespondenceState::Drifted
    };

    Ok(PrManifestProjection {
        schema_version: OBSERVE_SCHEMA_VERSION.to_string(),
        read_only: true,
        authoritative: false,
        snapshot,
        prospective_pr,
        published_pr,
        correspondence: PrCorrespondenceReport {
            state,
            plan_absent,
            plan_is_not_published,
            drift_flags: drift_flags.into_iter().collect(),
            details,
        },
        source_ref: GOVERNANCE_PATH.to_string(),
    })
}
