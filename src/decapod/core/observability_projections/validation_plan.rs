//! Read-only validation-plan, requirement coverage, joint/integration checks,
//! and global constraint-relation consistency projection (#1730).

use crate::core::error::DecapodError;
use crate::core::governance_document::{self, GOVERNANCE_PATH};
use crate::core::observability_projections::snapshot::{
    OBSERVE_SCHEMA_VERSION, ObservationSnapshot, capture_observation_with,
    enumerate_live_workspace_inventory,
};
use crate::core::observability_projections::specs::{
    extract_intent_lineage, inspect_validation_receipt,
};
use crate::core::validation_rules::{
    EvaluationPhase, RuleAuthority, RuleId, RuleRemediation, RuleSeverity, RuleVerdict,
};
use crate::plan_governance::GovernedPlan;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ValidationPlanQuery {
    /// Optional declared accepted requirement IDs (e.g. `["R1", "R2", "R3"]`)
    /// to verify decomposition coverage (#1730 / #1768).
    #[serde(default)]
    pub accepted_requirements: Vec<String>,
    /// Optional declared joint/integration check specifications (#1730).
    #[serde(default)]
    pub joint_checks: Vec<JointCheckSpec>,
    /// Optional accepted equality/inequality constraints for global relation
    /// consistency verification (#1730 / #1770).
    #[serde(default)]
    pub constraint_relations: Vec<ConstraintRelation>,
    /// Optional assertion count reported by test evidence (0 triggers
    /// `ZERO_ASSERTION_SUITE` so empty tests never imply readiness!).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_assertion_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JointCheckSpec {
    pub check_id: String,
    pub participating_tasks: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_head: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_base: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintRelationKind {
    Equal,
    NotEqual,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConstraintRelation {
    pub lhs: String,
    pub rhs: String,
    pub kind: ConstraintRelationKind,
    pub source_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConstraintContradiction {
    pub lhs: String,
    pub rhs: String,
    pub detail: String,
    pub source_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConstraintConsistencyReport {
    pub status: String,
    pub total_constraints: usize,
    pub pairwise_feasible_does_not_imply_global: bool,
    pub contradictions: Vec<ConstraintContradiction>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OverallReadinessVerdict {
    Ready,
    NotReady,
    NotProven,
    Stale,
    Blocked,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceFreshness {
    Fresh,
    Stale,
    Missing,
    Tampered,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CandidateIdentity {
    pub head_commit: Option<String>,
    pub base_commit: Option<String>,
    pub working_tree_dirty: bool,
    pub governance_doc_sha256: Option<String>,
    pub plan_revision: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApplicableRuleProjection {
    pub rule_id: RuleId,
    pub description: String,
    pub authority: RuleAuthority,
    pub phase: EvaluationPhase,
    pub severity: RuleSeverity,
    pub verdict: RuleVerdict,
    pub freshness: EvidenceFreshness,
    pub exact_candidate_match: bool,
    pub evidence_source: String,
    pub remediation: RuleRemediation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RequirementCoverageEntry {
    pub requirement_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derived_claim: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegated_owner: Option<String>,
    pub required_checks: Vec<String>,
    pub proof_refs: Vec<String>,
    pub status: String,
    pub missing_mapping: bool,
    pub missing_acceptance_criteria: bool,
    pub zero_assertion_warning: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlannedCheckProjection {
    pub check_id: String,
    pub category: String,
    pub scope: String,
    pub status: String,
    pub exact_candidate_match: bool,
    pub freshness: EvidenceFreshness,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JointCheckProjection {
    pub check_id: String,
    pub participating_tasks: Vec<String>,
    pub combined_head: Option<String>,
    pub combined_base: Option<String>,
    pub producer: String,
    pub status: String,
    pub exact_candidate_match: bool,
    /// A clean Git merge never infers a passed joint check (#1730).
    pub clean_merge_does_not_prove: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationPlanProjection {
    pub schema_version: String,
    pub read_only: bool,
    pub authoritative: bool,
    pub snapshot: ObservationSnapshot,
    pub overall_readiness: OverallReadinessVerdict,
    pub individual_check_pass_implies_readiness: bool,
    pub semantic_assurance_status: String,
    pub candidate_identity: CandidateIdentity,
    pub applicable_rules: Vec<ApplicableRuleProjection>,
    pub requirement_coverage: Vec<RequirementCoverageEntry>,
    pub local_checks: Vec<PlannedCheckProjection>,
    pub joint_integration_checks: Vec<JointCheckProjection>,
    pub relation_consistency: ConstraintConsistencyReport,
    pub unresolved_decisions: Vec<String>,
    pub blockers: Vec<String>,
}

/// Evaluate global satisfiability of equality/inequality constraint relations
/// using union-find equivalence classes. Detects cyclic unsatisfiable systems
/// such as `A = B, B = C, A != C` even though every pair is individually
/// feasible (#1730 / #1770).
pub fn evaluate_constraint_relations(
    constraints: &[ConstraintRelation],
) -> ConstraintConsistencyReport {
    if constraints.is_empty() {
        return ConstraintConsistencyReport {
            status: "not_applicable".to_string(),
            total_constraints: 0,
            pairwise_feasible_does_not_imply_global: true,
            contradictions: Vec::new(),
        };
    }

    let mut parent: BTreeMap<String, String> = BTreeMap::new();
    let mut eq_sources: Vec<String> = Vec::new();

    fn find(parent: &mut BTreeMap<String, String>, x: &str) -> String {
        let p = parent
            .entry(x.to_string())
            .or_insert_with(|| x.to_string())
            .clone();
        if p == x {
            return p;
        }
        let root = find(parent, &p);
        parent.insert(x.to_string(), root.clone());
        root
    }

    for rel in constraints {
        if rel.kind == ConstraintRelationKind::Equal {
            let ra = find(&mut parent, &rel.lhs);
            let rb = find(&mut parent, &rel.rhs);
            if ra != rb {
                parent.insert(ra, rb);
            }
            eq_sources.push(rel.source_ref.clone());
        }
    }

    let mut contradictions = Vec::new();
    for rel in constraints {
        if rel.kind == ConstraintRelationKind::NotEqual {
            let ra = find(&mut parent, &rel.lhs);
            let rb = find(&mut parent, &rel.rhs);
            if ra == rb {
                let mut sources = eq_sources.clone();
                sources.push(rel.source_ref.clone());
                sources.sort();
                sources.dedup();
                contradictions.push(ConstraintContradiction {
                    lhs: rel.lhs.clone(),
                    rhs: rel.rhs.clone(),
                    detail: format!(
                        "global contradiction: '{}' and '{}' are forced equal by accepted equality chain yet constrained not-equal",
                        rel.lhs, rel.rhs
                    ),
                    source_refs: sources,
                });
            }
        }
    }

    ConstraintConsistencyReport {
        status: if contradictions.is_empty() {
            "passed".to_string()
        } else {
            "failed".to_string()
        },
        total_constraints: constraints.len(),
        pairwise_feasible_does_not_imply_global: true,
        contradictions,
    }
}

fn make_remediation(
    code: &str,
    desc: &str,
    expected: &str,
    observed: &str,
    path: &str,
) -> RuleRemediation {
    RuleRemediation {
        error_code: code.to_string(),
        violation_description: desc.to_string(),
        expected_condition: expected.to_string(),
        observed_condition: observed.to_string(),
        affected_paths: vec![path.to_string()],
        required_steps: vec![format!("Resolve {code} and rerun `decapod validate`")],
        autonomous_remediation_permitted: true,
        human_authorization_required: false,
        evidence_for_revalidation: vec!["Fresh validation receipt matching HEAD".to_string()],
        related_rules: Vec::new(),
    }
}

/// Project applicable validation rules, requirement coverage, local vs joint
/// checks, relation consistency, and honest readiness verdict (#1730).
pub fn project_validation_plan(
    repo_root: &Path,
    query: &ValidationPlanQuery,
) -> Result<ValidationPlanProjection, DecapodError> {
    let (snapshot, doc) = capture_observation_with(repo_root, |root, _| {
        Ok(governance_document::load(root).ok().flatten())
    })?;

    let inventory = enumerate_live_workspace_inventory(repo_root, &snapshot.snapshot_id, 0, 256);
    let working_tree_dirty = inventory.total_observed > 0;
    let plan_opt: Option<GovernedPlan> = doc
        .as_ref()
        .and_then(|d| d.sections.get("plan"))
        .and_then(|v| serde_json::from_value(v.clone()).ok());
    let lineage = extract_intent_lineage(doc.as_ref(), plan_opt.as_ref());
    let receipt =
        inspect_validation_receipt(doc.as_ref(), snapshot.worktree.head_commit.as_deref());

    let candidate_identity = CandidateIdentity {
        head_commit: snapshot.worktree.head_commit.clone(),
        base_commit: snapshot.baseline.pinned_commit.clone(),
        working_tree_dirty,
        governance_doc_sha256: snapshot.governance_doc_sha256.clone(),
        plan_revision: lineage.instruction_revision.clone(),
    };

    let mut blockers = Vec::new();
    let mut unresolved_decisions = Vec::new();
    unresolved_decisions.extend(lineage.unresolved_questions.clone());
    unresolved_decisions.extend(lineage.unresolved_contradictions.clone());
    if let Some(ref plan) = plan_opt {
        for r in &plan.spec_reviews {
            let disp = format!("{:?}", r.disposition).to_ascii_lowercase();
            if disp.contains("block") || disp.contains("unresolved") || disp.contains("reject") {
                unresolved_decisions.push(format!("spec_review:{}:{}", r.path, disp));
            }
        }
    }

    // 1. Applicable rules spanning all 4 authorities & phases (beyond tests!)
    let mut applicable_rules = Vec::new();

    // Kernel invariant: clean committed candidate & worktree isolation
    let isolation_ok = snapshot.worktree.worktree_present
        && snapshot
            .worktree
            .branch
            .as_deref()
            .map(|b| b != "main" && b != "master")
            .unwrap_or(true);
    applicable_rules.push(ApplicableRuleProjection {
        rule_id: RuleId::new("kernel.inv_root_isolation"),
        description: "Execution must occur on a non-protected isolated branch/worktree".to_string(),
        authority: RuleAuthority::KernelInvariant,
        phase: EvaluationPhase::PreExecution,
        severity: RuleSeverity::Critical,
        verdict: if isolation_ok {
            RuleVerdict::Pass
        } else {
            RuleVerdict::Fail
        },
        freshness: EvidenceFreshness::Fresh,
        exact_candidate_match: true,
        evidence_source: "git:symbolic-ref".to_string(),
        remediation: make_remediation(
            "INV_ROOT_ISOLATION",
            "Protected branch detected",
            "Isolated feature/agent branch",
            snapshot.worktree.branch.as_deref().unwrap_or("detached"),
            ".git/HEAD",
        ),
    });

    // Constitution: exact validation receipt bound to clean HEAD
    let (receipt_verdict, receipt_freshness) = if !receipt.present {
        blockers.push("MISSING_VALIDATION_RECEIPT".to_string());
        (RuleVerdict::Unverifiable, EvidenceFreshness::Missing)
    } else if receipt.forged_or_tampered {
        blockers.push("FORGED_VALIDATION_RECEIPT".to_string());
        (RuleVerdict::Fail, EvidenceFreshness::Tampered)
    } else if receipt.fail_count > 0 {
        blockers.push("VALIDATION_RECEIPT_FAILED".to_string());
        (RuleVerdict::Fail, EvidenceFreshness::Fresh)
    } else if receipt.stale_vs_head {
        blockers.push("STALE_RECEIPT_HEAD_MISMATCH".to_string());
        (RuleVerdict::Stale, EvidenceFreshness::Stale)
    } else if working_tree_dirty {
        blockers.push("DIRTY_WORKTREE_INVALIDATES_RECEIPT".to_string());
        (RuleVerdict::Stale, EvidenceFreshness::Stale)
    } else {
        (RuleVerdict::Pass, EvidenceFreshness::Fresh)
    };
    applicable_rules.push(ApplicableRuleProjection {
        rule_id: RuleId::new("constitution.exact_candidate_validation"),
        description: "Validation receipt must verify exact current clean HEAD without tampering"
            .to_string(),
        authority: RuleAuthority::Constitution,
        phase: EvaluationPhase::Validation,
        severity: RuleSeverity::Error,
        verdict: receipt_verdict.clone(),
        freshness: receipt_freshness,
        exact_candidate_match: receipt.integrity_valid
            && !receipt.stale_vs_head
            && !working_tree_dirty,
        evidence_source: format!("{GOVERNANCE_PATH}#/sections/validation"),
        remediation: make_remediation(
            "VALIDATION_RECEIPT_REQUIRED",
            "Missing, stale, or invalid validation receipt for current candidate",
            "Fresh untampered receipt matching clean HEAD",
            if working_tree_dirty {
                "uncommitted working tree edits present"
            } else {
                receipt.status.as_deref().unwrap_or("absent")
            },
            GOVERNANCE_PATH,
        ),
    });

    // Task constraints: intent clarity & unresolved decisions
    let intent_clear = unresolved_decisions.is_empty() && lineage.original_intent.is_some();
    if !unresolved_decisions.is_empty() {
        blockers.push("UNRESOLVED_INTENT_OR_SPEC_DECISIONS".to_string());
    }
    applicable_rules.push(ApplicableRuleProjection {
        rule_id: RuleId::new("task.recorded_instruction_clarity"),
        description: "Recorded intent must have no unresolved human questions or contradictions"
            .to_string(),
        authority: RuleAuthority::TaskConstraints,
        phase: EvaluationPhase::DuringExecution,
        severity: RuleSeverity::Error,
        verdict: if intent_clear {
            RuleVerdict::Pass
        } else if lineage.original_intent.is_none() {
            RuleVerdict::Unverifiable
        } else {
            RuleVerdict::Fail
        },
        freshness: EvidenceFreshness::Fresh,
        exact_candidate_match: true,
        evidence_source: format!("{GOVERNANCE_PATH}#/sections/plan"),
        remediation: make_remediation(
            "UNRESOLVED_DECISIONS",
            "Unresolved intent questions or contradictions remain open",
            "All human questions and contradictions resolved",
            &format!("{} unresolved item(s)", unresolved_decisions.len()),
            GOVERNANCE_PATH,
        ),
    });

    // Project override: publication correspondence & baseline freshness
    let baseline_fresh = !snapshot.baseline.baseline_moved_vs_change_base;
    if !baseline_fresh {
        blockers.push("TARGET_BASELINE_MOVED".to_string());
    }
    applicable_rules.push(ApplicableRuleProjection {
        rule_id: RuleId::new("project.publication_baseline_freshness"),
        description: "Candidate change base must match accepted target baseline before publication"
            .to_string(),
        authority: RuleAuthority::ProjectOverride,
        phase: EvaluationPhase::Publication,
        severity: RuleSeverity::Error,
        verdict: if baseline_fresh {
            RuleVerdict::Pass
        } else {
            RuleVerdict::Stale
        },
        freshness: if baseline_fresh {
            EvidenceFreshness::Fresh
        } else {
            EvidenceFreshness::Stale
        },
        exact_candidate_match: baseline_fresh,
        evidence_source: format!("{GOVERNANCE_PATH}#/change"),
        remediation: make_remediation(
            "BASELINE_MOVED",
            "Accepted base branch advanced beyond recorded change.base_commit",
            "Rebased/reconciled against current accepted target",
            "baseline_moved_vs_change_base=true",
            GOVERNANCE_PATH,
        ),
    });

    // 2. Requirement coverage (#1730 / #1768)
    let mut requirement_coverage = Vec::new();
    let zero_assertions = query.observed_assertion_count == Some(0);
    if zero_assertions {
        blockers.push("ZERO_ASSERTION_SUITE".to_string());
    }

    let mut mapped_ids: BTreeSet<String> = BTreeSet::new();
    if let Some(ref doc) = doc {
        for (claim_id, claim) in &doc.claims {
            mapped_ids.insert(claim_id.clone());
            let missing_criteria = claim.falsifier.trim().is_empty();
            if missing_criteria {
                blockers.push(format!("MISSING_ACCEPTANCE_CRITERIA:{claim_id}"));
            }
            let has_proof = !claim.proof_refs.is_empty();
            let status = if zero_assertions || missing_criteria {
                "not_proven"
            } else if !has_proof {
                "uncovered"
            } else if receipt_verdict == RuleVerdict::Pass
                && matches!(
                    claim.status.to_ascii_lowercase().as_str(),
                    "verified" | "proven"
                )
            {
                "proven"
            } else if receipt_verdict == RuleVerdict::Stale {
                "stale"
            } else {
                "not_proven"
            };
            if status != "proven" {
                blockers.push(format!("REQUIREMENT_NOT_PROVEN:{claim_id}"));
            }
            requirement_coverage.push(RequirementCoverageEntry {
                requirement_id: claim_id.clone(),
                source_revision: lineage.instruction_revision.clone(),
                derived_claim: Some(claim.statement.clone()),
                delegated_owner: plan_opt.as_ref().and_then(|p| p.todo_ids.first().cloned()),
                required_checks: plan_opt
                    .as_ref()
                    .map(|p| p.proof_hooks.clone())
                    .unwrap_or_default(),
                proof_refs: claim.proof_refs.clone(),
                status: status.to_string(),
                missing_mapping: false,
                missing_acceptance_criteria: missing_criteria,
                zero_assertion_warning: zero_assertions,
            });
        }
    }

    // Explicitly surface accepted requirements dropped during decomposition (e.g. R3)!
    for req_id in &query.accepted_requirements {
        let covered = mapped_ids
            .iter()
            .any(|cid| cid.eq_ignore_ascii_case(req_id) || cid.contains(req_id.as_str()));
        if !covered {
            blockers.push(format!("UNCOVERED_ACCEPTED_REQUIREMENT:{req_id}"));
            requirement_coverage.push(RequirementCoverageEntry {
                requirement_id: req_id.clone(),
                source_revision: lineage.instruction_revision.clone(),
                derived_claim: None,
                delegated_owner: None,
                required_checks: Vec::new(),
                proof_refs: Vec::new(),
                status: "uncovered".to_string(),
                missing_mapping: true,
                missing_acceptance_criteria: true,
                zero_assertion_warning: zero_assertions,
            });
        }
    }

    if requirement_coverage.is_empty() {
        blockers.push("MISSING_ACCEPTANCE_CRITERIA:no_claims_recorded".to_string());
    }

    // 3. Local checks vs Joint/Integration checks (#1730)
    let mut local_checks = Vec::new();
    let hooks = plan_opt
        .as_ref()
        .map(|p| p.proof_hooks.clone())
        .unwrap_or_else(|| vec!["decapod validate".to_string()]);
    for hook in hooks {
        let status = if zero_assertions {
            "not_proven"
        } else {
            match receipt_verdict {
                RuleVerdict::Pass => "passed",
                RuleVerdict::Fail => "failed",
                RuleVerdict::Stale => "stale",
                RuleVerdict::Unverifiable => "unrun",
            }
        };
        local_checks.push(PlannedCheckProjection {
            check_id: hook,
            category: "local_proof_hook".to_string(),
            scope: "single_worktree".to_string(),
            status: status.to_string(),
            exact_candidate_match: receipt_verdict == RuleVerdict::Pass,
            freshness: receipt_freshness,
            receipt_ref: receipt.receipt_hash.clone(),
        });
    }

    let mut joint_integration_checks = Vec::new();
    for jc in &query.joint_checks {
        let head_match = jc
            .observed_head
            .as_deref()
            .zip(snapshot.worktree.head_commit.as_deref())
            .map(|(obs, cur)| obs == cur)
            .unwrap_or(false);
        let base_match = jc
            .observed_base
            .as_deref()
            .zip(snapshot.baseline.pinned_commit.as_deref())
            .map(|(obs, cur)| obs == cur)
            .unwrap_or(true);
        let exact_match = head_match && base_match && !working_tree_dirty;
        let raw_status = jc
            .observed_status
            .as_deref()
            .unwrap_or("unrun")
            .to_ascii_lowercase();
        let status = if raw_status == "passed" && !exact_match {
            "stale"
        } else if raw_status.is_empty() {
            "unrun"
        } else {
            raw_status.as_str()
        };
        if status != "passed" {
            blockers.push(format!(
                "JOINT_CHECK_{}:{}",
                status.to_ascii_uppercase(),
                jc.check_id
            ));
        }
        joint_integration_checks.push(JointCheckProjection {
            check_id: jc.check_id.clone(),
            participating_tasks: jc.participating_tasks.clone(),
            combined_head: snapshot.worktree.head_commit.clone(),
            combined_base: snapshot.baseline.pinned_commit.clone(),
            producer: jc
                .producer
                .clone()
                .unwrap_or_else(|| "unassigned_joint_verifier".to_string()),
            status: status.to_string(),
            exact_candidate_match: exact_match,
            clean_merge_does_not_prove: true,
        });
    }

    // 4. Global constraint relation consistency (#1730 / #1770)
    let relation_consistency = evaluate_constraint_relations(&query.constraint_relations);
    if relation_consistency.status == "failed" {
        blockers.push("CONSTRAINT_RELATION_CONTRADICTION".to_string());
    }

    blockers.sort();
    blockers.dedup();

    let overall_readiness = if blockers.is_empty() && snapshot.consistency.consistent {
        OverallReadinessVerdict::Ready
    } else if blockers
        .iter()
        .any(|b| b.contains("FORGED") || b.contains("FAILED") || b.contains("CONTRADICTION"))
    {
        OverallReadinessVerdict::Blocked
    } else if blockers
        .iter()
        .any(|b| b.contains("STALE") || b.contains("DIRTY") || b.contains("MOVED"))
    {
        OverallReadinessVerdict::Stale
    } else if blockers.iter().any(|b| {
        b.contains("UNCOVERED")
            || b.contains("NOT_PROVEN")
            || b.contains("ZERO_ASSERTION")
            || b.contains("MISSING")
            || b.contains("UNRUN")
    }) {
        OverallReadinessVerdict::NotProven
    } else {
        OverallReadinessVerdict::NotReady
    };

    Ok(ValidationPlanProjection {
        schema_version: OBSERVE_SCHEMA_VERSION.to_string(),
        read_only: true,
        authoritative: false,
        snapshot,
        overall_readiness,
        individual_check_pass_implies_readiness: false,
        semantic_assurance_status: "not_proven".to_string(),
        candidate_identity,
        applicable_rules,
        requirement_coverage,
        local_checks,
        joint_integration_checks,
        relation_consistency,
        unresolved_decisions,
        blockers,
    })
}
