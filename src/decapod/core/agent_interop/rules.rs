//! Machine-readable coding-rule taxonomy & gap matrix (#1724), canonical coding-rule
//! metadata & applicability/precedence evaluator (#1725), and semantic 6-state task-scope
//! overlap classifier & combined-integration proof report (#878).
//!
//! Governance constraints:
//! - No third-party product names in the coding-rule taxonomy or committed rule text.
//! - Kernel enforcement always takes precedence over imported instructions.
//! - Separate worktrees never imply semantic compatibility; combined integration proof is
//!   reported separately from task-local proof.

use super::registry::{AuthorityScope, scan_markdown_for_injection};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

// ============================================================================
// #1724: Machine-Readable Coding-Rule Taxonomy & Gap Analysis Matrix
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleTaxonomyDomain {
    RequirementsAndSpecs,
    AgentRiskPermissions,
    ChangeMinimizationAndDrift,
    ReproducibleBuilds,
    TestEvidenceAndControls,
    ClaimsIntegrity,
    SourceAndMetadataQuality,
    ContextStewardship,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GapDisposition {
    Supported,
    Partial,
    Missing,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleTaxonomyRow {
    pub behavior_id: String,
    pub domain: RuleTaxonomyDomain,
    pub candidate_behavior: String,
    pub existing_kernel_feature: String,
    pub related_issues: Vec<u32>,
    pub existing_tests: Vec<String>,
    pub missing_enforcement: String,
    pub proposed_smallest_impl: String,
    pub acceptance_evidence: String,
    pub disposition: GapDisposition,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejection_rationale: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodingRuleAuditMatrix {
    pub schema_version: String,
    pub domains_covered: Vec<RuleTaxonomyDomain>,
    pub rows: Vec<RuleTaxonomyRow>,
}

/// Deterministic, machine-readable coding-rule taxonomy and gap analysis matrix (#1724).
pub fn coding_rule_taxonomy_matrix() -> CodingRuleAuditMatrix {
    let domains_covered = vec![
        RuleTaxonomyDomain::RequirementsAndSpecs,
        RuleTaxonomyDomain::AgentRiskPermissions,
        RuleTaxonomyDomain::ChangeMinimizationAndDrift,
        RuleTaxonomyDomain::ReproducibleBuilds,
        RuleTaxonomyDomain::TestEvidenceAndControls,
        RuleTaxonomyDomain::ClaimsIntegrity,
        RuleTaxonomyDomain::SourceAndMetadataQuality,
        RuleTaxonomyDomain::ContextStewardship,
    ];
    let rows = vec![
        RuleTaxonomyRow {
            behavior_id: "req.scoped_living_specs".to_string(),
            domain: RuleTaxonomyDomain::RequirementsAndSpecs,
            candidate_behavior: "Maintain project-wide doctrine separately from feature-specific obligations and resumable checkpoints".to_string(),
            existing_kernel_feature: ".decapod/managed/specs/* + governance.json plan/trajectory".to_string(),
            related_issues: vec![1724, 1735],
            existing_tests: vec!["tests/specs_refresh_docs.rs".to_string()],
            missing_enforcement: "None for local living-spec materiality check".to_string(),
            proposed_smallest_impl: "Reuse existing specs.refresh and material-specs gate".to_string(),
            acceptance_evidence: "decapod validate rejects FINGERPRINT_ONLY_SPECS".to_string(),
            disposition: GapDisposition::Supported,
            rejection_rationale: None,
        },
        RuleTaxonomyRow {
            behavior_id: "risk.destructive_and_push_escalation".to_string(),
            domain: RuleTaxonomyDomain::AgentRiskPermissions,
            candidate_behavior: "Gate destructive operations, credential access, and external side effects under explicit approval and redaction".to_string(),
            existing_kernel_feature: "policy eval/approve + trace::redact_string + agent_eval prompt gate".to_string(),
            related_issues: vec![1724, 1718],
            existing_tests: vec!["tests/interop_conformance.rs".to_string()],
            missing_enforcement: "Imported fragment privilege-escalation denial before composition".to_string(),
            proposed_smallest_impl: "compose_fragments() authority-scope + tool allowlist denial (#1718)".to_string(),
            acceptance_evidence: "tests/agent_interop_conformance.rs::fragment_composition_denies_privilege_escalation".to_string(),
            disposition: GapDisposition::Partial,
            rejection_rationale: None,
        },
        RuleTaxonomyRow {
            behavior_id: "drift.change_minimization_and_overlap".to_string(),
            domain: RuleTaxonomyDomain::ChangeMinimizationAndDrift,
            candidate_behavior: "Detect concurrent task overlap, conflicting invariants, and manifest policy contradictions before implementation".to_string(),
            existing_kernel_feature: "workspace isolation + todo exclusive claims".to_string(),
            related_issues: vec![878, 1717, 1724],
            existing_tests: vec!["tests/agent_interop_conformance.rs".to_string()],
            missing_enforcement: "Semantic 6-state overlap classifier and canonical manifest conflict detector".to_string(),
            proposed_smallest_impl: "classify_task_overlap() (#878) + detect_contract_policy_conflicts() (#1717)".to_string(),
            acceptance_evidence: "tests/agent_interop_conformance.rs::task_overlap_six_states_and_combined_integration".to_string(),
            disposition: GapDisposition::Supported,
            rejection_rationale: None,
        },
        RuleTaxonomyRow {
            behavior_id: "build.reproducible_and_stale_binary".to_string(),
            domain: RuleTaxonomyDomain::ReproducibleBuilds,
            candidate_behavior: "Bind conformance and validation evidence to exact binary SHA, source commit, and dirty-tree state".to_string(),
            existing_kernel_feature: "dirty_classification + release_projection_drift + observed conformance receipt".to_string(),
            related_issues: vec![871, 1724],
            existing_tests: vec!["tests/agent_interop_conformance.rs".to_string()],
            missing_enforcement: "Observed-conformance receipt bound to binary/source/schema/transport".to_string(),
            proposed_smallest_impl: "ObservedConformanceReceipt + project_manifest_with_receipt() (#871)".to_string(),
            acceptance_evidence: "tests/agent_interop_conformance.rs::observed_conformance_receipt_rejects_stale_wrong_binary_and_self_declared".to_string(),
            disposition: GapDisposition::Supported,
            rejection_rationale: None,
        },
        RuleTaxonomyRow {
            behavior_id: "test.negative_controls_and_summary_fiction".to_string(),
            domain: RuleTaxonomyDomain::TestEvidenceAndControls,
            candidate_behavior: "Require negative controls, reject zero-assertion pass claims, and verify raw receipt bytes over summary-only digests".to_string(),
            existing_kernel_feature: "interop_conformance + independent consumer vector suite".to_string(),
            related_issues: vec![1724, 1733],
            existing_tests: vec!["tests/agent_interop_conformance.rs".to_string()],
            missing_enforcement: "Raw-receipt byte verification vs summary-fiction vector".to_string(),
            proposed_smallest_impl: "verify_envelope() raw receipt digest verification (#1733)".to_string(),
            acceptance_evidence: "tests/agent_interop_conformance.rs::independent_consumers_agree_on_all_conformance_vectors".to_string(),
            disposition: GapDisposition::Supported,
            rejection_rationale: None,
        },
        RuleTaxonomyRow {
            behavior_id: "claims.no_invented_capabilities_or_proof".to_string(),
            domain: RuleTaxonomyDomain::ClaimsIntegrity,
            candidate_behavior: "Verify capability manifests against actual dispatcher/guards and prevent self-attested verified states".to_string(),
            existing_kernel_feature: "core::interop::validate_manifest + advertised_rpc_sessions_match_the_actual_entry_guards".to_string(),
            related_issues: vec![871, 1724],
            existing_tests: vec!["tests/unit/core/interop_tests.rs".to_string()],
            missing_enforcement: "Two-way dispatcher-vs-authorization drift detector callable at runtime".to_string(),
            proposed_smallest_impl: "check_dispatcher_authorization_drift() (#871)".to_string(),
            acceptance_evidence: "tests/agent_interop_conformance.rs::dispatcher_authorization_two_way_drift_detection".to_string(),
            disposition: GapDisposition::Supported,
            rejection_rationale: None,
        },
        RuleTaxonomyRow {
            behavior_id: "meta.typed_rule_metadata_and_precedence".to_string(),
            domain: RuleTaxonomyDomain::SourceAndMetadataQuality,
            candidate_behavior: "Unify rule/skill discovery under typed metadata with precedence, conflict detection, license checks, and no-rule selection".to_string(),
            existing_kernel_feature: "agent_interop::registry + agent_interop::rules".to_string(),
            related_issues: vec![1715, 1724, 1725],
            existing_tests: vec!["tests/agent_interop_conformance.rs".to_string()],
            missing_enforcement: "Deterministic coding-rule metadata evaluator with prompt-injection rejection".to_string(),
            proposed_smallest_impl: "evaluate_rule_applicability() (#1725)".to_string(),
            acceptance_evidence: "tests/agent_interop_conformance.rs::rule_metadata_precedence_and_injection_rejection".to_string(),
            disposition: GapDisposition::Supported,
            rejection_rationale: None,
        },
        RuleTaxonomyRow {
            behavior_id: "ctx.bounded_context_bundle_stewardship".to_string(),
            domain: RuleTaxonomyDomain::ContextStewardship,
            candidate_behavior: "Compile minimal token-budgeted context bundles from validated capabilities rather than injecting unbounded prompt prose".to_string(),
            existing_kernel_feature: "context_capsule + select_skills_and_compile_bundle".to_string(),
            related_issues: vec![1716, 1724],
            existing_tests: vec!["tests/agent_interop_conformance.rs".to_string()],
            missing_enforcement: "Token-budgeted skill bundle compiler with A/B evaluation".to_string(),
            proposed_smallest_impl: "select_skills_and_compile_bundle() + evaluate_skill_ab_fixture() (#1716)".to_string(),
            acceptance_evidence: "tests/agent_interop_conformance.rs::skill_discovery_token_budget_and_ab_evaluation".to_string(),
            disposition: GapDisposition::Supported,
            rejection_rationale: None,
        },
        RuleTaxonomyRow {
            behavior_id: "rejected.mandatory_human_plan_clickthrough".to_string(),
            domain: RuleTaxonomyDomain::AgentRiskPermissions,
            candidate_behavior: "Require interactive human confirmation before every routine local edit or test run".to_string(),
            existing_kernel_feature: "bounded autonomy model (#1735)".to_string(),
            related_issues: vec![1724, 1735],
            existing_tests: vec![],
            missing_enforcement: "Intentionally omitted".to_string(),
            proposed_smallest_impl: "None (rejected)".to_string(),
            acceptance_evidence: "Documented rejection preserving bounded unattended execution".to_string(),
            disposition: GapDisposition::Rejected,
            rejection_rationale: Some("Conflicts with bounded autonomy and human escalation policy: routine implementation and validation proceed unattended while consequential authority gates remain enforced.".to_string()),
        },
        RuleTaxonomyRow {
            behavior_id: "rejected.bulk_unlicensed_prompt_import".to_string(),
            domain: RuleTaxonomyDomain::SourceAndMetadataQuality,
            candidate_behavior: "Automatically scrape and internalize external prompt collections into repository rules without license or injection review".to_string(),
            existing_kernel_feature: "CapabilityRegistry quarantine gate".to_string(),
            related_issues: vec![1715, 1718, 1724],
            existing_tests: vec!["tests/agent_interop_conformance.rs".to_string()],
            missing_enforcement: "Intentionally prohibited".to_string(),
            proposed_smallest_impl: "None (rejected)".to_string(),
            acceptance_evidence: "Unlicensed and unverified imports are quarantined by CapabilityRegistry::register".to_string(),
            disposition: GapDisposition::Rejected,
            rejection_rationale: Some("Crosses the repository trust boundary, introduces unverified licensing, and creates prompt-injection and policy-weakening risk.".to_string()),
        },
    ];

    CodingRuleAuditMatrix {
        schema_version: "1.0.0".to_string(),
        domains_covered,
        rows,
    }
}

// ============================================================================
// #1725: Canonical Coding-Rule Metadata & Applicability/Precedence Evaluator
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleArtifactKind {
    CanonicalSource,
    GeneratedManifest,
    RenderedProjection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleAuthorityTier {
    KernelImmutable = 0,
    OrganizationPolicy = 1,
    RepositoryRule = 2,
    ImportedAdvisory = 3,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodingRuleMetadata {
    pub id: String,
    pub purpose: String,
    pub triggers: Vec<String>,
    pub task_category: String,
    pub affected_tech: Vec<String>,
    pub supported_harnesses: Vec<String>,
    pub source_license: String,
    pub version: String,
    pub digest: String,
    pub scope: String,
    pub authority: RuleAuthorityTier,
    pub required_permissions: AuthorityScope,
    pub expected_evidence: Vec<String>,
    pub conflicts: Vec<String>,
    pub dependencies: Vec<String>,
    pub exception_policy: String,
    pub context_budget_tokens: usize,
    pub tests: Vec<String>,
    pub expires_at_epoch: Option<u64>,
    pub artifact_kind: RuleArtifactKind,
    pub weakens_kernel_policy: bool,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleEvaluationContext {
    pub task_intent: String,
    pub task_category: String,
    pub tech_stack: Vec<String>,
    pub harness: String,
    pub max_permissions: AuthorityScope,
    pub context_budget_tokens: usize,
    pub now_epoch: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleRejection {
    pub rule_id: String,
    pub code: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleEvaluationReport {
    pub selected_rule_ids: Vec<String>,
    pub no_rule_justified: bool,
    pub total_tokens_used: usize,
    pub rejected: Vec<RuleRejection>,
    pub precedence_notes: Vec<String>,
    pub kernel_enforcement_preserved: bool,
}

/// Evaluate typed coding-rule metadata deterministically with precedence, conflict
/// resolution, prompt-injection & privilege-escalation rejection, and honest no-rule
/// selection when none is justified (#1725).
pub fn evaluate_rule_applicability(
    rules: &[CodingRuleMetadata],
    ctx: &RuleEvaluationContext,
) -> RuleEvaluationReport {
    let by_id: BTreeMap<&str, &CodingRuleMetadata> =
        rules.iter().map(|r| (r.id.as_str(), r)).collect();
    let intent_lower = ctx.task_intent.to_ascii_lowercase();

    let mut rejected = Vec::new();
    let mut eligible: Vec<&CodingRuleMetadata> = Vec::new();

    for rule in rules {
        if rule.id.trim().is_empty()
            || rule.purpose.trim().is_empty()
            || rule.source_license.trim().is_empty()
            || rule.source_license.eq_ignore_ascii_case("unlicensed")
        {
            rejected.push(RuleRejection {
                rule_id: rule.id.clone(),
                code: "invalid_rule_metadata".to_string(),
                detail: "missing required id, purpose, or verified license".to_string(),
            });
            continue;
        }
        if rule.artifact_kind != RuleArtifactKind::CanonicalSource {
            rejected.push(RuleRejection {
                rule_id: rule.id.clone(),
                code: "non_canonical_rule_source".to_string(),
                detail: "only CanonicalSource rules may enter applicability selection; rendered projections are non-authoritative".to_string(),
            });
            continue;
        }
        if rule.weakens_kernel_policy && rule.authority != RuleAuthorityTier::KernelImmutable {
            rejected.push(RuleRejection {
                rule_id: rule.id.clone(),
                code: "policy_immutability_violation".to_string(),
                detail: "imported or repo rule attempted to weaken immutable kernel policy"
                    .to_string(),
            });
            continue;
        }
        if let Some(finding) = scan_markdown_for_injection(&rule.body) {
            rejected.push(RuleRejection {
                rule_id: rule.id.clone(),
                code: "prompt_injection_rejected".to_string(),
                detail: finding,
            });
            continue;
        }
        if rule.required_permissions > ctx.max_permissions {
            rejected.push(RuleRejection {
                rule_id: rule.id.clone(),
                code: "privilege_escalation_rejected".to_string(),
                detail: format!(
                    "rule requires {:?} but context permits at most {:?}",
                    rule.required_permissions, ctx.max_permissions
                ),
            });
            continue;
        }
        if let Some(exp) = rule.expires_at_epoch
            && ctx.now_epoch > exp
        {
            rejected.push(RuleRejection {
                rule_id: rule.id.clone(),
                code: "rule_expired".to_string(),
                detail: format!("rule expired at epoch {exp}, now={}", ctx.now_epoch),
            });
            continue;
        }
        if !rule
            .supported_harnesses
            .iter()
            .any(|h| h.eq_ignore_ascii_case(&ctx.harness) || h == "*")
        {
            continue;
        }
        let category_match = rule.task_category.eq_ignore_ascii_case(&ctx.task_category)
            || rule.task_category == "*";
        let tech_match = rule
            .affected_tech
            .iter()
            .any(|t| t == "*" || ctx.tech_stack.iter().any(|ct| ct.eq_ignore_ascii_case(t)));
        let trigger_match = rule
            .triggers
            .iter()
            .any(|tr| intent_lower.contains(&tr.to_ascii_lowercase()));

        if !(category_match && tech_match && trigger_match) {
            continue;
        }
        if rule
            .dependencies
            .iter()
            .any(|d| !by_id.contains_key(d.as_str()))
        {
            rejected.push(RuleRejection {
                rule_id: rule.id.clone(),
                code: "missing_rule_dependency".to_string(),
                detail: "one or more declared rule dependencies do not exist".to_string(),
            });
            continue;
        }
        eligible.push(rule);
    }

    // Precedence: lower RuleAuthorityTier wins (KernelImmutable < OrganizationPolicy < RepositoryRule < ImportedAdvisory),
    // tie-broken deterministically by id.
    eligible.sort_by(|a, b| a.authority.cmp(&b.authority).then_with(|| a.id.cmp(&b.id)));

    let mut selected_rule_ids = Vec::new();
    let mut selected_set: BTreeSet<String> = BTreeSet::new();
    let mut precedence_notes = Vec::new();
    let mut total_tokens_used = 0usize;

    for rule in eligible {
        let conflicts = rule.conflicts.iter().any(|c| selected_set.contains(c))
            || selected_set.iter().any(|sid| {
                by_id
                    .get(sid.as_str())
                    .is_some_and(|existing| existing.conflicts.contains(&rule.id))
            });
        if conflicts {
            precedence_notes.push(format!(
                "rule '{}' skipped due to conflict with higher-precedence selected rule",
                rule.id
            ));
            rejected.push(RuleRejection {
                rule_id: rule.id.clone(),
                code: "conflict_lower_precedence".to_string(),
                detail: "superseded by higher-precedence conflicting rule".to_string(),
            });
            continue;
        }
        if total_tokens_used.saturating_add(rule.context_budget_tokens) > ctx.context_budget_tokens
        {
            rejected.push(RuleRejection {
                rule_id: rule.id.clone(),
                code: "context_budget_exceeded".to_string(),
                detail: "adding rule would exceed context_budget_tokens".to_string(),
            });
            continue;
        }
        selected_set.insert(rule.id.clone());
        selected_rule_ids.push(rule.id.clone());
        total_tokens_used += rule.context_budget_tokens;
    }

    rejected.sort_by(|a, b| a.rule_id.cmp(&b.rule_id).then_with(|| a.code.cmp(&b.code)));

    RuleEvaluationReport {
        no_rule_justified: selected_rule_ids.is_empty(),
        selected_rule_ids,
        total_tokens_used,
        rejected,
        precedence_notes,
        kernel_enforcement_preserved: true,
    }
}

// ============================================================================
// #878: Semantic 6-State Task-Scope Overlap Classifier & Combined Integration Report
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskScopeDeclaration {
    pub task_id: String,
    pub agent_id: String,
    pub desired_outcome: String,
    pub affected_domains: Vec<String>,
    pub affected_modules: Vec<String>,
    /// Key -> required invariant assertion value. Different values on the same key indicate a
    /// conflicting invariant!
    pub invariants: BTreeMap<String, String>,
    /// Upstream task IDs this task depends on.
    pub dependencies: Vec<String>,
    pub integration_points: Vec<String>,
    pub predicted_touched_paths: Vec<String>,
    #[serde(default)]
    pub actual_diff_paths: Vec<String>,
}

/// The exact 6 semantic overlap states specified by issue #878.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlapState {
    ExactDuplicate,
    LikelyDuplicate,
    SafeOverlap,
    DependencyOverlap,
    ConflictingInvariant,
    UnknownOverlap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoordinationRecommendation {
    ParallelProceed,
    DependencyOrder,
    ExplicitCoordination,
    SerializeOrDeduplicate,
    BlockUntilResolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverlapClassification {
    pub task_a: String,
    pub task_b: String,
    pub state: OverlapState,
    pub recommendation: CoordinationRecommendation,
    pub shared_context_allowed: bool,
    pub isolated_workspaces_required: bool,
    pub requires_combined_integration_proof: bool,
    pub conflicting_invariants: Vec<String>,
    pub shared_modules: Vec<String>,
    pub shared_paths: Vec<String>,
    pub shared_integration_points: Vec<String>,
    pub dependency_order: Option<(String, String)>,
    pub uncertainty_note: Option<String>,
}

fn normalize_words(s: &str) -> BTreeSet<String> {
    s.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| w.len() >= 3)
        .map(|w| w.to_ascii_lowercase())
        .collect()
}

fn intersect(a: &[String], b: &[String]) -> Vec<String> {
    let set_a: BTreeSet<&str> = a.iter().map(String::as_str).collect();
    let set_b: BTreeSet<&str> = b.iter().map(String::as_str).collect();
    set_a
        .intersection(&set_b)
        .map(|s| (*s).to_string())
        .collect()
}

/// Classify two concurrent task-scope declarations into one of the 6 semantic overlap states (#878).
pub fn classify_task_overlap(
    a: &TaskScopeDeclaration,
    b: &TaskScopeDeclaration,
) -> OverlapClassification {
    let paths_a = if !a.actual_diff_paths.is_empty() {
        &a.actual_diff_paths
    } else {
        &a.predicted_touched_paths
    };
    let paths_b = if !b.actual_diff_paths.is_empty() {
        &b.actual_diff_paths
    } else {
        &b.predicted_touched_paths
    };

    let shared_modules = intersect(&a.affected_modules, &b.affected_modules);
    let shared_paths = intersect(paths_a, paths_b);
    let shared_integration_points = intersect(&a.integration_points, &b.integration_points);

    // 1. Conflicting invariants take highest precedence (hard semantic conflict).
    let mut conflicting_invariants = Vec::new();
    for (k, va) in &a.invariants {
        if let Some(vb) = b.invariants.get(k)
            && va != vb
        {
            conflicting_invariants.push(format!("{k}: '{va}' vs '{vb}'"));
        }
    }
    if !conflicting_invariants.is_empty() {
        return OverlapClassification {
            task_a: a.task_id.clone(),
            task_b: b.task_id.clone(),
            state: OverlapState::ConflictingInvariant,
            recommendation: CoordinationRecommendation::BlockUntilResolved,
            shared_context_allowed: true,
            isolated_workspaces_required: true,
            requires_combined_integration_proof: true,
            conflicting_invariants,
            shared_modules,
            shared_paths,
            shared_integration_points,
            dependency_order: None,
            uncertainty_note: None,
        };
    }

    // 2. Explicit dependency ordering.
    if b.dependencies.contains(&a.task_id) {
        return OverlapClassification {
            task_a: a.task_id.clone(),
            task_b: b.task_id.clone(),
            state: OverlapState::DependencyOverlap,
            recommendation: CoordinationRecommendation::DependencyOrder,
            shared_context_allowed: true,
            isolated_workspaces_required: true,
            requires_combined_integration_proof: true,
            conflicting_invariants: vec![],
            shared_modules,
            shared_paths,
            shared_integration_points,
            dependency_order: Some((a.task_id.clone(), b.task_id.clone())),
            uncertainty_note: None,
        };
    }
    if a.dependencies.contains(&b.task_id) {
        return OverlapClassification {
            task_a: a.task_id.clone(),
            task_b: b.task_id.clone(),
            state: OverlapState::DependencyOverlap,
            recommendation: CoordinationRecommendation::DependencyOrder,
            shared_context_allowed: true,
            isolated_workspaces_required: true,
            requires_combined_integration_proof: true,
            conflicting_invariants: vec![],
            shared_modules,
            shared_paths,
            shared_integration_points,
            dependency_order: Some((b.task_id.clone(), a.task_id.clone())),
            uncertainty_note: None,
        };
    }

    // 3. Unknown overlap when either task has empty scope declarations.
    if (a.affected_modules.is_empty() && paths_a.is_empty())
        || (b.affected_modules.is_empty() && paths_b.is_empty())
    {
        return OverlapClassification {
            task_a: a.task_id.clone(),
            task_b: b.task_id.clone(),
            state: OverlapState::UnknownOverlap,
            recommendation: CoordinationRecommendation::ExplicitCoordination,
            shared_context_allowed: true,
            isolated_workspaces_required: true,
            requires_combined_integration_proof: true,
            conflicting_invariants: vec![],
            shared_modules,
            shared_paths,
            shared_integration_points,
            dependency_order: None,
            uncertainty_note: Some(
                "One or both task-scope declarations omitted modules and predicted paths; overlap cannot be ruled out.".to_string(),
            ),
        };
    }

    // 4. Exact duplicate detection (normalized identical outcome + identical path/module target set).
    let norm_a = a.desired_outcome.trim().to_ascii_lowercase();
    let norm_b = b.desired_outcome.trim().to_ascii_lowercase();
    let paths_set_a: BTreeSet<&str> = paths_a.iter().map(String::as_str).collect();
    let paths_set_b: BTreeSet<&str> = paths_b.iter().map(String::as_str).collect();
    if !norm_a.is_empty() && norm_a == norm_b && paths_set_a == paths_set_b {
        return OverlapClassification {
            task_a: a.task_id.clone(),
            task_b: b.task_id.clone(),
            state: OverlapState::ExactDuplicate,
            recommendation: CoordinationRecommendation::SerializeOrDeduplicate,
            shared_context_allowed: true,
            isolated_workspaces_required: true,
            requires_combined_integration_proof: false,
            conflicting_invariants: vec![],
            shared_modules,
            shared_paths,
            shared_integration_points,
            dependency_order: None,
            uncertainty_note: None,
        };
    }

    // 5. Likely duplicate (high outcome token Jaccard >= 0.6 AND shared touched paths).
    let words_a = normalize_words(&a.desired_outcome);
    let words_b = normalize_words(&b.desired_outcome);
    let union_len = words_a.union(&words_b).count();
    let inter_len = words_a.intersection(&words_b).count();
    let jaccard_pct = (inter_len * 100).checked_div(union_len).unwrap_or(0);
    if jaccard_pct >= 60 && !shared_paths.is_empty() {
        return OverlapClassification {
            task_a: a.task_id.clone(),
            task_b: b.task_id.clone(),
            state: OverlapState::LikelyDuplicate,
            recommendation: CoordinationRecommendation::ExplicitCoordination,
            shared_context_allowed: true,
            isolated_workspaces_required: true,
            requires_combined_integration_proof: true,
            conflicting_invariants: vec![],
            shared_modules,
            shared_paths,
            shared_integration_points,
            dependency_order: None,
            uncertainty_note: Some(format!(
                "High outcome similarity ({jaccard_pct}%) with overlapping touched paths."
            )),
        };
    }

    // 6. Safe overlap: distinct outcomes & compatible invariants. Even when modules or
    // integration points overlap, allow parallel work in isolated workspaces without false
    // serialization, while requiring combined integration proof if boundaries/paths overlap.
    let requires_combined = !shared_paths.is_empty()
        || !shared_integration_points.is_empty()
        || !shared_modules.is_empty();
    OverlapClassification {
        task_a: a.task_id.clone(),
        task_b: b.task_id.clone(),
        state: OverlapState::SafeOverlap,
        recommendation: CoordinationRecommendation::ParallelProceed,
        shared_context_allowed: true,
        isolated_workspaces_required: true,
        requires_combined_integration_proof: requires_combined,
        conflicting_invariants: vec![],
        shared_modules,
        shared_paths,
        shared_integration_points,
        dependency_order: None,
        uncertainty_note: None,
    }
}

/// Post-workspace combined integration proof status (#878).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProofOutcome {
    Passed,
    Failed,
    NotRun,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombinedIntegrationReport {
    pub classification_after_diffs: OverlapClassification,
    pub textual_merge_clean: bool,
    pub task_a_local_proof: ProofOutcome,
    pub task_b_local_proof: ProofOutcome,
    pub combined_integration_proof: ProofOutcome,
    pub publication_admissible: bool,
    pub reason: String,
}

/// Re-evaluate overlap after diffs exist and combine task-local vs integrated proof (#878).
/// Clean textual merges with semantic integration failures are explicitly denied!
pub fn evaluate_combined_integration(
    a: &TaskScopeDeclaration,
    b: &TaskScopeDeclaration,
    textual_merge_clean: bool,
    task_a_local_proof: ProofOutcome,
    task_b_local_proof: ProofOutcome,
    combined_integration_proof: ProofOutcome,
) -> CombinedIntegrationReport {
    let classification = classify_task_overlap(a, b);
    let (admissible, reason) = if classification.state == OverlapState::ConflictingInvariant {
        (
            false,
            "conflicting task invariants block combined publication".to_string(),
        )
    } else if !textual_merge_clean {
        (
            false,
            "textual merge conflict requires resolution".to_string(),
        )
    } else if task_a_local_proof != ProofOutcome::Passed
        || task_b_local_proof != ProofOutcome::Passed
    {
        (
            false,
            "both task-local proofs must pass before combined admission".to_string(),
        )
    } else if classification.requires_combined_integration_proof
        && combined_integration_proof != ProofOutcome::Passed
    {
        (
            false,
            "clean textual merge is insufficient when shared boundaries/modules exist; combined integration proof must pass".to_string(),
        )
    } else {
        (
            true,
            "task-local and combined integration proofs satisfied".to_string(),
        )
    };

    CombinedIntegrationReport {
        classification_after_diffs: classification,
        textual_merge_clean,
        task_a_local_proof,
        task_b_local_proof,
        combined_integration_proof,
        publication_admissible: admissible,
        reason,
    }
}
